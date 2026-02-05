//! Agent configuration and state types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Agent configuration stored in database
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub icon: Option<String>,
    pub builtin: bool,
    pub enabled: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl AgentConfig {
    /// Create a new agent configuration
    pub fn new(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: id.into(),
            name: name.into(),
            description: None,
            command: command.into(),
            args: Vec::new(),
            env: HashMap::new(),
            icon: None,
            builtin: false,
            enabled: true,
            created_at: now,
            updated_at: now,
        }
    }

    /// Create built-in Claude Code agent config
    pub fn claude_code() -> Self {
        Self {
            id: "claude-code".to_string(),
            name: "Claude Code".to_string(),
            description: Some("Anthropic's Claude Code agent with full coding capabilities".to_string()),
            command: "claude".to_string(),
            args: vec!["--acp".to_string()],
            env: HashMap::new(),
            icon: Some("claude".to_string()),
            builtin: true,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Create built-in Gemini CLI agent config
    pub fn gemini_cli() -> Self {
        Self {
            id: "gemini-cli".to_string(),
            name: "Gemini CLI".to_string(),
            description: Some("Google's Gemini CLI with native ACP support".to_string()),
            command: "gemini".to_string(),
            args: vec!["--experimental-acp".to_string()],
            env: HashMap::new(),
            icon: Some("gemini".to_string()),
            builtin: true,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Create built-in Codex CLI agent config
    pub fn codex_cli() -> Self {
        Self {
            id: "codex-cli".to_string(),
            name: "Codex CLI".to_string(),
            description: Some("OpenAI's Codex CLI agent".to_string()),
            command: "codex".to_string(),
            args: vec!["--acp".to_string()],
            env: HashMap::new(),
            icon: Some("openai".to_string()),
            builtin: true,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Create built-in Goose agent config
    pub fn goose() -> Self {
        Self {
            id: "goose".to_string(),
            name: "Goose".to_string(),
            description: Some("Block's Goose agent with native ACP support".to_string()),
            command: "goose".to_string(),
            args: vec!["--acp".to_string()],
            env: HashMap::new(),
            icon: Some("goose".to_string()),
            builtin: true,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    /// Get all built-in agents
    pub fn builtin_agents() -> Vec<Self> {
        vec![
            Self::claude_code(),
            Self::gemini_cli(),
            Self::codex_cli(),
            Self::goose(),
        ]
    }
}

/// Agent runtime status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is not running
    Stopped,
    /// Agent is starting up
    Starting,
    /// Agent is running and ready
    Running,
    /// Agent is initializing (ACP handshake in progress)
    Initializing,
    /// Agent encountered an error
    Error,
    /// Agent is shutting down
    Stopping,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

/// Agent runtime state (in-memory only)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentState {
    pub config: AgentConfig,
    pub status: AgentStatus,
    pub error_message: Option<String>,
    pub capabilities: Option<super::AgentCapabilities>,
    pub agent_info: Option<super::AgentInfo>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_activity: Option<chrono::DateTime<chrono::Utc>>,
    pub session_count: u32,
}

impl AgentState {
    pub fn new(config: AgentConfig) -> Self {
        Self {
            config,
            status: AgentStatus::Stopped,
            error_message: None,
            capabilities: None,
            agent_info: None,
            started_at: None,
            last_activity: None,
            session_count: 0,
        }
    }
}

/// Agent usage statistics (persisted)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentStats {
    pub agent_id: String,
    pub total_sessions: u64,
    pub successful_sessions: u64,
    pub failed_sessions: u64,
    pub total_tasks: u64,
    pub completed_tasks: u64,
    pub total_tool_calls: u64,
    pub avg_session_duration_secs: f64,
    pub last_used: Option<chrono::DateTime<chrono::Utc>>,
}

impl AgentStats {
    pub fn new(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            total_sessions: 0,
            successful_sessions: 0,
            failed_sessions: 0,
            total_tasks: 0,
            completed_tasks: 0,
            total_tool_calls: 0,
            avg_session_duration_secs: 0.0,
            last_used: None,
        }
    }

    pub fn success_rate(&self) -> f64 {
        if self.total_sessions == 0 {
            0.0
        } else {
            self.successful_sessions as f64 / self.total_sessions as f64
        }
    }
}
