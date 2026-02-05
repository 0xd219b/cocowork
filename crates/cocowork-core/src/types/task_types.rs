//! Task state types for TaskStateAccumulator

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Task status state machine
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// User sent prompt, waiting for agent response
    Pending,
    /// Agent is creating/updating plan
    Planning,
    /// Agent is executing tool calls
    Executing,
    /// Partial completion, more work to do
    Progressing,
    /// Task completed successfully
    Completed,
    /// Task was cancelled
    Cancelled,
    /// Task encountered an error
    Error,
}

impl Default for TaskStatus {
    fn default() -> Self {
        Self::Pending
    }
}

impl TaskStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Error)
    }
}

/// Complete task state (the core accumulator structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskState {
    // Identity
    pub id: String,
    pub session_id: String,
    pub agent_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,

    // Status
    pub status: TaskStatus,
    pub stop_reason: Option<super::StopReason>,
    pub error_message: Option<String>,

    // User input
    pub prompt: Vec<super::ContentBlock>,
    pub working_directory: String,

    // Agent plan
    pub plan: Vec<super::PlanEntry>,

    // Conversation
    pub messages: Vec<super::MessageBlock>,

    // Tool calls
    pub tool_calls: HashMap<String, super::ToolCallState>,

    // Artifacts
    pub artifacts: Vec<super::Artifact>,

    // Context
    pub context: TaskContext,

    // File changes
    pub file_changes: Vec<super::FileChange>,
}

impl TaskState {
    pub fn new(
        id: String,
        session_id: String,
        agent_id: String,
        prompt: Vec<super::ContentBlock>,
        working_directory: String,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id,
            session_id,
            agent_id,
            created_at: now,
            updated_at: now,
            status: TaskStatus::Pending,
            stop_reason: None,
            error_message: None,
            prompt,
            working_directory: working_directory.clone(),
            plan: Vec::new(),
            messages: Vec::new(),
            tool_calls: HashMap::new(),
            artifacts: Vec::new(),
            context: TaskContext::new(working_directory),
            file_changes: Vec::new(),
        }
    }

    /// Check if task is in a terminal state
    pub fn is_finished(&self) -> bool {
        self.status.is_terminal()
    }

    /// Get duration of the task
    pub fn duration(&self) -> chrono::Duration {
        self.updated_at - self.created_at
    }

    /// Count completed tool calls
    pub fn completed_tool_calls(&self) -> usize {
        self.tool_calls
            .values()
            .filter(|tc| tc.status == super::ToolCallStatus::Completed)
            .count()
    }

    /// Count pending tool calls
    pub fn pending_tool_calls(&self) -> usize {
        self.tool_calls
            .values()
            .filter(|tc| matches!(tc.status, super::ToolCallStatus::Pending | super::ToolCallStatus::InProgress))
            .count()
    }
}

/// Task context snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskContext {
    pub working_directory: String,
    pub granted_paths: Vec<String>,
    pub mcp_servers: Vec<String>,
    pub agent_capabilities: Option<super::AgentCapabilities>,
    pub current_mode: Option<String>,
}

impl TaskContext {
    pub fn new(working_directory: String) -> Self {
        Self {
            working_directory,
            granted_paths: Vec::new(),
            mcp_servers: Vec::new(),
            agent_capabilities: None,
            current_mode: None,
        }
    }
}

/// Task summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub session_id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub prompt_preview: String,
    pub status: TaskStatus,
    pub artifact_count: u32,
    pub file_change_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&TaskState> for TaskSummary {
    fn from(state: &TaskState) -> Self {
        let prompt_preview = state
            .prompt
            .iter()
            .filter_map(|c| match c {
                super::ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ");

        let prompt_preview = if prompt_preview.len() > 100 {
            format!("{}...", &prompt_preview[..97])
        } else {
            prompt_preview
        };

        Self {
            id: state.id.clone(),
            session_id: state.session_id.clone(),
            agent_id: state.agent_id.clone(),
            agent_name: state.agent_id.clone(), // Will be resolved later
            prompt_preview,
            status: state.status,
            artifact_count: state.artifacts.len() as u32,
            file_change_count: state.file_changes.len() as u32,
            created_at: state.created_at,
            updated_at: state.updated_at,
        }
    }
}

/// UI event emitted from TaskStateAccumulator
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TaskUiEvent {
    PlanUpdated { task_id: String },
    MessageAppended { task_id: String },
    ThoughtAppended { task_id: String },
    ToolCallStarted { task_id: String, tool_call_id: String },
    ToolCallUpdated { task_id: String, tool_call_id: String },
    ArtifactCreated { task_id: String, artifact_id: String },
    FileChanged { task_id: String, path: String },
    StatusChanged { task_id: String, status: TaskStatus },
    ModeChanged { task_id: String, mode_id: String },
    TaskCompleted { task_id: String, stop_reason: super::StopReason },
    TaskError { task_id: String, error: String },
}
