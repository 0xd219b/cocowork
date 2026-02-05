//! Core ACP traits for agent communication
//!
//! This module defines the core abstractions for ACP (Agent Client Protocol):
//! - `AgentServer` - Represents an agent server that can be connected to
//! - `AgentConnection` - An active connection to an agent
//! - `AgentClient` - Callback interface for handling agent requests

use crate::error::Result;
use crate::types::{
    ContentBlock, JsonRpcResponse, McpServerConfig, MessageBlock, SessionUpdateNotification,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::broadcast;

// ============================================================================
// Session Mode and Model IDs
// ============================================================================

/// Unique identifier for a session mode
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionModeId(pub String);

impl SessionModeId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for SessionModeId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SessionModeId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Unique identifier for a model
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelId(pub String);

impl ModelId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ModelId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ModelId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Unique identifier for a config option
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigOptionId(pub String);

impl ConfigOptionId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for ConfigOptionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ConfigOptionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

// ============================================================================
// Session Mode, Model, and Config
// ============================================================================

/// A session mode (e.g., "ask", "code", "architect")
#[derive(Debug, Clone)]
pub struct SessionMode {
    pub id: SessionModeId,
    pub name: String,
    pub description: Option<String>,
}

impl SessionMode {
    pub fn new(id: impl Into<SessionModeId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// A model that can be used in a session
#[derive(Debug, Clone)]
pub struct SessionModel {
    pub id: ModelId,
    pub name: String,
    pub description: Option<String>,
}

impl SessionModel {
    pub fn new(id: impl Into<ModelId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Type of a config option value
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigValueType {
    String,
    Number,
    Boolean,
    Select,
}

/// A configuration option for a session
#[derive(Debug, Clone)]
pub struct SessionConfigOption {
    pub id: ConfigOptionId,
    pub name: String,
    pub description: Option<String>,
    pub value_type: ConfigValueType,
    pub current_value: Option<String>,
    pub options: Option<Vec<String>>, // For Select type
}

impl SessionConfigOption {
    pub fn new(
        id: impl Into<ConfigOptionId>,
        name: impl Into<String>,
        value_type: ConfigValueType,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            value_type,
            current_value: None,
            options: None,
        }
    }
}

// ============================================================================
// Session Responses
// ============================================================================

/// Response from creating a new session
#[derive(Debug, Clone)]
pub struct NewSessionResponse {
    pub session_id: String,
    pub modes: Vec<SessionMode>,
    pub models: Vec<SessionModel>,
    pub config_options: Vec<SessionConfigOption>,
    pub current_mode: Option<SessionModeId>,
    pub current_model: Option<ModelId>,
}

impl NewSessionResponse {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            modes: Vec::new(),
            models: Vec::new(),
            config_options: Vec::new(),
            current_mode: None,
            current_model: None,
        }
    }
}

/// Response from loading an existing session
#[derive(Debug, Clone)]
pub struct LoadSessionResponse {
    pub session_id: String,
    pub modes: Vec<SessionMode>,
    pub models: Vec<SessionModel>,
    pub messages: Vec<MessageBlock>,
    pub current_mode: Option<SessionModeId>,
    pub current_model: Option<ModelId>,
}

impl LoadSessionResponse {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            modes: Vec::new(),
            models: Vec::new(),
            messages: Vec::new(),
            current_mode: None,
            current_model: None,
        }
    }
}

/// Information about a session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub title: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub message_count: u32,
}

/// Prompt message to send to agent
#[derive(Debug, Clone)]
pub struct PromptMessage {
    pub content: Vec<ContentBlock>,
    pub mode: Option<SessionModeId>,
}

impl PromptMessage {
    pub fn new(content: Vec<ContentBlock>) -> Self {
        Self {
            content,
            mode: None,
        }
    }

    pub fn with_mode(mut self, mode: impl Into<SessionModeId>) -> Self {
        self.mode = Some(mode.into());
        self
    }
}

/// Prompt response from agent
#[derive(Debug, Clone)]
pub struct PromptResult {
    pub stop_reason: crate::types::StopReason,
}

// ============================================================================
// Notifications
// ============================================================================

/// Notification types that can be received from an agent connection
#[derive(Debug, Clone)]
pub enum SessionNotification {
    /// Session update notification
    Update(SessionUpdateNotification),
    /// Connection closed
    Disconnected,
    /// Error occurred
    Error(String),
}

// ============================================================================
// Agent Server Command
// ============================================================================

/// Command configuration for spawning an agent
#[derive(Debug, Clone)]
pub struct AgentServerCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl AgentServerCommand {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

// ============================================================================
// Agent Server Trait
// ============================================================================

/// Represents an agent server that can be connected to.
///
/// This trait defines the interface for different AI agents (Claude Code, Gemini, etc.)
/// and provides methods to get agent metadata and establish connections.
#[async_trait]
pub trait AgentServer: Send + Sync {
    /// Get the agent's display name
    fn name(&self) -> &str;

    /// Get the agent's unique identifier
    fn id(&self) -> &str;

    /// Get the agent's icon name
    fn icon(&self) -> &str {
        "terminal"
    }

    /// Get default mode for the agent (if supported)
    fn default_mode(&self) -> Option<SessionModeId> {
        None
    }

    /// Get default model for the agent (if supported)
    fn default_model(&self) -> Option<ModelId> {
        None
    }

    /// Get the command to spawn the agent
    fn get_command(&self) -> Option<AgentServerCommand>;

    /// Get additional environment variables for the agent
    fn get_env(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Check if the agent is available (installed)
    async fn is_available(&self) -> bool;

    /// Connect to the agent and return a connection
    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>>;
}

// ============================================================================
// Agent Connection Trait
// ============================================================================

/// An active connection to an agent.
///
/// This trait defines the interface for communicating with a connected agent,
/// including session management, prompting, and mode/model configuration.
#[async_trait]
pub trait AgentConnection: Send + Sync {
    /// Create a new session
    async fn new_session(
        &self,
        cwd: std::path::PathBuf,
        mcp_servers: Vec<McpServerConfig>,
    ) -> Result<NewSessionResponse>;

    /// Load an existing session
    async fn load_session(
        &self,
        session_id: String,
        mcp_servers: Vec<McpServerConfig>,
    ) -> Result<LoadSessionResponse>;

    /// Send a prompt to a session
    async fn prompt(&self, session_id: String, message: PromptMessage) -> Result<PromptResult>;

    /// Send a prompt without waiting for completion (streaming)
    async fn prompt_streaming(&self, session_id: String, message: PromptMessage) -> Result<()>;

    /// Cancel a session
    async fn cancel(&self, session_id: String) -> Result<()>;

    /// Set the mode for a session
    async fn set_mode(&self, session_id: String, mode_id: SessionModeId) -> Result<()>;

    /// Set the model for a session
    async fn set_model(&self, session_id: String, model_id: ModelId) -> Result<()>;

    /// Set a config option for a session
    async fn set_config(
        &self,
        session_id: String,
        config_id: ConfigOptionId,
        value: String,
    ) -> Result<()>;

    /// List all sessions
    async fn list_sessions(&self) -> Result<Vec<SessionInfo>>;

    /// Subscribe to session update notifications
    fn subscribe_updates(&self) -> broadcast::Receiver<SessionNotification>;

    /// Check if connection is still active
    async fn is_running(&self) -> bool;

    /// Terminate the connection
    async fn terminate(&self) -> Result<()>;

    /// Send a raw response to the agent (for handling agent requests)
    async fn send_response(&self, response: JsonRpcResponse) -> Result<()>;
}

// ============================================================================
// Agent Client Trait (Callback Interface)
// ============================================================================

/// Callback interface for handling requests from the agent.
///
/// The agent may request file operations, terminal execution, or permissions.
/// This trait allows the host application to handle these requests.
#[async_trait]
pub trait AgentClient: Send + Sync {
    /// Read a text file
    async fn read_text_file(&self, session_id: &str, path: &str) -> Result<String>;

    /// Write a text file
    async fn write_text_file(&self, session_id: &str, path: &str, content: &str) -> Result<()>;

    /// List a directory
    async fn list_directory(&self, session_id: &str, path: &str) -> Result<Vec<crate::types::FileMetadata>>;

    /// Delete a file
    async fn delete_file(&self, session_id: &str, path: &str) -> Result<()>;

    /// Move/rename a file
    async fn move_file(&self, session_id: &str, old_path: &str, new_path: &str) -> Result<()>;

    /// Create a directory
    async fn create_directory(&self, session_id: &str, path: &str) -> Result<()>;

    /// Execute a terminal command
    async fn execute_command(
        &self,
        session_id: &str,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        env: Option<&HashMap<String, String>>,
    ) -> Result<crate::types::TerminalExecuteResult>;

    /// Request permission for an operation
    async fn request_permission(
        &self,
        session_id: &str,
        operation: &str,
        resource: &str,
    ) -> Result<bool>;

    /// Handle a session notification (for forwarding to UI)
    async fn on_session_notification(&self, notification: SessionNotification) -> Result<()>;
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_mode_id() {
        let id = SessionModeId::new("code");
        assert_eq!(id.as_str(), "code");

        let id2: SessionModeId = "ask".into();
        assert_eq!(id2.as_str(), "ask");
    }

    #[test]
    fn test_model_id() {
        let id = ModelId::new("claude-3-opus");
        assert_eq!(id.as_str(), "claude-3-opus");
    }

    #[test]
    fn test_session_mode() {
        let mode = SessionMode::new("code", "Code Mode")
            .with_description("Write and edit code");

        assert_eq!(mode.id.as_str(), "code");
        assert_eq!(mode.name, "Code Mode");
        assert_eq!(mode.description, Some("Write and edit code".to_string()));
    }

    #[test]
    fn test_new_session_response() {
        let response = NewSessionResponse::new("session-123");
        assert_eq!(response.session_id, "session-123");
        assert!(response.modes.is_empty());
    }

    #[test]
    fn test_prompt_message() {
        let msg = PromptMessage::new(vec![ContentBlock::Text { text: "Hello".into() }])
            .with_mode("code");

        assert_eq!(msg.mode.unwrap().as_str(), "code");
    }
}
