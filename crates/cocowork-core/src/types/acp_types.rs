//! ACP (Agent Client Protocol) type definitions
//!
//! Based on the ACP specification at https://agentclientprotocol.com

use serde::{de, Deserialize, Deserializer, Serialize};
use std::collections::HashMap;

/// ACP Protocol version supported by this client
pub const ACP_PROTOCOL_VERSION: u32 = 1;

/// Client information sent during initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self {
            name: "CocoWork".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }
}

/// Client capabilities declared during initialization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system: Option<FileSystemCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp: Option<McpCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub load_session: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileSystemCapability {
    pub read: bool,
    pub write: bool,
    pub list: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TerminalCapability {
    pub execute: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapability {
    pub servers: Vec<super::McpServerConfig>,
}

/// Agent capabilities received during initialization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    #[serde(default)]
    pub supports_mcp: bool,
    #[serde(default)]
    pub supports_modes: bool,
    #[serde(default)]
    pub supports_plans: bool,
    #[serde(default)]
    pub supports_thoughts: bool,
    #[serde(default)]
    pub load_session: bool,
    #[serde(default)]
    pub available_modes: Vec<AgentMode>,
}

/// Agent mode (e.g., "ask", "code", "architect")
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMode {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}

/// JSON-RPC 2.0 Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<serde_json::Value>, method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id.into()),
            method: method.to_string(),
            params,
        }
    }

    pub fn notification(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.to_string(),
            params,
        }
    }
}

/// JSON-RPC 2.0 Response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Initialize request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: u32,
    pub client_info: ClientInfo,
    pub capabilities: ClientCapabilities,
}

/// Initialize response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    #[serde(default)]
    pub protocol_version: u32,
    #[serde(default)]
    pub agent_info: Option<AgentInfo>,
    /// Standard ACP capabilities field
    #[serde(default)]
    pub capabilities: Option<AgentCapabilities>,
    /// Gemini CLI uses agentCapabilities instead of capabilities
    #[serde(default)]
    pub agent_capabilities: Option<GeminiAgentCapabilities>,
}

impl InitializeResult {
    /// Get capabilities, preferring standard format over Gemini format
    pub fn get_capabilities(&self) -> AgentCapabilities {
        if let Some(caps) = &self.capabilities {
            caps.clone()
        } else if let Some(gemini_caps) = &self.agent_capabilities {
            AgentCapabilities {
                supports_mcp: gemini_caps.mcp_capabilities.is_some(),
                supports_modes: false,
                supports_plans: false,
                supports_thoughts: false,
                load_session: gemini_caps.load_session,
                available_modes: Vec::new(),
            }
        } else {
            AgentCapabilities::default()
        }
    }
}

/// Gemini CLI specific capabilities format
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeminiAgentCapabilities {
    #[serde(default)]
    pub load_session: bool,
    #[serde(default)]
    pub prompt_capabilities: Option<GeminiPromptCapabilities>,
    #[serde(default)]
    pub mcp_capabilities: Option<GeminiMcpCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeminiPromptCapabilities {
    #[serde(default)]
    pub image: bool,
    #[serde(default)]
    pub audio: bool,
    #[serde(default)]
    pub embedded_context: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GeminiMcpCapabilities {
    #[serde(default)]
    pub http: bool,
    #[serde(default)]
    pub sse: bool,
}

/// Agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub version: String,
}

/// Session/new request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// MCP servers - Gemini CLI requires this to be an array (not undefined)
    #[serde(default)]
    pub mcp_servers: Vec<super::McpServerConfig>,
}

/// Session/new response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewResult {
    pub session_id: String,
}

/// Session/prompt request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub prompt: Vec<super::ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

/// Prompt response (completion)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptResponse {
    pub stop_reason: StopReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    Cancelled,
    Error,
}

/// Session update notification
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateNotification {
    pub session_id: String,
    pub update: SessionUpdate,
}

impl<'de> Deserialize<'de> for SessionUpdateNotification {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        // ACP spec shape:
        // { "sessionId": "...", "update": { "sessionUpdate": "...", ... } }
        if value.get("update").is_some() {
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Wrapped {
                session_id: String,
                update: SessionUpdate,
            }

            let wrapped: Wrapped = serde_json::from_value(value).map_err(de::Error::custom)?;
            Ok(Self {
                session_id: wrapped.session_id,
                update: wrapped.update,
            })
        } else {
            // Back-compat: some implementations flatten the union at the top level:
            // { "sessionId": "...", "sessionUpdate": "...", ... }
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct Flat {
                session_id: String,
                #[serde(flatten)]
                update: SessionUpdate,
            }

            let flat: Flat = serde_json::from_value(value).map_err(de::Error::custom)?;
            Ok(Self {
                session_id: flat.session_id,
                update: flat.update,
            })
        }
    }
}

/// Session update types (union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "sessionUpdate", rename_all = "snake_case")]
pub enum SessionUpdate {
    AgentMessageChunk {
        content: super::ContentBlock,
    },
    UserMessageChunk {
        content: super::ContentBlock,
    },
    #[serde(alias = "agent_thought_chunk")]
    Thought {
        content: super::ContentBlock,
    },
    ToolCall {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        title: Option<String>,
        kind: Option<ToolCallKind>,
        status: ToolCallStatus,
    },
    ToolCallUpdate {
        #[serde(rename = "toolCallId")]
        tool_call_id: String,
        status: ToolCallStatus,
        content: Option<Vec<ToolCallContent>>,
    },
    Plan {
        entries: Vec<super::PlanEntry>,
    },
    CurrentModeUpdate {
        #[serde(rename = "modeId")]
        mode_id: String,
    },
    AvailableCommandsUpdate {
        #[serde(rename = "availableCommands")]
        available_commands: Vec<AvailableCommand>,
    },
    /// Internal: Prompt response received (not from ACP protocol)
    #[serde(skip)]
    PromptResponseReceived {
        stop_reason: Option<super::StopReason>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallKind {
    Read,
    Write,
    Delete,
    Move,
    Execute,
    Fetch,
    Search,
    Glob,
    Grep,
    Edit,
    Create,
    Terminal,
    Bash,
    Task,
    Plan,
    Think,
    #[default]
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallContent {
    Content { content: super::ContentBlock },
    Diff { diff: FileDiff },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffLineKind {
    Context,
    Add,
    Remove,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableCommand {
    pub name: String,
    pub description: Option<String>,
}

// === Client-to-Agent Requests (Agent requests these from Client) ===

/// fs/read_text_file request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsReadTextFileParams {
    pub session_id: String,
    pub path: String,
}

/// fs/write_file request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsWriteFileParams {
    pub session_id: String,
    pub path: String,
    pub content: String,
}

/// fs/list_directory request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsListDirectoryParams {
    pub session_id: String,
    pub path: String,
}

/// fs/delete_file request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsDeleteFileParams {
    pub session_id: String,
    pub path: String,
}

/// fs/move_file request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsMoveFileParams {
    pub session_id: String,
    #[serde(rename = "oldPath")]
    pub old_path: String,
    #[serde(rename = "newPath")]
    pub new_path: String,
}

/// fs/create_directory request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FsCreateDirectoryParams {
    pub session_id: String,
    pub path: String,
}

/// terminal/execute request from agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExecuteParams {
    pub session_id: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
}

/// terminal/execute response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExecuteResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

// ============================================================================
// Extended Session Types (Mode/Model/Config)
// ============================================================================

/// Session/new extended response with mode/model/config support
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewResultExtended {
    pub session_id: String,
    #[serde(default)]
    pub modes: Vec<SessionModeInfo>,
    #[serde(default)]
    pub models: Vec<SessionModelInfo>,
    #[serde(default)]
    pub config_options: Vec<SessionConfigOptionInfo>,
    #[serde(default)]
    pub current_mode: Option<String>,
    #[serde(default)]
    pub current_model: Option<String>,
}

impl From<SessionNewResult> for SessionNewResultExtended {
    fn from(result: SessionNewResult) -> Self {
        Self {
            session_id: result.session_id,
            modes: Vec::new(),
            models: Vec::new(),
            config_options: Vec::new(),
            current_mode: None,
            current_model: None,
        }
    }
}

/// Session mode information from ACP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModeInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Session model information from ACP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionModelInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Session config option type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfigOptionType {
    String,
    Number,
    Boolean,
    Select,
}

impl Default for ConfigOptionType {
    fn default() -> Self {
        Self::String
    }
}

/// Session config option information from ACP
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigOptionInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub value_type: ConfigOptionType,
    #[serde(default)]
    pub current_value: Option<String>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

/// Session/load request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLoadParams {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub mcp_servers: Vec<super::McpServerConfig>,
}

/// Session/load response with messages and mode/model info
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLoadResult {
    pub session_id: String,
    #[serde(default)]
    pub modes: Vec<SessionModeInfo>,
    #[serde(default)]
    pub models: Vec<SessionModelInfo>,
    #[serde(default)]
    pub messages: Vec<SessionMessage>,
    #[serde(default)]
    pub current_mode: Option<String>,
    #[serde(default)]
    pub current_model: Option<String>,
}

/// A message in a loaded session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: SessionMessageRole,
    pub content: Vec<super::ContentBlock>,
    #[serde(default)]
    pub timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

/// Role of a session message
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageRole {
    User,
    Agent,
    System,
}

/// Session/setMode request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSetModeParams {
    pub session_id: String,
    pub mode_id: String,
}

/// Session/setModel request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSetModelParams {
    pub session_id: String,
    pub model_id: String,
}

/// Session/setConfig request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSetConfigParams {
    pub session_id: String,
    pub config_id: String,
    pub value: String,
}

/// Session/list request parameters (empty for now)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionListParams {}

/// Session/list response
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListResult {
    pub sessions: Vec<SessionListEntry>,
}

/// Entry in session list
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionListEntry {
    pub session_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub message_count: Option<u32>,
}

// ============================================================================
// MCP Server Types (for session creation)
// ============================================================================

/// MCP server configuration for ACP sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "transport", rename_all = "camelCase")]
pub enum McpServerAcp {
    /// Stdio-based MCP server
    Stdio(McpServerStdioAcp),
    /// HTTP-based MCP server
    Http(McpServerHttpAcp),
}

/// Stdio MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStdioAcp {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// HTTP MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerHttpAcp {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}
