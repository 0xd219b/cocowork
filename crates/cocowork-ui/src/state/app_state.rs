//! Main application state

use cocowork_core::{
    AgentManager, AgentState, MessageBlock, PermissionManager,
    SessionManager, Storage, TaskState,
};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use super::TopicNode;

/// Main application state
pub struct AppState {
    // === Backend Services ===
    /// Database storage
    pub storage: Arc<Storage>,
    /// Agent lifecycle manager
    pub agent_manager: Arc<Mutex<AgentManager>>,
    /// ACP session manager
    pub session_manager: Arc<Mutex<SessionManager>>,
    /// File permission manager
    pub permission_manager: Arc<RwLock<PermissionManager>>,

    // === UI Layout State ===
    /// Current sidebar width
    pub sidebar_width: f32,
    /// Context panel width
    pub context_panel_width: f32,
    /// Whether context panel is collapsed
    pub context_panel_collapsed: bool,
    /// Active context tab
    pub active_context_tab: ContextTab,

    // === Session Data ===
    /// Topic tree data (left sidebar)
    pub topics: Vec<TopicNode>,
    /// Currently active session
    pub active_session: Option<SessionState>,
    /// Available agents
    pub agents: Vec<AgentState>,
    /// Currently selected agent ID
    pub selected_agent_id: Option<String>,

    // === UI State ===
    /// Current input text
    pub input_text: String,
    /// Whether we're waiting for a response
    pub is_loading: bool,
    /// Error message to display
    pub error_message: Option<String>,
}

impl AppState {
    /// Create a new app state with services
    pub fn new(
        storage: Arc<Storage>,
        agent_manager: Arc<Mutex<AgentManager>>,
        session_manager: Arc<Mutex<SessionManager>>,
        permission_manager: Arc<RwLock<PermissionManager>>,
    ) -> Self {
        Self {
            storage,
            agent_manager,
            session_manager,
            permission_manager,

            sidebar_width: crate::theme::layout::SIDEBAR_WIDTH,
            context_panel_width: crate::theme::layout::CONTEXT_PANEL_WIDTH,
            context_panel_collapsed: false,
            active_context_tab: ContextTab::State,

            topics: Vec::new(),
            active_session: None,
            agents: Vec::new(),
            selected_agent_id: None,

            input_text: String::new(),
            is_loading: false,
            error_message: None,
        }
    }

    /// Initialize state with default data
    pub fn with_defaults(self) -> Self {
        // TODO: Load from storage
        self
    }

    /// Get the currently selected agent
    pub fn selected_agent(&self) -> Option<&AgentState> {
        self.selected_agent_id
            .as_ref()
            .and_then(|id| self.agents.iter().find(|a| &a.config.id == id))
    }

    /// Toggle context panel collapse state
    pub fn toggle_context_panel(&mut self) {
        self.context_panel_collapsed = !self.context_panel_collapsed;
    }

    /// Set active context tab
    pub fn set_context_tab(&mut self, tab: ContextTab) {
        self.active_context_tab = tab;
        if self.context_panel_collapsed {
            self.context_panel_collapsed = false;
        }
    }

    /// Clear error message
    pub fn clear_error(&mut self) {
        self.error_message = None;
    }

    /// Set error message
    pub fn set_error(&mut self, msg: impl Into<String>) {
        self.error_message = Some(msg.into());
    }
}

/// Context panel tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextTab {
    /// State/status view
    State,
    /// Artifacts list
    Artifacts,
    /// Context files
    Context,
}

/// Session state for the UI
#[derive(Debug, Clone)]
pub struct SessionState {
    /// Session ID
    pub id: String,
    /// Session title
    pub title: String,
    /// Agent ID
    pub agent_id: String,
    /// Working directory
    pub working_directory: String,
    /// Messages in this session
    pub messages: Vec<MessageBlock>,
    /// Current task state
    pub current_task: Option<TaskState>,
    /// Created timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl SessionState {
    /// Create a new session state
    pub fn new(id: String, agent_id: String, working_directory: String) -> Self {
        Self {
            id,
            title: "New session".to_string(),
            agent_id,
            working_directory,
            messages: Vec::new(),
            current_task: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// Add a message to the session
    pub fn add_message(&mut self, message: MessageBlock) {
        self.messages.push(message);
    }
}

/// Simple in-memory state for testing without backend services
pub struct SimpleAppState {
    pub sidebar_width: f32,
    pub context_panel_width: f32,
    pub context_panel_collapsed: bool,
    pub active_context_tab: ContextTab,
    pub topics: Vec<TopicNode>,
    pub input_text: String,
    pub is_loading: bool,
    pub selected_agent_name: String,
}

impl Default for SimpleAppState {
    fn default() -> Self {
        Self {
            sidebar_width: crate::theme::layout::SIDEBAR_WIDTH,
            context_panel_width: crate::theme::layout::CONTEXT_PANEL_WIDTH,
            context_panel_collapsed: false,
            active_context_tab: ContextTab::State,
            topics: Self::mock_topics(),
            input_text: String::new(),
            is_loading: false,
            selected_agent_name: "Default".to_string(),
        }
    }
}

impl SimpleAppState {
    /// Create mock topic data for testing
    fn mock_topics() -> Vec<TopicNode> {
        vec![
            TopicNode {
                id: "zed".to_string(),
                name: "zed".to_string(),
                icon: Some("folder".to_string()),
                is_expanded: true,
                children: vec![
                    TopicNode::leaf("rooms", "rooms"),
                    TopicNode::leaf("triage", "triage"),
                ],
            },
            TopicNode {
                id: "workstreams".to_string(),
                name: "workstreams".to_string(),
                icon: Some("folder".to_string()),
                is_expanded: false,
                children: vec![
                    TopicNode::leaf("open-src", "open-src"),
                    TopicNode::leaf("dev-cont", "dev-cont"),
                ],
            },
            TopicNode {
                id: "projects".to_string(),
                name: "projects".to_string(),
                icon: Some("folder".to_string()),
                is_expanded: true,
                children: vec![
                    TopicNode {
                        id: "git".to_string(),
                        name: "git".to_string(),
                        icon: Some("folder".to_string()),
                        is_expanded: false,
                        children: vec![TopicNode::leaf("demo-d", "demo-d")],
                    },
                    TopicNode::leaf("windows", "windows"),
                    TopicNode::leaf("upcoming", "upcoming"),
                ],
            },
            TopicNode::leaf("notes", "notes"),
        ]
    }
}
