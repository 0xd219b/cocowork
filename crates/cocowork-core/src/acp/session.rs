//! Session management for ACP

use super::connection::AcpConnection;
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::warn;

// Type alias for backward compatibility
pub type AcpClient = AcpConnection;

/// Active session state
pub struct Session {
    pub id: String,
    pub task_id: String,
    pub agent_id: String,
    pub state: TaskState,
    pub client: Arc<AcpClient>,
}

impl Session {
    pub fn new(
        session_id: String,
        task_id: String,
        agent_id: String,
        prompt: Vec<ContentBlock>,
        working_directory: String,
        client: Arc<AcpClient>,
    ) -> Self {
        Self {
            id: session_id.clone(),
            task_id: task_id.clone(),
            agent_id: agent_id.clone(),
            state: TaskState::new(task_id, session_id, agent_id, prompt, working_directory),
            client,
        }
    }

    /// Process a session update notification
    pub fn process_update(&mut self, update: SessionUpdate) {
        self.state.updated_at = chrono::Utc::now();

        match update {
            SessionUpdate::Plan { entries } => {
                self.state.plan = entries;
                if self.state.status == TaskStatus::Pending {
                    self.state.status = TaskStatus::Planning;
                }
            }

            SessionUpdate::AgentMessageChunk { content } => {
                // Append to existing message or create new
                self.append_message(MessageBlock::agent(vec![content]));
            }

            SessionUpdate::UserMessageChunk { content } => {
                self.append_message(MessageBlock::user(vec![content]));
            }

            SessionUpdate::Thought { content } => {
                self.append_message(MessageBlock::thought(vec![content]));
            }

            SessionUpdate::ToolCall {
                tool_call_id,
                title,
                kind,
                status,
            } => {
                let tc = ToolCallState {
                    id: tool_call_id.clone(),
                    title,
                    kind,
                    status,
                    content: Vec::new(),
                    input: None,
                    output: None,
                    started_at: chrono::Utc::now(),
                    completed_at: None,
                };
                self.state.tool_calls.insert(tool_call_id, tc);
                self.state.status = TaskStatus::Executing;
            }

            SessionUpdate::ToolCallUpdate {
                tool_call_id,
                status,
                content,
            } => {
                let should_extract = if let Some(tc) = self.state.tool_calls.get_mut(&tool_call_id) {
                    tc.status = status;
                    if let Some(c) = content {
                        tc.content = c;
                    }
                    if status == ToolCallStatus::Completed {
                        tc.completed_at = Some(chrono::Utc::now());
                        true
                    } else {
                        false
                    }
                } else {
                    false
                };

                // Extract artifacts after releasing the mutable borrow
                if should_extract {
                    if let Some(tc) = self.state.tool_calls.get(&tool_call_id) {
                        self.extract_artifacts_from_tool_call_ref(tc.clone());
                    }
                }
            }

            SessionUpdate::CurrentModeUpdate { mode_id } => {
                self.state.context.current_mode = Some(mode_id);
            }

            SessionUpdate::AvailableCommandsUpdate { .. } => {
                // Store available commands if needed
            }

            SessionUpdate::PromptResponseReceived { stop_reason } => {
                // Internal notification - prompt response received
                if let Some(reason) = stop_reason {
                    self.state.stop_reason = Some(reason);
                    self.state.status = match reason {
                        crate::types::StopReason::EndTurn => TaskStatus::Completed,
                        crate::types::StopReason::Error => TaskStatus::Error,
                        crate::types::StopReason::Cancelled => TaskStatus::Cancelled,
                        crate::types::StopReason::MaxTokens => TaskStatus::Progressing,
                    };
                }
            }
        }
    }

    /// Handle prompt response (completion)
    pub fn handle_prompt_response(&mut self, response: PromptResponse) {
        self.state.stop_reason = Some(response.stop_reason);
        self.state.status = match response.stop_reason {
            StopReason::EndTurn => TaskStatus::Completed,
            StopReason::Cancelled => TaskStatus::Cancelled,
            StopReason::Error => TaskStatus::Error,
            StopReason::MaxTokens => TaskStatus::Completed,
        };
        self.state.updated_at = chrono::Utc::now();
    }

    /// Append a message, merging if possible
    fn append_message(&mut self, message: MessageBlock) {
        match message {
            MessageBlock::User { mut content, timestamp } => {
                if let Some(MessageBlock::User { content: last, .. }) =
                    self.state.messages.last_mut()
                {
                    last.append(&mut content);
                } else {
                    self.state
                        .messages
                        .push(MessageBlock::User { content, timestamp });
                }
            }
            MessageBlock::Agent { mut content, timestamp } => {
                if let Some(MessageBlock::Agent { content: last, .. }) =
                    self.state.messages.last_mut()
                {
                    last.append(&mut content);
                } else {
                    self.state
                        .messages
                        .push(MessageBlock::Agent { content, timestamp });
                }
            }
            MessageBlock::Thought { mut content, timestamp } => {
                if let Some(MessageBlock::Thought { content: last, .. }) =
                    self.state.messages.last_mut()
                {
                    last.append(&mut content);
                } else {
                    self.state
                        .messages
                        .push(MessageBlock::Thought { content, timestamp });
                }
            }
            MessageBlock::System { content, timestamp } => {
                self.state
                    .messages
                    .push(MessageBlock::System { content, timestamp });
            }
        }
    }

    /// Extract artifacts from a completed tool call (takes ownership to avoid borrow issues)
    fn extract_artifacts_from_tool_call_ref(&mut self, tc: ToolCallState) {
        self.extract_artifacts_from_tool_call(&tc);
    }

    /// Extract artifacts from a completed tool call
    fn extract_artifacts_from_tool_call(&mut self, tc: &ToolCallState) {
        // Check tool call kind and content for artifacts
        if let Some(kind) = &tc.kind {
            match kind {
                ToolCallKind::Write => {
                    // Look for file paths in tool call content/input
                    if let Some(input) = &tc.input {
                        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                            let artifact = Artifact::new_file_created(
                                self.state.id.clone(),
                                path.to_string(),
                                0, // Size unknown from tool call
                                String::new(),
                                ArtifactSource::from_acp(
                                    tc.id.clone(),
                                    "fs/write_file".to_string(),
                                ),
                            );
                            self.state.artifacts.push(artifact);
                        }
                    }
                }
                ToolCallKind::Delete => {
                    if let Some(input) = &tc.input {
                        if let Some(path) = input.get("path").and_then(|v| v.as_str()) {
                            let artifact = Artifact::new_file_deleted(
                                self.state.id.clone(),
                                path.to_string(),
                                ArtifactSource::from_acp(
                                    tc.id.clone(),
                                    "fs/delete_file".to_string(),
                                ),
                            );
                            self.state.artifacts.push(artifact);
                        }
                    }
                }
                ToolCallKind::Move => {
                    if let Some(input) = &tc.input {
                        let old_path = input.get("oldPath").and_then(|v| v.as_str());
                        let new_path = input.get("newPath").and_then(|v| v.as_str());
                        if let (Some(old), Some(new)) = (old_path, new_path) {
                            let artifact = Artifact::new_file_moved(
                                self.state.id.clone(),
                                old.to_string(),
                                new.to_string(),
                                0,
                                String::new(),
                                ArtifactSource::from_acp(tc.id.clone(), "fs/move_file".to_string()),
                            );
                            self.state.artifacts.push(artifact);
                        }
                    }
                }
                ToolCallKind::Execute => {
                    // Terminal output artifact
                    if let Some(output) = &tc.output {
                        let stdout = output
                            .get("stdout")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let command = tc.title.clone().unwrap_or_else(|| "command".to_string());

                        let artifact = Artifact::new_terminal_output(
                            self.state.id.clone(),
                            command,
                            stdout.to_string(),
                            ArtifactSource::from_terminal(tc.id.clone(), String::new()),
                        );
                        self.state.artifacts.push(artifact);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Session manager for handling multiple sessions
pub struct SessionManager {
    /// Active sessions by session ID
    sessions: HashMap<String, Session>,
    /// Session ID to task ID mapping
    session_to_task: HashMap<String, String>,
    /// Agent clients by agent ID
    clients: HashMap<String, Arc<AcpClient>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            session_to_task: HashMap::new(),
            clients: HashMap::new(),
        }
    }

    /// Register an agent client
    pub fn register_client(&mut self, agent_id: String, client: Arc<AcpClient>) {
        self.clients.insert(agent_id, client);
    }

    /// Get or create a client for an agent
    pub fn get_client(&self, agent_id: &str) -> Option<Arc<AcpClient>> {
        self.clients.get(agent_id).cloned()
    }

    /// Create a new session
    pub fn create_session(
        &mut self,
        session_id: String,
        task_id: String,
        agent_id: String,
        prompt: Vec<ContentBlock>,
        working_directory: String,
        client: Arc<AcpClient>,
    ) -> &Session {
        let session = Session::new(
            session_id.clone(),
            task_id.clone(),
            agent_id,
            prompt,
            working_directory,
            client,
        );

        self.session_to_task.insert(session_id.clone(), task_id);
        self.sessions.insert(session_id.clone(), session);
        self.sessions.get(&session_id).unwrap()
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&Session> {
        self.sessions.get(session_id)
    }

    /// Get a mutable session by ID
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(session_id)
    }

    /// Get session by task ID
    pub fn get_session_by_task(&self, task_id: &str) -> Option<&Session> {
        self.session_to_task
            .iter()
            .find(|(_, tid)| *tid == task_id)
            .and_then(|(sid, _)| self.sessions.get(sid))
    }

    /// Get mutable session by task ID
    pub fn get_session_by_task_mut(&mut self, task_id: &str) -> Option<&mut Session> {
        let session_id = self
            .session_to_task
            .iter()
            .find(|(_, tid)| *tid == task_id)
            .map(|(sid, _)| sid.clone());

        session_id.and_then(|sid| self.sessions.get_mut(&sid))
    }

    /// Remove a session
    pub fn remove_session(&mut self, session_id: &str) -> Option<Session> {
        self.session_to_task.remove(session_id);
        self.sessions.remove(session_id)
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Vec<SessionSummary> {
        self.sessions
            .values()
            .map(|s| SessionSummary {
                session_id: s.id.clone(),
                task_id: s.task_id.clone(),
                agent_id: s.agent_id.clone(),
                prompt_preview: s
                    .state
                    .prompt
                    .iter()
                    .filter_map(|c| match c {
                        ContentBlock::Text { text } => Some(text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(100)
                    .collect(),
                status: s.state.status,
                message_count: s.state.messages.len() as u32,
                tool_call_count: s.state.tool_calls.len() as u32,
                created_at: s.state.created_at,
                updated_at: s.state.updated_at,
            })
            .collect()
    }

    /// Process a session update notification
    pub fn process_update(&mut self, notification: SessionUpdateNotification) {
        if let Some(session) = self.sessions.get_mut(&notification.session_id) {
            session.process_update(notification.update);
        } else {
            warn!(
                "Received update for unknown session: {}",
                notification.session_id
            );
        }
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_manager_create_and_get() {
        let manager = SessionManager::new();

        // We need a mock client for testing
        // For now, just test the data structures

        assert!(manager.get_session("nonexistent").is_none());
    }

    #[test]
    fn test_task_state_creation() {
        let state = TaskState::new(
            "task-1".to_string(),
            "session-1".to_string(),
            "agent-1".to_string(),
            vec![ContentBlock::Text {
                text: "Test prompt".to_string(),
            }],
            "/home/user".to_string(),
        );

        assert_eq!(state.id, "task-1");
        assert_eq!(state.session_id, "session-1");
        assert_eq!(state.status, TaskStatus::Pending);
        assert!(!state.is_finished());
    }

    #[test]
    fn test_task_status_terminal_states() {
        assert!(!TaskStatus::Pending.is_terminal());
        assert!(!TaskStatus::Planning.is_terminal());
        assert!(!TaskStatus::Executing.is_terminal());
        assert!(TaskStatus::Completed.is_terminal());
        assert!(TaskStatus::Cancelled.is_terminal());
        assert!(TaskStatus::Error.is_terminal());
    }
}
