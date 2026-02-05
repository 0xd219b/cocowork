//! ACP Connection implementation
//!
//! This module implements the AgentConnection trait for communicating with agents
//! via the Agent Client Protocol (ACP).

use super::protocol::{AcpMessage, ProtocolHandler};
use super::traits::{
    AgentClient, AgentConnection, ConfigOptionId, LoadSessionResponse, ModelId, NewSessionResponse,
    PromptMessage, PromptResult, SessionConfigOption, SessionInfo, SessionMode, SessionModeId,
    SessionModel, SessionNotification,
};
use super::transport::Transport;
use crate::error::{AcpError, Error, Result};
use crate::types::{
    AgentCapabilities, AgentInfo, ClientCapabilities, ConfigOptionType, ContentBlock,
    FsCreateDirectoryParams, FsDeleteFileParams, FsListDirectoryParams, FsMoveFileParams,
    FsReadTextFileParams, FsWriteFileParams, JsonRpcRequest, JsonRpcResponse, McpServerConfig,
    MessageBlock, PromptResponse, SessionMessageRole, SessionUpdateNotification,
    TerminalExecuteParams,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Child;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, error, info, trace, warn};

/// ACP Connection for communicating with an agent
///
/// This struct implements the `AgentConnection` trait and provides the full
/// functionality for communicating with an ACP-compatible agent.
pub struct AcpConnection {
    /// Agent name
    name: String,
    /// Protocol handler
    protocol: ProtocolHandler,
    /// Transport layer
    transport: Arc<Transport>,
    /// Child process handle
    child: Arc<Mutex<Child>>,
    /// Agent capabilities from initialization
    capabilities: Arc<RwLock<Option<AgentCapabilities>>>,
    /// Agent info
    agent_info: Arc<RwLock<Option<AgentInfo>>>,
    /// Pending requests (request_id -> response channel)
    pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    /// Notification broadcast channel
    notification_tx: broadcast::Sender<SessionNotification>,
    /// Message processing task
    _message_task: tokio::task::JoinHandle<()>,
}

impl AcpConnection {
    /// Create a new ACP connection by spawning the agent process
    pub async fn new(
        name: impl Into<String>,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        cwd: Option<&str>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Self> {
        let name = name.into();
        info!("Connecting to agent: {} ({})", name, command);

        let (transport, child) = Transport::spawn(command, args, env, cwd).await?;

        let transport = Arc::new(transport);
        let child = Arc::new(Mutex::new(child));
        let protocol = ProtocolHandler::new();
        let capabilities = Arc::new(RwLock::new(None));
        let agent_info = Arc::new(RwLock::new(None));
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));

        // Create notification broadcast channel with reasonable capacity
        let (notification_tx, _) = broadcast::channel(256);

        // Start message processing task
        let message_task = tokio::spawn(Self::message_loop(
            Arc::clone(&transport),
            Arc::clone(&pending_requests),
            notification_tx.clone(),
            delegate,
        ));

        Ok(Self {
            name,
            protocol,
            transport,
            child,
            capabilities,
            agent_info,
            pending_requests,
            notification_tx,
            _message_task: message_task,
        })
    }

    /// Initialize the ACP connection
    pub async fn initialize(&self, client_capabilities: ClientCapabilities) -> Result<()> {
        info!("Initializing ACP connection for {}", self.name);

        let request = self
            .protocol
            .create_initialize_request(client_capabilities);

        let response = self.send_request(request).await?;
        let init_result = self.protocol.parse_initialize_response(&response)?;

        // Store capabilities
        {
            let mut caps = self.capabilities.write().await;
            *caps = Some(init_result.get_capabilities());
        }

        // Store agent info
        {
            let mut info = self.agent_info.write().await;
            *info = init_result.agent_info;
        }

        info!("ACP connection initialized successfully for {}", self.name);
        Ok(())
    }

    /// Get agent capabilities
    pub async fn capabilities(&self) -> Option<AgentCapabilities> {
        self.capabilities.read().await.clone()
    }

    /// Get agent info
    pub async fn agent_info(&self) -> Option<AgentInfo> {
        self.agent_info.read().await.clone()
    }

    /// Send request and wait for response
    async fn send_request(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse> {
        let rx = self.send_request_with_receiver(request).await?;

        // Wait for response with timeout
        let response = tokio::time::timeout(std::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| Error::Acp(AcpError::Timeout))?
            .map_err(|_| {
                Error::Acp(AcpError::ConnectionFailed(
                    "Response channel closed".to_string(),
                ))
            })?;

        Ok(response)
    }

    async fn send_request_with_receiver(
        &self,
        request: JsonRpcRequest,
    ) -> Result<oneshot::Receiver<JsonRpcResponse>> {
        let request_id = request
            .id
            .as_ref()
            .and_then(|v| v.as_u64())
            .ok_or_else(|| Error::Acp(AcpError::InvalidMessage("Request missing ID".to_string())))?;

        debug!(
            "Sending request {} method={} params={:?}",
            request_id, request.method, request.params
        );

        // Create response channel
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(request_id, tx);
        }

        // Send request
        if let Err(e) = self.transport.send_request(&request).await {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&request_id);
            return Err(e);
        }

        debug!("Request {} sent, waiting for response", request_id);
        Ok(rx)
    }

    /// Send request without waiting for response
    async fn send_request_no_wait(&self, request: JsonRpcRequest) -> Result<()> {
        self.transport.send_request(&request).await
    }

    /// Message processing loop
    async fn message_loop(
        transport: Arc<Transport>,
        pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
        notification_tx: broadcast::Sender<SessionNotification>,
        delegate: Arc<dyn AgentClient>,
    ) {
        let protocol = ProtocolHandler::new();
        let mut buffer = String::new();

        let json_start_index = |s: &str| -> Option<usize> {
            let obj = s.find('{');
            let arr = s.find('[');
            match (obj, arr) {
                (Some(o), Some(a)) => Some(o.min(a)),
                (Some(o), None) => Some(o),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        };

        loop {
            let line = match transport.recv_line().await {
                Some(line) => line,
                None => {
                    debug!("Transport closed");
                    let _ = notification_tx.send(SessionNotification::Disconnected);
                    break;
                }
            };

            // Accumulate for multi-line JSON
            if buffer.is_empty() {
                buffer.push_str(&line);
            } else {
                buffer.push('\n');
                buffer.push_str(&line);
            }

            if buffer.len() > 1024 * 1024 {
                warn!("Dropping oversized stdout buffer ({} bytes)", buffer.len());
                buffer.clear();
                continue;
            }

            let value = match serde_json::from_str::<serde_json::Value>(&buffer) {
                Ok(v) => {
                    buffer.clear();
                    v
                }
                Err(e) if e.is_eof() => continue,
                Err(e) => {
                    let snippet = buffer.chars().take(300).collect::<String>();
                    debug!("Ignoring non-JSON agent output ({}): {}", e, snippet);

                    let trimmed = line.trim_start();
                    if let Some(idx) = json_start_index(trimmed) {
                        buffer.clear();
                        buffer.push_str(&trimmed[idx..]);

                        match serde_json::from_str::<serde_json::Value>(&buffer) {
                            Ok(v) => {
                                buffer.clear();
                                v
                            }
                            Err(e) if e.is_eof() => continue,
                            Err(e) => {
                                let snippet = buffer.chars().take(300).collect::<String>();
                                debug!("Ignoring non-JSON agent output ({}): {}", e, snippet);
                                buffer.clear();
                                continue;
                            }
                        }
                    } else {
                        buffer.clear();
                        continue;
                    }
                }
            };

            debug!("Received message: {}", value);

            match protocol.parse_message(&value) {
                Ok(AcpMessage::Response(response)) => {
                    debug!("Parsed as Response with id: {:?}", response.id);
                    if let Some(id) = response.id.as_ref().and_then(|v| v.as_u64()) {
                        let mut pending = pending_requests.lock().await;
                        if let Some(tx) = pending.remove(&id) {
                            debug!("Delivering response for request {}", id);
                            let _ = tx.send(response);
                        } else {
                            warn!("Received response for unknown request: {}", id);
                        }
                    }
                }
                Ok(AcpMessage::SessionUpdate(notification)) => {
                    info!(
                        "Received SessionUpdate for session: {} - {:?}",
                        notification.session_id,
                        notification.update
                    );
                    if notification_tx.send(SessionNotification::Update(notification)).is_err() {
                        warn!("No receivers for session update");
                    }
                }
                Ok(AcpMessage::AgentRequest(request)) => {
                    debug!("Parsed as AgentRequest: {}", request.method);
                    let response = Self::handle_agent_request(&protocol, &delegate, &request).await;
                    if let Err(e) = transport.send_response(&response).await {
                        error!("Failed to send response: {}", e);
                    }
                }
                Ok(AcpMessage::Progress(value)) => {
                    trace!("Progress: {:?}", value);
                }
                Ok(AcpMessage::Unknown(value)) => {
                    warn!("Unknown message: {:?}", value);
                }
                Err(e) => {
                    error!("Failed to parse message: {}", e);
                }
            }
        }
    }

    /// Handle an agent request using the delegate
    async fn handle_agent_request(
        protocol: &ProtocolHandler,
        delegate: &Arc<dyn AgentClient>,
        request: &JsonRpcRequest,
    ) -> JsonRpcResponse {
        let request_id = request.id.clone().unwrap_or(serde_json::Value::Null);
        let params = request.params.clone().unwrap_or(serde_json::Value::Null);

        match request.method.as_str() {
            "fs/read_text_file" => {
                match serde_json::from_value::<FsReadTextFileParams>(params) {
                    Ok(p) => match delegate.read_text_file(&p.session_id, &p.path).await {
                        Ok(content) => protocol.create_fs_read_response(request_id, &content),
                        Err(e) => protocol.create_error_response(request_id, -32603, &e.to_string()),
                    },
                    Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
                }
            }
            "fs/write_file" | "fs/write_text_file" => {
                match serde_json::from_value::<FsWriteFileParams>(params) {
                    Ok(p) => {
                        match delegate
                            .write_text_file(&p.session_id, &p.path, &p.content)
                            .await
                        {
                            Ok(()) => protocol.create_fs_write_response(request_id),
                            Err(e) => {
                                protocol.create_error_response(request_id, -32603, &e.to_string())
                            }
                        }
                    }
                    Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
                }
            }
            "fs/list_directory" => match serde_json::from_value::<FsListDirectoryParams>(params) {
                Ok(p) => match delegate.list_directory(&p.session_id, &p.path).await {
                    Ok(entries) => protocol.create_fs_list_response(request_id, entries),
                    Err(e) => protocol.create_error_response(request_id, -32603, &e.to_string()),
                },
                Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
            },
            "fs/delete_file" => match serde_json::from_value::<FsDeleteFileParams>(params) {
                Ok(p) => match delegate.delete_file(&p.session_id, &p.path).await {
                    Ok(()) => protocol.create_fs_write_response(request_id),
                    Err(e) => protocol.create_error_response(request_id, -32603, &e.to_string()),
                },
                Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
            },
            "fs/move_file" => match serde_json::from_value::<FsMoveFileParams>(params) {
                Ok(p) => {
                    match delegate
                        .move_file(&p.session_id, &p.old_path, &p.new_path)
                        .await
                    {
                        Ok(()) => protocol.create_fs_write_response(request_id),
                        Err(e) => protocol.create_error_response(request_id, -32603, &e.to_string()),
                    }
                }
                Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
            },
            "fs/create_directory" => {
                match serde_json::from_value::<FsCreateDirectoryParams>(params) {
                    Ok(p) => match delegate.create_directory(&p.session_id, &p.path).await {
                        Ok(()) => protocol.create_fs_write_response(request_id),
                        Err(e) => protocol.create_error_response(request_id, -32603, &e.to_string()),
                    },
                    Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
                }
            }
            "terminal/execute" | "terminal/create" => {
                match serde_json::from_value::<TerminalExecuteParams>(params) {
                    Ok(p) => {
                        let args = p.args.unwrap_or_default();
                        match delegate
                            .execute_command(
                                &p.session_id,
                                &p.command,
                                &args,
                                p.cwd.as_deref(),
                                p.env.as_ref(),
                            )
                            .await
                        {
                            Ok(result) => protocol.create_terminal_response(request_id, result),
                            Err(e) => {
                                protocol.create_error_response(request_id, -32603, &e.to_string())
                            }
                        }
                    }
                    Err(e) => protocol.create_error_response(request_id, -32602, &e.to_string()),
                }
            }
            other => protocol.create_error_response(
                request_id,
                -32601,
                &format!("Unsupported method: {}", other),
            ),
        }
    }
}

#[async_trait]
impl AgentConnection for AcpConnection {
    async fn new_session(
        &self,
        cwd: PathBuf,
        mcp_servers: Vec<McpServerConfig>,
    ) -> Result<NewSessionResponse> {
        debug!("Creating new session (cwd: {:?})", cwd);

        let request = self.protocol.create_session_new_request(
            Some(cwd.to_string_lossy().to_string()),
            Some(mcp_servers),
        );
        let response = self.send_request(request).await?;
        let result = self.protocol.parse_session_new_response_extended(&response)?;

        // Convert to our response type
        let modes: Vec<SessionMode> = result
            .modes
            .into_iter()
            .map(|m| {
                let mut mode = SessionMode::new(m.id, m.name);
                if let Some(desc) = m.description {
                    mode = mode.with_description(desc);
                }
                mode
            })
            .collect();

        let models: Vec<SessionModel> = result
            .models
            .into_iter()
            .map(|m| {
                let mut model = SessionModel::new(m.id, m.name);
                if let Some(desc) = m.description {
                    model = model.with_description(desc);
                }
                model
            })
            .collect();

        let config_options: Vec<SessionConfigOption> = result
            .config_options
            .into_iter()
            .map(|c| {
                use super::traits::ConfigValueType;
                let value_type = match c.value_type {
                    ConfigOptionType::String => ConfigValueType::String,
                    ConfigOptionType::Number => ConfigValueType::Number,
                    ConfigOptionType::Boolean => ConfigValueType::Boolean,
                    ConfigOptionType::Select => ConfigValueType::Select,
                };
                SessionConfigOption {
                    id: ConfigOptionId::new(c.id),
                    name: c.name,
                    description: c.description,
                    value_type,
                    current_value: c.current_value,
                    options: c.options,
                }
            })
            .collect();

        Ok(NewSessionResponse {
            session_id: result.session_id,
            modes,
            models,
            config_options,
            current_mode: result.current_mode.map(SessionModeId::new),
            current_model: result.current_model.map(ModelId::new),
        })
    }

    async fn load_session(
        &self,
        session_id: String,
        mcp_servers: Vec<McpServerConfig>,
    ) -> Result<LoadSessionResponse> {
        let caps = self.capabilities.read().await;
        if !caps.as_ref().map(|c| c.load_session).unwrap_or(false) {
            return Err(Error::Acp(AcpError::CapabilityNotSupported(
                "loadSession".to_string(),
            )));
        }
        drop(caps);

        debug!("Loading session: {}", session_id);

        let request = self
            .protocol
            .create_session_load_request(session_id.clone(), None, Some(mcp_servers));
        let response = self.send_request(request).await?;
        let result = self.protocol.parse_session_load_response(&response)?;

        // Convert to our response type
        let modes: Vec<SessionMode> = result
            .modes
            .into_iter()
            .map(|m| {
                let mut mode = SessionMode::new(m.id, m.name);
                if let Some(desc) = m.description {
                    mode = mode.with_description(desc);
                }
                mode
            })
            .collect();

        let models: Vec<SessionModel> = result
            .models
            .into_iter()
            .map(|m| {
                let mut model = SessionModel::new(m.id, m.name);
                if let Some(desc) = m.description {
                    model = model.with_description(desc);
                }
                model
            })
            .collect();

        let messages: Vec<MessageBlock> = result
            .messages
            .into_iter()
            .map(|m| match m.role {
                SessionMessageRole::User => MessageBlock::user(m.content),
                SessionMessageRole::Agent => MessageBlock::agent(m.content),
                SessionMessageRole::System => {
                    let text = m
                        .content
                        .into_iter()
                        .filter_map(|c| match c {
                            ContentBlock::Text { text } => Some(text),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    MessageBlock::System {
                        content: text,
                        timestamp: m.timestamp.unwrap_or_else(chrono::Utc::now),
                    }
                }
            })
            .collect();

        Ok(LoadSessionResponse {
            session_id: result.session_id,
            modes,
            models,
            messages,
            current_mode: result.current_mode.map(SessionModeId::new),
            current_model: result.current_model.map(ModelId::new),
        })
    }

    async fn prompt(&self, session_id: String, message: PromptMessage) -> Result<PromptResult> {
        debug!("Sending prompt to session: {}", session_id);

        let mode = message.mode.map(|m| m.0);
        let request = self
            .protocol
            .create_session_prompt_request(session_id, message.content, mode);

        let response = self.send_request(request).await?;

        // Parse the prompt response
        if let Some(error) = &response.error {
            return Err(Error::Acp(AcpError::InvalidMessage(format!(
                "Prompt failed: {} (code {})",
                error.message, error.code
            ))));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            Error::Acp(AcpError::InvalidMessage(
                "Missing result in prompt response".to_string(),
            ))
        })?;

        let prompt_response: PromptResponse = serde_json::from_value(result.clone())?;

        Ok(PromptResult {
            stop_reason: prompt_response.stop_reason,
        })
    }

    async fn prompt_streaming(&self, session_id: String, message: PromptMessage) -> Result<()> {
        debug!("Sending streaming prompt to session: {}", session_id);

        let mode = message.mode.map(|m| m.0);
        let request = self
            .protocol
            .create_session_prompt_request(session_id, message.content, mode);

        // Don't wait for response - updates come via session/update notifications
        self.send_request_no_wait(request).await?;

        Ok(())
    }

    async fn cancel(&self, session_id: String) -> Result<()> {
        debug!("Cancelling session: {}", session_id);

        let request = self.protocol.create_session_cancel_request(session_id);
        let _ = self.send_request(request).await?;

        Ok(())
    }

    async fn set_mode(&self, session_id: String, mode_id: SessionModeId) -> Result<()> {
        debug!("Setting mode for session {}: {}", session_id, mode_id.as_str());

        let request = self
            .protocol
            .create_session_set_mode_request(session_id, mode_id.0);
        let response = self.send_request(request).await?;
        self.protocol.parse_void_response(&response)?;

        Ok(())
    }

    async fn set_model(&self, session_id: String, model_id: ModelId) -> Result<()> {
        debug!(
            "Setting model for session {}: {}",
            session_id,
            model_id.as_str()
        );

        let request = self
            .protocol
            .create_session_set_model_request(session_id, model_id.0);
        let response = self.send_request(request).await?;
        self.protocol.parse_void_response(&response)?;

        Ok(())
    }

    async fn set_config(
        &self,
        session_id: String,
        config_id: ConfigOptionId,
        value: String,
    ) -> Result<()> {
        debug!(
            "Setting config for session {}: {} = {}",
            session_id,
            config_id.as_str(),
            value
        );

        let request =
            self.protocol
                .create_session_set_config_request(session_id, config_id.0, value);
        let response = self.send_request(request).await?;
        self.protocol.parse_void_response(&response)?;

        Ok(())
    }

    async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        debug!("Listing sessions");

        let request = self.protocol.create_session_list_request();
        let response = self.send_request(request).await?;
        let result = self.protocol.parse_session_list_response(&response)?;

        let sessions: Vec<SessionInfo> = result
            .sessions
            .into_iter()
            .map(|s| SessionInfo {
                session_id: s.session_id,
                title: s.title,
                created_at: s.created_at.unwrap_or_else(chrono::Utc::now),
                updated_at: s.updated_at.unwrap_or_else(chrono::Utc::now),
                message_count: s.message_count.unwrap_or(0),
            })
            .collect();

        Ok(sessions)
    }

    fn subscribe_updates(&self) -> broadcast::Receiver<SessionNotification> {
        self.notification_tx.subscribe()
    }

    async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) => false,
            Err(_) => false,
        }
    }

    async fn terminate(&self) -> Result<()> {
        info!("Terminating agent: {}", self.name);

        let mut child = self.child.lock().await;
        child.kill().await.map_err(|e| {
            Error::Acp(AcpError::ConnectionFailed(format!(
                "Failed to kill agent: {}",
                e
            )))
        })?;

        Ok(())
    }

    async fn send_response(&self, response: JsonRpcResponse) -> Result<()> {
        self.transport.send_response(&response).await
    }
}

// ============================================================================
// Legacy AcpClient interface for backward compatibility
// ============================================================================

impl AcpConnection {
    /// Legacy connect method for backward compatibility with existing code.
    ///
    /// This method creates an AcpConnection using the old channel-based approach.
    /// New code should use the AgentServer::connect() method instead.
    pub async fn connect(
        config: crate::types::AgentConfig,
        cwd: Option<&str>,
        update_tx: mpsc::Sender<SessionUpdateNotification>,
        agent_request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
    ) -> Result<Self> {
        info!(
            "Connecting to agent (legacy): {} ({})",
            config.name, config.command
        );

        let (transport, child) = Transport::spawn(&config.command, &config.args, &config.env, cwd)
            .await?;

        let transport = Arc::new(transport);
        let child = Arc::new(Mutex::new(child));
        let protocol = ProtocolHandler::new();
        let capabilities = Arc::new(RwLock::new(None));
        let agent_info = Arc::new(RwLock::new(None));
        let pending_requests = Arc::new(Mutex::new(HashMap::new()));

        // Create notification broadcast channel
        let (notification_tx, _) = broadcast::channel(256);

        // Start message processing task with legacy channel forwarding
        let message_task = tokio::spawn(Self::legacy_message_loop(
            Arc::clone(&transport),
            Arc::clone(&pending_requests),
            update_tx,
            agent_request_tx,
        ));

        Ok(Self {
            name: config.name.clone(),
            protocol,
            transport,
            child,
            capabilities,
            agent_info,
            pending_requests,
            notification_tx,
            _message_task: message_task,
        })
    }

    /// Legacy message processing loop that forwards to channels
    async fn legacy_message_loop(
        transport: Arc<Transport>,
        pending_requests: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
        update_tx: mpsc::Sender<SessionUpdateNotification>,
        agent_request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
    ) {
        let protocol = ProtocolHandler::new();
        let mut buffer = String::new();

        let json_start_index = |s: &str| -> Option<usize> {
            let obj = s.find('{');
            let arr = s.find('[');
            match (obj, arr) {
                (Some(o), Some(a)) => Some(o.min(a)),
                (Some(o), None) => Some(o),
                (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        };

        loop {
            let line = match transport.recv_line().await {
                Some(line) => line,
                None => {
                    debug!("Transport closed");
                    break;
                }
            };

            // Accumulate for multi-line JSON
            if buffer.is_empty() {
                buffer.push_str(&line);
            } else {
                buffer.push('\n');
                buffer.push_str(&line);
            }

            if buffer.len() > 1024 * 1024 {
                warn!("Dropping oversized stdout buffer ({} bytes)", buffer.len());
                buffer.clear();
                continue;
            }

            let value = match serde_json::from_str::<serde_json::Value>(&buffer) {
                Ok(v) => {
                    buffer.clear();
                    v
                }
                Err(e) if e.is_eof() => continue,
                Err(e) => {
                    let snippet = buffer.chars().take(300).collect::<String>();
                    debug!("Ignoring non-JSON agent output ({}): {}", e, snippet);

                    let trimmed = line.trim_start();
                    if let Some(idx) = json_start_index(trimmed) {
                        buffer.clear();
                        buffer.push_str(&trimmed[idx..]);

                        match serde_json::from_str::<serde_json::Value>(&buffer) {
                            Ok(v) => {
                                buffer.clear();
                                v
                            }
                            Err(e) if e.is_eof() => continue,
                            Err(e) => {
                                let snippet = buffer.chars().take(300).collect::<String>();
                                debug!("Ignoring non-JSON agent output ({}): {}", e, snippet);
                                buffer.clear();
                                continue;
                            }
                        }
                    } else {
                        buffer.clear();
                        continue;
                    }
                }
            };

            debug!("Received message: {}", value);

            match protocol.parse_message(&value) {
                Ok(AcpMessage::Response(response)) => {
                    debug!("Parsed as Response with id: {:?}", response.id);
                    if let Some(id) = response.id.as_ref().and_then(|v| v.as_u64()) {
                        let mut pending = pending_requests.lock().await;
                        if let Some(tx) = pending.remove(&id) {
                            debug!("Delivering response for request {}", id);
                            let _ = tx.send(response);
                        } else {
                            warn!("Received response for unknown request: {}", id);
                        }
                    }
                }
                Ok(AcpMessage::SessionUpdate(notification)) => {
                    debug!(
                        "Parsed as SessionUpdate for session: {}",
                        notification.session_id
                    );
                    if update_tx.send(notification).await.is_err() {
                        warn!("Failed to send session update, channel closed");
                    }
                }
                Ok(AcpMessage::AgentRequest(request)) => {
                    debug!("Parsed as AgentRequest: {}", request.method);
                    let (tx, rx) = oneshot::channel();
                    if agent_request_tx.send((request.clone(), tx)).await.is_err() {
                        warn!("Failed to send agent request, channel closed");
                        continue;
                    }

                    // Wait for handler to provide response, then send it back
                    if let Ok(response) = rx.await {
                        if let Err(e) = transport.send_response(&response).await {
                            error!("Failed to send response: {}", e);
                        }
                    }
                }
                Ok(AcpMessage::Progress(value)) => {
                    trace!("Progress: {:?}", value);
                }
                Ok(AcpMessage::Unknown(value)) => {
                    warn!("Unknown message: {:?}", value);
                }
                Err(e) => {
                    error!("Failed to parse message: {}", e);
                }
            }
        }
    }

    /// Legacy method: Create a new session (returns just session_id)
    pub async fn new_session(
        &self,
        cwd: Option<String>,
        mcp_servers: Option<Vec<McpServerConfig>>,
    ) -> Result<String> {
        debug!("Creating new session (cwd: {:?})", cwd);

        let request = self
            .protocol
            .create_session_new_request(cwd, mcp_servers);
        let response = self.send_request(request).await?;
        let result = self.protocol.parse_session_new_response(&response)?;

        Ok(result.session_id)
    }

    /// Legacy method: Send a prompt to a session
    pub async fn send_prompt(
        &self,
        session_id: String,
        prompt_content: Vec<ContentBlock>,
        mode: Option<String>,
    ) -> Result<()> {
        debug!("Sending prompt to session: {}", session_id);

        let request = self
            .protocol
            .create_session_prompt_request(session_id, prompt_content, mode);

        // Don't wait for response - updates come via session/update notifications
        self.send_request_no_wait(request).await?;

        Ok(())
    }

    /// Legacy method: Send a prompt with response channel
    pub async fn send_prompt_with_response_channel(
        &self,
        session_id: String,
        prompt_content: Vec<ContentBlock>,
        mode: Option<String>,
    ) -> Result<oneshot::Receiver<JsonRpcResponse>> {
        debug!("Sending prompt (awaitable) to session: {}", session_id);

        let request = self
            .protocol
            .create_session_prompt_request(session_id, prompt_content, mode);

        self.send_request_with_receiver(request).await
    }

    /// Legacy method: Cancel a session
    pub async fn cancel_session(&self, session_id: String) -> Result<()> {
        debug!("Cancelling session: {}", session_id);

        let request = self.protocol.create_session_cancel_request(session_id);
        let _ = self.send_request(request).await?;

        Ok(())
    }

    /// Legacy method: Load an existing session
    pub async fn load_session_legacy(
        &self,
        session_id: String,
        cwd: Option<String>,
        mcp_servers: Option<Vec<McpServerConfig>>,
    ) -> Result<()> {
        let caps = self.capabilities.read().await;
        if !caps.as_ref().map(|c| c.load_session).unwrap_or(false) {
            return Err(Error::Acp(AcpError::CapabilityNotSupported(
                "loadSession".to_string(),
            )));
        }
        drop(caps);

        debug!("Loading session: {}", session_id);

        let request = self
            .protocol
            .create_session_load_request(session_id, cwd, mcp_servers);
        let _ = self.send_request(request).await?;

        Ok(())
    }
}

/// Type alias for backward compatibility
pub type AcpClient = AcpConnection;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_mode_id() {
        let id = SessionModeId::new("code");
        assert_eq!(id.as_str(), "code");
    }

    #[test]
    fn test_model_id() {
        let id = ModelId::new("claude-3-opus");
        assert_eq!(id.as_str(), "claude-3-opus");
    }
}
