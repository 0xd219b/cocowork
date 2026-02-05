//! Session and context types

use serde::{Deserialize, Serialize};

/// Session context combining all context dimensions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    /// ACP session ID
    pub session_id: String,
    /// Agent capabilities from initialization
    pub agent_capabilities: super::AgentCapabilities,
    /// Current agent mode
    pub current_mode: Option<String>,
    /// Available modes
    pub available_modes: Vec<super::AgentMode>,
    /// Conversation history
    pub conversation_history: Vec<MessageBlock>,
    /// Current plan
    pub active_plan: Vec<super::PlanEntry>,
    /// Tool call log
    pub tool_call_log: Vec<ToolCallState>,
    /// Estimated token usage
    pub total_tokens_estimate: u64,
}

impl SessionContext {
    pub fn new(session_id: String, agent_capabilities: super::AgentCapabilities) -> Self {
        Self {
            session_id,
            available_modes: agent_capabilities.available_modes.clone(),
            agent_capabilities,
            current_mode: None,
            conversation_history: Vec::new(),
            active_plan: Vec::new(),
            tool_call_log: Vec::new(),
            total_tokens_estimate: 0,
        }
    }
}

/// Environment context (client-side managed)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentContext {
    /// Working directory for the session
    pub working_directory: String,
    /// Granted file paths
    pub granted_paths: Vec<String>,
    /// MCP servers available to the session
    pub mcp_servers: Vec<super::McpServerConfig>,
    /// Platform information
    pub platform: String,
    /// Shell to use for terminal commands
    pub shell: String,
}

impl Default for EnvironmentContext {
    fn default() -> Self {
        let platform = std::env::consts::OS.to_string();
        let shell = if cfg!(target_os = "windows") {
            "cmd.exe".to_string()
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string())
        };

        Self {
            working_directory: dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string()),
            granted_paths: Vec::new(),
            mcp_servers: Vec::new(),
            platform,
            shell,
        }
    }
}

/// Message block in conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum MessageBlock {
    User {
        content: Vec<super::ContentBlock>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Agent {
        content: Vec<super::ContentBlock>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    Thought {
        content: Vec<super::ContentBlock>,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
    System {
        content: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    },
}

impl MessageBlock {
    pub fn user(content: Vec<super::ContentBlock>) -> Self {
        Self::User {
            content,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn agent(content: Vec<super::ContentBlock>) -> Self {
        Self::Agent {
            content,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn thought(content: Vec<super::ContentBlock>) -> Self {
        Self::Thought {
            content,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::System {
            content: content.into(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Self::User { timestamp, .. } => *timestamp,
            Self::Agent { timestamp, .. } => *timestamp,
            Self::Thought { timestamp, .. } => *timestamp,
            Self::System { timestamp, .. } => *timestamp,
        }
    }
}

/// Tool call state tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCallState {
    pub id: String,
    pub title: Option<String>,
    pub kind: Option<super::ToolCallKind>,
    pub status: super::ToolCallStatus,
    pub content: Vec<super::ToolCallContent>,
    pub input: Option<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl ToolCallState {
    pub fn new(id: String, title: Option<String>, kind: Option<super::ToolCallKind>) -> Self {
        Self {
            id,
            title,
            kind,
            status: super::ToolCallStatus::Pending,
            content: Vec::new(),
            input: None,
            output: None,
            started_at: chrono::Utc::now(),
            completed_at: None,
        }
    }

    pub fn duration(&self) -> Option<chrono::Duration> {
        self.completed_at.map(|end| end - self.started_at)
    }
}

/// Session summary for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: String,
    pub task_id: String,
    pub agent_id: String,
    pub prompt_preview: String,
    pub status: super::TaskStatus,
    pub message_count: u32,
    pub tool_call_count: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
