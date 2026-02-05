//! ACP Integration for GPUI
//!
//! This module bridges the ACP client with the GPUI framework,
//! handling async message flow and UI updates.
//!
//! The integration supports the new trait-based ACP architecture with
//! mode/model/config dynamic management.

use cocowork_core::{
    AgentAdapterRegistry, AgentClientDelegate, AgentConfig, AgentConnection,
    ContentBlock, MessageBlock, PermissionManager, SessionModeId, SessionUpdate,
    SessionUpdateNotification, Storage, TaskState, TaskStatus, ToolCallState,
    // New types for mode/model support
    SessionMode, SessionModel, SessionConfigOption, ModelId, SessionNotification,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

// ============================================================================
// Connection State
// ============================================================================

/// Connection state for the ACP manager
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Connecting to agent
    Connecting,
    /// Connected and ready
    Connected,
    /// Connection failed
    Error,
}

impl Default for ConnectionState {
    fn default() -> Self {
        Self::Disconnected
    }
}

// ============================================================================
// ACP Session
// ============================================================================

/// ACP Connection state for a single agent session
pub struct AcpSession {
    /// Session ID (from ACP)
    pub session_id: String,
    /// Agent ID
    pub agent_id: String,
    /// Working directory
    pub working_dir: PathBuf,
    /// Current task state
    pub current_task: Option<TaskState>,
    /// Messages in this session
    pub messages: Vec<MessageBlock>,
    /// Whether the session is active
    pub is_active: bool,
    /// Whether we're waiting for a response
    pub is_loading: bool,
    /// Error message if any
    pub error: Option<String>,
    /// Available modes for this session
    pub available_modes: Vec<SessionMode>,
    /// Available models for this session
    pub available_models: Vec<SessionModel>,
    /// Current mode ID
    pub current_mode: Option<SessionModeId>,
    /// Current model ID
    pub current_model: Option<ModelId>,
    /// Configuration options
    pub config_options: Vec<SessionConfigOption>,
    /// Current streaming agent message (accumulates chunks)
    streaming_agent_message: Option<usize>,
    /// Current streaming thinking content (accumulates chunks)
    streaming_thinking: Option<usize>,
}

impl AcpSession {
    pub fn new(session_id: String, agent_id: String, working_dir: PathBuf) -> Self {
        Self {
            session_id,
            agent_id,
            working_dir,
            current_task: None,
            messages: Vec::new(),
            is_active: false,
            is_loading: false,
            error: None,
            available_modes: Vec::new(),
            available_models: Vec::new(),
            current_mode: None,
            current_model: None,
            config_options: Vec::new(),
            streaming_agent_message: None,
            streaming_thinking: None,
        }
    }

    /// Create a new session with modes and models
    pub fn with_modes_and_models(
        session_id: String,
        agent_id: String,
        working_dir: PathBuf,
        modes: Vec<SessionMode>,
        models: Vec<SessionModel>,
        config_options: Vec<SessionConfigOption>,
        current_mode: Option<SessionModeId>,
        current_model: Option<ModelId>,
    ) -> Self {
        Self {
            session_id,
            agent_id,
            working_dir,
            current_task: None,
            messages: Vec::new(),
            is_active: false,
            is_loading: false,
            error: None,
            available_modes: modes,
            available_models: models,
            current_mode,
            current_model,
            config_options,
            streaming_agent_message: None,
            streaming_thinking: None,
        }
    }

    /// Set the current mode
    pub fn set_mode(&mut self, mode_id: SessionModeId) {
        self.current_mode = Some(mode_id);
    }

    /// Set the current model
    pub fn set_model(&mut self, model_id: ModelId) {
        self.current_model = Some(model_id);
    }

    /// Add a user message (starts a new message)
    pub fn add_user_message(&mut self, content: Vec<ContentBlock>) {
        // End any streaming message when user sends a new message
        self.streaming_agent_message = None;
        self.streaming_thinking = None;
        self.messages.push(MessageBlock::user(content));
    }

    /// Append content to the current streaming agent message, or create a new one
    pub fn append_agent_content(&mut self, content: ContentBlock) {
        if let Some(idx) = self.streaming_agent_message {
            // Append to existing streaming message
            if let Some(msg) = self.messages.get_mut(idx) {
                if let MessageBlock::Agent { content: ref mut msg_content, .. } = msg {
                    msg_content.push(content);
                }
            }
        } else {
            // Create new agent message and start streaming
            let idx = self.messages.len();
            self.messages.push(MessageBlock::agent(vec![content]));
            self.streaming_agent_message = Some(idx);
        }
    }

    /// Append thinking content, accumulating into the current thinking block
    pub fn append_thinking_content(&mut self, content: ContentBlock) {
        if let Some(idx) = self.streaming_thinking {
            // Append to existing thinking block
            if let Some(msg) = self.messages.get_mut(idx) {
                if let MessageBlock::Thought { content: ref mut msg_content, .. } = msg {
                    msg_content.push(content);
                }
            }
        } else {
            // Create new thinking block
            let idx = self.messages.len();
            self.messages.push(MessageBlock::thought(vec![content]));
            self.streaming_thinking = Some(idx);
        }
    }

    /// Finish the current streaming response (called when prompt completes)
    pub fn finish_streaming(&mut self) {
        self.streaming_agent_message = None;
        self.streaming_thinking = None;
    }

    /// Add a complete agent message (non-streaming)
    pub fn add_agent_message(&mut self, content: Vec<ContentBlock>) {
        self.messages.push(MessageBlock::agent(content));
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }
}

// ============================================================================
// ACP Manager
// ============================================================================

/// Result of an async connection attempt
type ConnectionResult = std::result::Result<
    (Arc<dyn AgentConnection>, tokio::sync::broadcast::Receiver<SessionNotification>),
    String,
>;

/// Result of an async session creation
type SessionResult = std::result::Result<String, String>;

/// ACP Manager - manages agent connections and sessions
pub struct AcpManager {
    /// Available agent adapters (wrapped in Arc<RwLock> for sharing with async tasks)
    pub adapters: Arc<tokio::sync::RwLock<AgentAdapterRegistry>>,
    /// Active sessions by session ID
    pub sessions: HashMap<String, AcpSession>,
    /// Currently selected agent ID
    pub selected_agent_id: Option<String>,
    /// Connected agent connection (new architecture)
    pub connection: Option<Arc<dyn AgentConnection>>,
    /// Tokio runtime for async operations
    pub runtime: Arc<Runtime>,
    /// Storage
    storage: Arc<Storage>,
    /// Permission manager
    permission_manager: Arc<RwLock<PermissionManager>>,
    /// Notification receiver (subscribed once on connect)
    notification_rx: Option<tokio::sync::broadcast::Receiver<SessionNotification>>,
    /// Connection state
    pub connection_state: ConnectionState,
    /// Pending connection result receiver
    pending_connection_rx: Option<tokio::sync::oneshot::Receiver<ConnectionResult>>,
    /// Pending session creation result receiver
    pending_session_rx: Option<tokio::sync::oneshot::Receiver<SessionResult>>,
    /// Pending message to send after session is created
    pub pending_message: Option<String>,
    /// Error message from connection/session creation
    pub error_message: Option<String>,
    /// Auto-create session after connection (for new thread flow)
    auto_create_session: bool,
    /// Working directory for agent (user-selected workspace)
    working_dir: Option<PathBuf>,
}

impl AcpManager {
    pub fn new(runtime: Arc<Runtime>) -> Self {
        // Initialize storage
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cocowork");
        let storage = Arc::new(
            Storage::new_with_path(&data_dir).unwrap_or_else(|e| {
                warn!("Failed to open storage, using in-memory: {}", e);
                Storage::in_memory().expect("Failed to create in-memory storage")
            }),
        );

        // Initialize permission manager
        let permission_manager = Arc::new(RwLock::new(PermissionManager::new()));

        Self {
            adapters: Arc::new(tokio::sync::RwLock::new(AgentAdapterRegistry::with_builtins())),
            sessions: HashMap::new(),
            selected_agent_id: Some("claude-code".to_string()),
            connection: None,
            runtime,
            storage,
            permission_manager,
            notification_rx: None,
            connection_state: ConnectionState::Disconnected,
            pending_connection_rx: None,
            pending_session_rx: None,
            pending_message: None,
            error_message: None,
            auto_create_session: false,
            working_dir: None,
        }
    }

    /// Get all available agents
    pub fn available_agents(&self) -> Vec<AgentConfig> {
        self.adapters.blocking_read().configs()
    }

    /// Get the currently selected agent's config
    pub fn selected_agent_config(&self) -> Option<AgentConfig> {
        let adapters = self.adapters.blocking_read();
        self.selected_agent_id
            .as_ref()
            .and_then(|id| adapters.get(id))
            .map(|a| a.config())
    }

    /// Select an agent by ID
    pub fn select_agent(&mut self, agent_id: impl Into<String>) {
        self.selected_agent_id = Some(agent_id.into());
    }

    /// Set the working directory for the agent
    pub fn set_working_dir(&mut self, dir: Option<PathBuf>) {
        self.working_dir = dir;
    }

    /// Get the working directory (falls back to current dir if not set)
    pub fn get_working_dir(&self) -> PathBuf {
        self.working_dir
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    }

    /// Check if connected to an agent
    pub fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Connect to the selected agent using the new AgentServer architecture
    pub async fn connect(&mut self) -> Result<(), String> {
        let agent_id = self.selected_agent_id.clone().ok_or("No agent selected")?;

        self.connection_state = ConnectionState::Connecting;
        info!("Connecting to agent: {}", agent_id);

        // Get current working directory
        let cwd = std::env::current_dir().ok();

        // Create the delegate for handling agent requests
        let delegate = Arc::new(AgentClientDelegate::new(
            Arc::clone(&self.permission_manager),
            Arc::clone(&self.storage),
        ));

        // Connect using the new architecture
        let connection: Arc<dyn AgentConnection> = {
            let adapters = self.adapters.read().await;
            match adapters.connect(&agent_id, cwd.as_deref(), delegate).await {
                Ok(conn) => conn,
                Err(e) => {
                    self.connection_state = ConnectionState::Error;
                    return Err(format!("Failed to connect: {}", e));
                }
            }
        };

        // Subscribe to notifications ONCE and store the receiver
        let notification_rx = connection.subscribe_updates();
        self.notification_rx = Some(notification_rx);
        self.connection = Some(connection);
        self.connection_state = ConnectionState::Connected;

        info!("Connected to agent: {}", agent_id);
        Ok(())
    }

    /// Start connecting to the selected agent (non-blocking)
    /// Call poll_pending_operations() to check for completion
    pub fn start_connect(&mut self) {
        if self.connection_state == ConnectionState::Connecting {
            return; // Already connecting
        }
        if self.connection.is_some() {
            return; // Already connected
        }

        let agent_id = match self.selected_agent_id.clone() {
            Some(id) => id,
            None => {
                self.error_message = Some("No agent selected".to_string());
                return;
            }
        };

        self.connection_state = ConnectionState::Connecting;
        self.error_message = None;
        info!("Starting async connection to agent: {}", agent_id);

        // Create channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_connection_rx = Some(rx);

        // Clone what we need for the async task
        let adapters = Arc::clone(&self.adapters);
        let permission_manager = Arc::clone(&self.permission_manager);
        let storage = Arc::clone(&self.storage);
        let cwd = self.get_working_dir();

        // Spawn the connection task
        self.runtime.spawn(async move {
            let delegate = Arc::new(AgentClientDelegate::new(permission_manager, storage));

            let adapters_guard = adapters.read().await;
            let result: ConnectionResult = match adapters_guard.connect(&agent_id, Some(cwd.as_path()), delegate).await {
                Ok(connection) => {
                    let notification_rx: tokio::sync::broadcast::Receiver<SessionNotification> = connection.subscribe_updates();
                    Ok((connection, notification_rx))
                }
                Err(e) => Err(format!("Failed to connect: {}", e)),
            };

            let _ = tx.send(result);
        });
    }

    /// Start creating a session (non-blocking)
    /// Call poll_pending_operations() to check for completion
    pub fn start_create_session(&mut self, working_dir: PathBuf) {
        let connection = match &self.connection {
            Some(conn) => Arc::clone(conn),
            None => {
                self.error_message = Some("Not connected to agent".to_string());
                return;
            }
        };

        let _agent_id = self.selected_agent_id.clone().unwrap_or_default();
        info!("Starting async session creation");

        // Create channel for result
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_session_rx = Some(rx);

        // Clone sessions map key info
        let working_dir_clone = working_dir.clone();

        // Spawn the session creation task
        self.runtime.spawn(async move {
            match connection.new_session(working_dir_clone, vec![]).await {
                Ok(response) => {
                    let _ = tx.send(Ok(response.session_id));
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to create session: {}", e)));
                }
            }
        });

        // Store working dir for when session completes
        // We'll create the AcpSession when we get the result
    }

    /// Poll for completion of pending async operations
    /// Returns the newly created session ID if a session was just created
    pub fn poll_pending_operations(&mut self) -> Option<String> {
        let mut new_session_id = None;

        // Check pending connection
        if let Some(mut rx) = self.pending_connection_rx.take() {
            match rx.try_recv() {
                Ok(Ok((connection, notification_rx))) => {
                    info!("Async connection completed successfully");
                    self.connection = Some(connection);
                    self.notification_rx = Some(notification_rx);
                    self.connection_state = ConnectionState::Connected;

                    // Auto-create session if requested (new thread flow) or if there's a pending message
                    if self.auto_create_session || self.pending_message.is_some() {
                        let cwd = self.get_working_dir();
                        self.start_create_session(cwd);
                        self.auto_create_session = false; // Reset flag
                    }
                }
                Ok(Err(e)) => {
                    error!("Async connection failed: {}", e);
                    self.connection_state = ConnectionState::Error;
                    self.error_message = Some(e);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still pending, put it back
                    self.pending_connection_rx = Some(rx);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    // Channel closed without result
                    self.connection_state = ConnectionState::Error;
                    self.error_message = Some("Connection task cancelled".to_string());
                }
            }
        }

        // Check pending session creation
        if let Some(mut rx) = self.pending_session_rx.take() {
            match rx.try_recv() {
                Ok(Ok(session_id)) => {
                    info!("Async session creation completed: {}", session_id);
                    // Create the session object with user-selected working directory
                    let agent_id = self.selected_agent_id.clone().unwrap_or_default();
                    let working_dir = self.get_working_dir();
                    let session = AcpSession::new(session_id.clone(), agent_id, working_dir);
                    self.sessions.insert(session_id.clone(), session);
                    // Return the new session ID so caller can set it as active
                    new_session_id = Some(session_id);
                }
                Ok(Err(e)) => {
                    error!("Async session creation failed: {}", e);
                    self.error_message = Some(e);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still pending, put it back
                    self.pending_session_rx = Some(rx);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    self.error_message = Some("Session creation task cancelled".to_string());
                }
            }
        }

        new_session_id
    }

    /// Check if there's a pending operation
    pub fn has_pending_operation(&self) -> bool {
        self.pending_connection_rx.is_some() || self.pending_session_rx.is_some()
    }

    /// Create a new session with the connected agent
    pub async fn create_session(&mut self, working_dir: PathBuf) -> Result<String, String> {
        let connection = self.connection.as_ref().ok_or("Not connected to agent")?;
        let agent_id = self.selected_agent_id.clone().unwrap_or_default();

        // Create session using the new architecture
        let response = connection
            .new_session(working_dir.clone(), vec![])
            .await
            .map_err(|e| format!("Failed to create session: {}", e))?;

        let session_id = response.session_id.clone();

        // Create session with mode/model info from response
        let session = AcpSession::with_modes_and_models(
            session_id.clone(),
            agent_id,
            working_dir,
            response.modes,
            response.models,
            response.config_options,
            response.current_mode,
            response.current_model,
        );
        self.sessions.insert(session_id.clone(), session);

        info!("Created session: {}", session_id);
        Ok(session_id)
    }

    /// Send a prompt to a session
    pub async fn send_prompt(
        &mut self,
        session_id: &str,
        text: String,
        mode: Option<SessionModeId>,
    ) -> Result<(), String> {
        let connection = self.connection.as_ref().ok_or("Not connected to agent")?;

        // Add user message to session
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.add_user_message(vec![ContentBlock::Text { text: text.clone() }]);
            session.set_loading(true);
        }

        // Create prompt message
        let mut prompt_message =
            cocowork_core::PromptMessage::new(vec![ContentBlock::Text { text }]);
        if let Some(mode_id) = mode {
            prompt_message = prompt_message.with_mode(mode_id);
        }

        // Send to agent using streaming (non-blocking)
        connection
            .prompt_streaming(session_id.to_string(), prompt_message)
            .await
            .map_err(|e| format!("Failed to send prompt: {}", e))?;

        Ok(())
    }

    /// Poll for updates from the connection (call from GPUI event loop)
    pub fn poll_updates(&mut self) -> Vec<SessionNotification> {
        let mut updates = Vec::new();

        // Use the stored receiver instead of creating a new one
        if let Some(rx) = &mut self.notification_rx {
            loop {
                match rx.try_recv() {
                    Ok(notification) => {
                        debug!("UI received notification: {:?}", notification);
                        updates.push(notification);
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
                        warn!("Missed {} notifications due to lag", n);
                        // Continue receiving
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => {
                        warn!("Notification channel closed");
                        self.connection_state = ConnectionState::Disconnected;
                        break;
                    }
                }
            }
        }

        if !updates.is_empty() {
            info!("Polled {} updates from ACP", updates.len());
        }

        updates
    }

    /// Process a session notification
    pub fn process_notification(&mut self, notification: SessionNotification) {
        match notification {
            SessionNotification::Update(update_notification) => {
                info!("Processing session update for: {}", update_notification.session_id);
                self.process_session_update(update_notification);
            }
            SessionNotification::Disconnected => {
                warn!("Agent connection disconnected");
                self.connection = None;
                self.connection_state = ConnectionState::Disconnected;
            }
            SessionNotification::Error(err) => {
                error!("Agent error: {}", err);
            }
        }
    }

    /// Process a session update notification
    fn process_session_update(&mut self, notification: SessionUpdateNotification) {
        let session_id = notification.session_id.clone();

        if let Some(session) = self.sessions.get_mut(&session_id) {
            // Ensure we have a task state for tracking
            if session.current_task.is_none() {
                let working_dir = session.working_dir.to_string_lossy().to_string();
                session.current_task = Some(TaskState::new(
                    uuid::Uuid::new_v4().to_string(),
                    session_id.clone(),
                    session.agent_id.clone(),
                    Vec::new(),
                    working_dir,
                ));
            }

            // Match on the session update type
            match notification.update {
                SessionUpdate::AgentMessageChunk { content } => {
                    // Append to current streaming agent message
                    session.append_agent_content(content);
                }
                SessionUpdate::UserMessageChunk { content } => {
                    debug!("Received user message chunk: {:?}", content);
                }
                SessionUpdate::Thought { content } => {
                    // Append to current streaming thinking block
                    session.append_thinking_content(content);
                }
                SessionUpdate::ToolCall {
                    tool_call_id,
                    title,
                    kind,
                    status: _,
                } => {
                    debug!("Tool call started: {} ({:?})", tool_call_id, title);
                    // Split streaming content so any subsequent agent output appears *after* the tool call
                    session.finish_streaming();
                    if let Some(task) = &mut session.current_task {
                        let tool_call = ToolCallState::new(tool_call_id.clone(), title, kind);
                        task.tool_calls.insert(tool_call_id, tool_call);
                    }
                }
                SessionUpdate::ToolCallUpdate {
                    tool_call_id,
                    status,
                    content,
                } => {
                    debug!("Tool call update: {} - {:?}", tool_call_id, status);
                    if let Some(task) = &mut session.current_task {
                        if let Some(tc) = task.tool_calls.get_mut(&tool_call_id) {
                            tc.status = status;
                            if let Some(contents) = content {
                                tc.content.extend(contents);
                            }
                        }
                    }
                }
                SessionUpdate::Plan { entries } => {
                    debug!("Plan update: {} entries", entries.len());
                    if let Some(task) = &mut session.current_task {
                        task.plan = entries;
                        task.status = TaskStatus::Planning;
                    }
                }
                SessionUpdate::CurrentModeUpdate { mode_id } => {
                    debug!("Mode changed to: {}", mode_id);
                    if let Some(task) = &mut session.current_task {
                        task.context.current_mode = Some(mode_id);
                    }
                }
                SessionUpdate::AvailableCommandsUpdate { available_commands } => {
                    debug!(
                        "Available commands updated: {} commands",
                        available_commands.len()
                    );
                }
                SessionUpdate::PromptResponseReceived { stop_reason } => {
                    debug!("Prompt completed: {:?}", stop_reason);
                    session.is_loading = false;
                    session.finish_streaming();
                    if let Some(task) = &mut session.current_task {
                        task.stop_reason = stop_reason;
                        task.status = TaskStatus::Completed;
                    }
                }
            }
        }
    }

    /// Get a session by ID
    pub fn get_session(&self, session_id: &str) -> Option<&AcpSession> {
        self.sessions.get(session_id)
    }

    /// Get a mutable session by ID
    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut AcpSession> {
        self.sessions.get_mut(session_id)
    }

    /// Register a custom agent
    pub fn register_custom_agent(&mut self, config: AgentConfig) {
        self.adapters.blocking_write().register_custom(config);
    }
}

impl Default for AcpManager {
    fn default() -> Self {
        let runtime = Arc::new(Runtime::new().expect("Failed to create Tokio runtime"));
        Self::new(runtime)
    }
}

// ============================================================================
// ACP Model (GPUI wrapper)
// ============================================================================

/// GPUI Model for ACP state
/// This wraps AcpManager and provides GPUI-specific functionality
pub struct AcpModel {
    pub manager: AcpManager,
    /// Pending input text
    pub input_text: String,
    /// Active session ID
    pub active_session_id: Option<String>,
}

impl AcpModel {
    pub fn new() -> Self {
        Self {
            manager: AcpManager::default(),
            input_text: String::new(),
            active_session_id: None,
        }
    }

    pub fn with_runtime(runtime: Arc<Runtime>) -> Self {
        Self {
            manager: AcpManager::new(runtime),
            input_text: String::new(),
            active_session_id: None,
        }
    }

    /// Get the active session
    pub fn active_session(&self) -> Option<&AcpSession> {
        self.active_session_id
            .as_ref()
            .and_then(|id| self.manager.get_session(id))
    }

    /// Get the active session mutably
    pub fn active_session_mut(&mut self) -> Option<&mut AcpSession> {
        let id = self.active_session_id.clone()?;
        self.manager.get_session_mut(&id)
    }

    /// Connect to the selected agent and create a session
    /// This is a blocking call - use for initial setup or when blocking is acceptable
    pub fn connect_and_create_session(&mut self, working_dir: PathBuf) -> Option<String> {
        let runtime = Arc::clone(&self.manager.runtime);

        // Update state to show we're connecting
        self.manager.connection_state = ConnectionState::Connecting;

        // Block on async operations
        let result = runtime.block_on(async {
            // Connect if not connected
            if !self.manager.is_connected() {
                if let Err(e) = self.manager.connect().await {
                    error!("Failed to connect: {}", e);
                    return None;
                }
            }

            // Create session
            match self.manager.create_session(working_dir).await {
                Ok(session_id) => Some(session_id),
                Err(e) => {
                    error!("Failed to create session: {}", e);
                    None
                }
            }
        });

        if let Some(ref session_id) = result {
            self.active_session_id = Some(session_id.clone());
        }

        result
    }

    /// Get the current connection state
    pub fn connection_state(&self) -> ConnectionState {
        self.manager.connection_state
    }

    /// Check if there's a pending operation
    pub fn has_pending_operation(&self) -> bool {
        self.manager.has_pending_operation()
    }

    /// Get the pending message
    pub fn pending_message(&self) -> Option<&String> {
        self.manager.pending_message.as_ref()
    }

    /// Get the error message
    pub fn error_message(&self) -> Option<&String> {
        self.manager.error_message.as_ref()
    }

    /// Clear the error message
    pub fn clear_error(&mut self) {
        self.manager.error_message = None;
    }

    /// Start creating a new thread (non-blocking)
    /// This clears any active session and starts the connection/session creation flow
    pub fn start_new_thread(&mut self) {
        // Clear active session - we want a fresh thread
        self.active_session_id = None;

        // Start connection if not connected
        if !self.manager.is_connected() {
            // Set flag to auto-create session after connection
            self.manager.auto_create_session = true;
            self.manager.start_connect();
        } else {
            // Already connected - start creating a new session
            let cwd = self.manager.get_working_dir();
            self.manager.start_create_session(cwd);
        }
    }

    /// Start creating a new thread with a specific agent (non-blocking)
    /// This switches the agent, clears active session, and starts connection
    pub fn start_new_thread_with_agent(&mut self, agent_id: impl Into<String>) {
        let agent_id = agent_id.into();

        // Clear active session - we want a fresh thread
        self.active_session_id = None;

        // Disconnect if connected to a different agent
        if self.manager.selected_agent_id.as_ref() != Some(&agent_id) {
            self.manager.connection = None;
            self.manager.notification_rx = None;
            self.manager.connection_state = ConnectionState::Disconnected;
        }

        // Select the new agent
        self.manager.select_agent(&agent_id);

        // Set flag to auto-create session after connection
        self.manager.auto_create_session = true;

        // Start connection
        self.manager.start_connect();
    }

    /// Check if we're in the process of creating a new thread
    pub fn is_creating_thread(&self) -> bool {
        self.manager.has_pending_operation()
    }

    /// Start non-blocking message send flow
    /// If not connected, starts connection and queues the message
    /// Returns true if the message was either sent or queued for sending
    pub fn start_send_message(&mut self, text: String) -> bool {
        // If we have an active session and are connected, send immediately
        if let Some(session_id) = &self.active_session_id {
            if self.manager.is_connected() {
                // Add user message immediately
                if let Some(session) = self.manager.get_session_mut(session_id) {
                    session.add_user_message(vec![ContentBlock::Text { text: text.clone() }]);
                    session.set_loading(true);
                }

                // Send via ACP
                let runtime = Arc::clone(&self.manager.runtime);
                let connection = self.manager.connection.clone();
                let session_id = session_id.clone();

                if let Some(connection) = connection {
                    runtime.spawn(async move {
                        let prompt_message = cocowork_core::PromptMessage::new(vec![ContentBlock::Text {
                            text,
                        }]);
                        if let Err(e) = connection.prompt_streaming(session_id, prompt_message).await {
                            error!("Failed to send prompt: {}", e);
                        }
                    });
                }
                return true;
            }
        }

        // Not connected or no session - start the async flow
        // Queue the message
        self.manager.pending_message = Some(text);

        // Start connection if not already connecting
        if !self.manager.is_connected() && self.manager.connection_state != ConnectionState::Connecting {
            self.manager.start_connect();
        } else if self.manager.is_connected() && self.active_session_id.is_none() {
            // Connected but no session - start session creation
            let cwd = self.manager.get_working_dir();
            self.manager.start_create_session(cwd);
        }

        true
    }

    /// Create a local-only session for testing (does not connect to agent)
    #[cfg(test)]
    pub fn create_local_test_session(&mut self, working_dir: PathBuf) -> Option<String> {
        let agent_id = self.manager.selected_agent_id.clone()?;
        let session_id = uuid::Uuid::new_v4().to_string();

        let session = AcpSession::new(session_id.clone(), agent_id, working_dir);
        self.manager.sessions.insert(session_id.clone(), session);
        self.active_session_id = Some(session_id.clone());

        Some(session_id)
    }

    /// Send a message in the active session
    pub fn send_message(&mut self, text: String) -> bool {
        if let Some(session) = self.active_session_mut() {
            let content = vec![ContentBlock::Text { text }];
            session.add_user_message(content);
            session.set_loading(true);
            true
        } else {
            false
        }
    }

    /// Send a message via ACP (async)
    pub fn send_message_async(&mut self, text: String) {
        let session_id = match &self.active_session_id {
            Some(id) => id.clone(),
            None => return,
        };

        // Add user message immediately
        if let Some(session) = self.manager.get_session_mut(&session_id) {
            session.add_user_message(vec![ContentBlock::Text { text: text.clone() }]);
            session.set_loading(true);
        }

        // Send via ACP if connected
        if self.manager.is_connected() {
            let runtime = Arc::clone(&self.manager.runtime);
            let connection = self.manager.connection.clone();

            if let Some(connection) = connection {
                runtime.spawn(async move {
                    let prompt_message = cocowork_core::PromptMessage::new(vec![ContentBlock::Text {
                        text,
                    }]);
                    if let Err(e) = connection.prompt_streaming(session_id, prompt_message).await {
                        error!("Failed to send prompt: {}", e);
                    }
                });
            }
        }
    }

    /// Poll for updates and process them
    pub fn poll_and_process_updates(&mut self) {
        // Poll pending async operations (connection, session creation)
        // This returns the newly created session ID if one was just created
        let new_session_id = self.manager.poll_pending_operations();

        // Only set active session if a NEW session was just created
        // Don't auto-pick old sessions - that causes session reuse bugs
        if let Some(session_id) = new_session_id {
            info!("Setting newly created session as active: {}", session_id);
            self.active_session_id = Some(session_id.clone());

            // If there's a pending message, send it now
            if let Some(message) = self.manager.pending_message.take() {
                info!("Sending pending message to session: {}", session_id);
                // Add user message
                if let Some(session) = self.manager.get_session_mut(&session_id) {
                    session.add_user_message(vec![ContentBlock::Text { text: message.clone() }]);
                    session.set_loading(true);
                }

                // Send via ACP
                let runtime = Arc::clone(&self.manager.runtime);
                let connection = self.manager.connection.clone();

                if let Some(connection) = connection {
                    runtime.spawn(async move {
                        let prompt_message = cocowork_core::PromptMessage::new(vec![ContentBlock::Text {
                            text: message,
                        }]);
                        if let Err(e) = connection.prompt_streaming(session_id, prompt_message).await {
                            error!("Failed to send prompt: {}", e);
                        }
                    });
                }
            }
        }

        // Poll for session notifications
        let notifications = self.manager.poll_updates();
        for notification in notifications {
            self.manager.process_notification(notification);
        }
    }

    /// Get available agents
    pub fn available_agents(&self) -> Vec<AgentConfig> {
        self.manager.available_agents()
    }

    /// Get selected agent name
    pub fn selected_agent_name(&self) -> String {
        self.manager
            .selected_agent_config()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Select Agent".to_string())
    }

    /// Select an agent
    pub fn select_agent(&mut self, agent_id: impl Into<String>) {
        self.manager.select_agent(agent_id);
    }

    /// Set the working directory for the agent
    pub fn set_working_dir(&mut self, dir: Option<PathBuf>) {
        self.manager.set_working_dir(dir);
    }

    /// Get the current working directory
    pub fn get_working_dir(&self) -> PathBuf {
        self.manager.get_working_dir()
    }

    /// Check if currently loading
    pub fn is_loading(&self) -> bool {
        self.active_session().map(|s| s.is_loading).unwrap_or(false)
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.manager.is_connected()
    }

    /// Get messages from active session
    pub fn messages(&self) -> Vec<&MessageBlock> {
        self.active_session()
            .map(|s| s.messages.iter().collect())
            .unwrap_or_default()
    }

    /// Get error from active session
    pub fn error(&self) -> Option<&str> {
        self.active_session().and_then(|s| s.error.as_deref())
    }

    /// Get tool calls from active session
    pub fn tool_calls(&self) -> Vec<&ToolCallState> {
        self.active_session()
            .and_then(|s| s.current_task.as_ref())
            .map(|t| t.tool_calls.values().collect())
            .unwrap_or_default()
    }

    /// Get current task from active session
    pub fn current_task(&self) -> Option<&TaskState> {
        self.active_session()
            .and_then(|s| s.current_task.as_ref())
    }

    /// Clear session error
    pub fn clear_session_error(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.set_error(None);
        }
    }
}

impl Default for AcpModel {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acp_manager_creation() {
        let manager = AcpManager::default();
        assert!(manager.selected_agent_id.is_some());
        assert_eq!(manager.selected_agent_id.as_deref(), Some("claude-code"));
    }

    #[test]
    fn test_acp_model() {
        let mut model = AcpModel::new();

        // Create session (local test mode)
        let session_id = model.create_local_test_session(PathBuf::from("/tmp"));
        assert!(session_id.is_some());

        // Send message (local only)
        assert!(model.send_message("Hello".to_string()));

        // Check messages
        assert_eq!(model.messages().len(), 1);
    }
}
