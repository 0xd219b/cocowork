//! Core type definitions for CocoWork
//!
//! This module contains all shared types used across the application,
//! including ACP protocol types, task state types, and configuration types.

mod acp_types;
mod agent_types;
mod artifact_types;
mod session_types;
mod task_types;

pub use acp_types::*;
pub use agent_types::*;
pub use artifact_types::*;
pub use session_types::*;
pub use task_types::*;

use serde::{Deserialize, Serialize};

/// Content block that can contain text, images, or other content types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    Image { source: ImageSource },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String, is_error: Option<bool> },
}

/// Image source for content blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 {
        media_type: String,
        data: String,
    },
    Url {
        url: String,
    },
}

/// File metadata for directory listings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileMetadata {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
    pub mime_type: Option<String>,
}

/// Plan entry from agent planning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub priority: PlanPriority,
    pub status: PlanStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PlanPriority {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Skipped,
}

/// MCP Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: std::collections::HashMap<String, String>,
    pub transport: McpTransport,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    WebSocket,
}

impl Default for McpTransport {
    fn default() -> Self {
        Self::Stdio
    }
}

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_agent: Option<String>,
    pub auto_accept_edits: bool,
    pub show_thoughts: bool,
    pub terminal_policy: TerminalPolicy,
    pub mcp_servers: Vec<McpServerConfig>,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            default_agent: None,
            auto_accept_edits: false,
            show_thoughts: true,
            terminal_policy: TerminalPolicy::default(),
            mcp_servers: Vec::new(),
            theme: "light".to_string(),
        }
    }
}

/// Terminal execution policy
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPolicy {
    pub enabled: bool,
    pub require_confirmation: bool,
    pub allowed_commands: Vec<String>,
    pub blocked_patterns: Vec<String>,
}

impl Default for TerminalPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            require_confirmation: true,
            allowed_commands: vec![
                "ls".to_string(),
                "cat".to_string(),
                "grep".to_string(),
                "find".to_string(),
                "mv".to_string(),
                "cp".to_string(),
                "mkdir".to_string(),
                "touch".to_string(),
            ],
            blocked_patterns: vec![
                "rm -rf".to_string(),
                "sudo".to_string(),
                "chmod".to_string(),
                "chown".to_string(),
            ],
        }
    }
}
