//! ACP Protocol message handling

use crate::error::{AcpError, Error, Result};
use crate::types::{
    ContentBlock, FileMetadata, InitializeParams, InitializeResult, JsonRpcError, JsonRpcRequest,
    JsonRpcResponse, McpServerConfig, SessionNewParams, SessionNewResult, SessionNewResultExtended,
    SessionLoadResult, SessionListResult, SessionPromptParams, SessionUpdateNotification,
    TerminalExecuteResult, ACP_PROTOCOL_VERSION, ClientCapabilities, ClientInfo,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{debug, trace, warn};

/// Protocol handler for ACP messages
pub struct ProtocolHandler {
    request_id: AtomicU64,
}

impl ProtocolHandler {
    pub fn new() -> Self {
        Self {
            request_id: AtomicU64::new(1),
        }
    }

    /// Generate next request ID
    pub fn next_id(&self) -> u64 {
        self.request_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Create initialize request
    pub fn create_initialize_request(
        &self,
        capabilities: ClientCapabilities,
    ) -> JsonRpcRequest {
        let params = InitializeParams {
            protocol_version: ACP_PROTOCOL_VERSION,
            client_info: ClientInfo::default(),
            capabilities,
        };

        JsonRpcRequest::new(
            self.next_id(),
            "initialize",
            Some(serde_json::to_value(params).unwrap()),
        )
    }

    /// Create session/new request
    pub fn create_session_new_request(
        &self,
        cwd: Option<String>,
        mcp_servers: Option<Vec<McpServerConfig>>,
    ) -> JsonRpcRequest {
        let params = SessionNewParams {
            cwd,
            mcp_servers: mcp_servers.unwrap_or_default(),
        };

        JsonRpcRequest::new(
            self.next_id(),
            "session/new",
            Some(serde_json::to_value(params).unwrap()),
        )
    }

    /// Create session/prompt request
    pub fn create_session_prompt_request(
        &self,
        session_id: String,
        prompt: Vec<ContentBlock>,
        mode: Option<String>,
    ) -> JsonRpcRequest {
        let params = SessionPromptParams {
            session_id,
            prompt,
            mode,
        };

        JsonRpcRequest::new(
            self.next_id(),
            "session/prompt",
            Some(serde_json::to_value(params).unwrap()),
        )
    }

    /// Create session/cancel request
    pub fn create_session_cancel_request(&self, session_id: String) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/cancel",
            Some(serde_json::json!({ "sessionId": session_id })),
        )
    }

    /// Create session/load request
    pub fn create_session_load_request(
        &self,
        session_id: String,
        cwd: Option<String>,
        mcp_servers: Option<Vec<McpServerConfig>>,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/load",
            Some(serde_json::json!({
                "sessionId": session_id,
                "cwd": cwd,
                "mcpServers": mcp_servers,
            })),
        )
    }

    /// Create session/setMode request
    pub fn create_session_set_mode_request(
        &self,
        session_id: String,
        mode_id: String,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/setMode",
            Some(serde_json::json!({
                "sessionId": session_id,
                "modeId": mode_id,
            })),
        )
    }

    /// Create session/setModel request
    pub fn create_session_set_model_request(
        &self,
        session_id: String,
        model_id: String,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/setModel",
            Some(serde_json::json!({
                "sessionId": session_id,
                "modelId": model_id,
            })),
        )
    }

    /// Create session/setConfig request
    pub fn create_session_set_config_request(
        &self,
        session_id: String,
        config_id: String,
        value: String,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/setConfig",
            Some(serde_json::json!({
                "sessionId": session_id,
                "configId": config_id,
                "value": value,
            })),
        )
    }

    /// Create session/list request
    pub fn create_session_list_request(&self) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/list",
            Some(serde_json::json!({})),
        )
    }

    /// Create session/resume request (alias for load with different semantics)
    pub fn create_session_resume_request(
        &self,
        session_id: String,
        mcp_servers: Option<Vec<McpServerConfig>>,
    ) -> JsonRpcRequest {
        JsonRpcRequest::new(
            self.next_id(),
            "session/load",
            Some(serde_json::json!({
                "sessionId": session_id,
                "mcpServers": mcp_servers.unwrap_or_default(),
            })),
        )
    }

    /// Parse initialize response
    pub fn parse_initialize_response(
        &self,
        response: &JsonRpcResponse,
    ) -> Result<InitializeResult> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Initialize failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in initialize response".to_string(),
            ))
        })?;

        let init_result: InitializeResult = serde_json::from_value(result.clone())?;

        // Verify protocol version
        if init_result.protocol_version != ACP_PROTOCOL_VERSION {
            warn!(
                "Protocol version mismatch: expected {}, got {}",
                ACP_PROTOCOL_VERSION, init_result.protocol_version
            );
        }

        if let Some(ref agent_info) = init_result.agent_info {
            debug!(
                "Agent initialized: {} v{}",
                agent_info.name, agent_info.version
            );
        } else {
            debug!("Agent initialized (no agent info provided)");
        }

        Ok(init_result)
    }

    /// Parse session/new response
    pub fn parse_session_new_response(
        &self,
        response: &JsonRpcResponse,
    ) -> Result<SessionNewResult> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Session creation failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in session/new response".to_string(),
            ))
        })?;

        let session_result: SessionNewResult = serde_json::from_value(result.clone())?;
        debug!("Session created: {}", session_result.session_id);

        Ok(session_result)
    }

    /// Parse session/new extended response with mode/model/config
    pub fn parse_session_new_response_extended(
        &self,
        response: &JsonRpcResponse,
    ) -> Result<SessionNewResultExtended> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Session creation failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in session/new response".to_string(),
            ))
        })?;

        // Try extended format first, fall back to basic
        match serde_json::from_value::<SessionNewResultExtended>(result.clone()) {
            Ok(extended) => {
                debug!(
                    "Session created (extended): {} with {} modes, {} models",
                    extended.session_id,
                    extended.modes.len(),
                    extended.models.len()
                );
                Ok(extended)
            }
            Err(_) => {
                // Fall back to basic format
                let basic: SessionNewResult = serde_json::from_value(result.clone())?;
                debug!("Session created (basic): {}", basic.session_id);
                Ok(basic.into())
            }
        }
    }

    /// Parse session/load response
    pub fn parse_session_load_response(
        &self,
        response: &JsonRpcResponse,
    ) -> Result<SessionLoadResult> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Session load failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in session/load response".to_string(),
            ))
        })?;

        let load_result: SessionLoadResult = serde_json::from_value(result.clone())?;
        debug!(
            "Session loaded: {} with {} messages",
            load_result.session_id,
            load_result.messages.len()
        );

        Ok(load_result)
    }

    /// Parse session/list response
    pub fn parse_session_list_response(
        &self,
        response: &JsonRpcResponse,
    ) -> Result<SessionListResult> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Session list failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in session/list response".to_string(),
            ))
        })?;

        let list_result: SessionListResult = serde_json::from_value(result.clone())?;
        debug!("Session list: {} sessions", list_result.sessions.len());

        Ok(list_result)
    }

    /// Parse void response (for setMode, setModel, setConfig)
    pub fn parse_void_response(&self, response: &JsonRpcResponse) -> Result<()> {
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Request failed: {} (code {})",
                error.message, error.code
            ))));
        }
        Ok(())
    }

    /// Parse incoming message (could be response, notification, or request)
    pub fn parse_message(&self, value: &serde_json::Value) -> Result<AcpMessage> {
        // Check if it's a response (has "result" or "error" and "id")
        if value.get("id").is_some()
            && (value.get("result").is_some() || value.get("error").is_some())
        {
            let response: JsonRpcResponse = serde_json::from_value(value.clone())?;
            return Ok(AcpMessage::Response(response));
        }

        // Check if it's a notification (has "method" but no "id")
        if value.get("method").is_some() && value.get("id").is_none() {
            let method = value["method"].as_str().unwrap_or("");
            return self.parse_notification(method, value);
        }

        // Check if it's a request from agent (has "method" and "id")
        if value.get("method").is_some() && value.get("id").is_some() {
            let request: JsonRpcRequest = serde_json::from_value(value.clone())?;
            return Ok(AcpMessage::AgentRequest(request));
        }

        Err(Error::Acp(AcpError::InvalidMessage(format!(
            "Unknown message type: {}",
            value
        ))))
    }

    /// Parse notification message
    fn parse_notification(
        &self,
        method: &str,
        value: &serde_json::Value,
    ) -> Result<AcpMessage> {
        match method {
            "session/update" => {
                let params = value.get("params").ok_or_else(|| {
                    Error::Acp(AcpError::InvalidMessage(
                        "Missing params in session/update".to_string(),
                    ))
                })?;

                let notification: SessionUpdateNotification =
                    serde_json::from_value(params.clone())?;
                Ok(AcpMessage::SessionUpdate(notification))
            }
            "$/progress" => {
                // Progress notification, can be logged but not critical
                trace!("Progress notification: {:?}", value);
                Ok(AcpMessage::Progress(value.clone()))
            }
            _ => {
                warn!("Unknown notification method: {}", method);
                Ok(AcpMessage::Unknown(value.clone()))
            }
        }
    }

    /// Create response to agent's fs/read_text_file request
    pub fn create_fs_read_response(
        &self,
        request_id: serde_json::Value,
        content: &str,
    ) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id),
            result: Some(serde_json::json!({ "content": content })),
            error: None,
        }
    }

    /// Create response to agent's fs/write_file request
    pub fn create_fs_write_response(
        &self,
        request_id: serde_json::Value,
    ) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id),
            result: Some(serde_json::json!({ "_meta": {} })),
            error: None,
        }
    }

    /// Create response to agent's fs/list_directory request
    pub fn create_fs_list_response(
        &self,
        request_id: serde_json::Value,
        entries: Vec<FileMetadata>,
    ) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id),
            result: Some(serde_json::json!({ "entries": entries })),
            error: None,
        }
    }

    /// Create response to agent's terminal/execute request
    pub fn create_terminal_response(
        &self,
        request_id: serde_json::Value,
        result: TerminalExecuteResult,
    ) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id),
            result: Some(serde_json::to_value(result).unwrap()),
            error: None,
        }
    }

    /// Create error response
    pub fn create_error_response(
        &self,
        request_id: serde_json::Value,
        code: i32,
        message: &str,
    ) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: Some(request_id),
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.to_string(),
                data: None,
            }),
        }
    }
}

impl Default for ProtocolHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Parsed ACP message types
#[derive(Debug, Clone)]
pub enum AcpMessage {
    /// Response to our request
    Response(JsonRpcResponse),
    /// Session update notification
    SessionUpdate(SessionUpdateNotification),
    /// Request from agent (fs/*, terminal/*)
    AgentRequest(JsonRpcRequest),
    /// Progress notification
    Progress(serde_json::Value),
    /// Unknown message
    Unknown(serde_json::Value),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_handler_request_ids() {
        let handler = ProtocolHandler::new();
        assert_eq!(handler.next_id(), 1);
        assert_eq!(handler.next_id(), 2);
        assert_eq!(handler.next_id(), 3);
    }

    #[test]
    fn test_create_initialize_request() {
        let handler = ProtocolHandler::new();
        let request = handler.create_initialize_request(ClientCapabilities::default());

        assert_eq!(request.method, "initialize");
        assert!(request.params.is_some());

        let params = request.params.unwrap();
        assert_eq!(params["protocolVersion"], ACP_PROTOCOL_VERSION);
        assert_eq!(params["clientInfo"]["name"], "CocoWork");
    }

    #[test]
    fn test_create_session_new_request() {
        let handler = ProtocolHandler::new();
        let request = handler.create_session_new_request(
            Some("/home/user".to_string()),
            None,
        );

        assert_eq!(request.method, "session/new");
        let params = request.params.unwrap();
        assert_eq!(params["cwd"], "/home/user");
    }

    #[test]
    fn test_parse_message_response() {
        let handler = ProtocolHandler::new();
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "sessionId": "test-session" }
        });

        let msg = handler.parse_message(&value).unwrap();
        assert!(matches!(msg, AcpMessage::Response(_)));
    }

    #[test]
    fn test_parse_message_session_update() {
        let handler = ProtocolHandler::new();
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "session/update",
            "params": {
                "sessionId": "test-session",
                "sessionUpdate": "agent_message_chunk",
                "content": {
                    "type": "text",
                    "text": "Hello"
                }
            }
        });

        let msg = handler.parse_message(&value).unwrap();
        assert!(matches!(msg, AcpMessage::SessionUpdate(_)));
    }

    #[test]
    fn test_parse_message_agent_request() {
        let handler = ProtocolHandler::new();
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "fs/read_text_file",
            "params": {
                "sessionId": "test-session",
                "path": "/home/user/file.txt"
            }
        });

        let msg = handler.parse_message(&value).unwrap();
        assert!(matches!(msg, AcpMessage::AgentRequest(_)));
    }
}
