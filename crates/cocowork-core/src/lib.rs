//! CocoWork Core Library
//!
//! This crate provides the core functionality for CocoWork, including:
//! - ACP (Agent Client Protocol) client and session management
//! - Agent lifecycle management
//! - File system sandboxing and permissions
//! - SQLite-based persistence
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     cocowork-core                           │
//! ├─────────────────────────────────────────────────────────────┤
//! │  acp/          - ACP protocol, client, sessions             │
//! │  agent/        - Agent configuration and lifecycle          │
//! │  sandbox/      - File permissions, watcher                  │
//! │  storage/      - SQLite database, queries                   │
//! │  types/        - Shared type definitions                    │
//! │  error.rs      - Error types                                │
//! └─────────────────────────────────────────────────────────────┘
//! ```

pub mod acp;
pub mod agent;
pub mod error;
pub mod sandbox;
pub mod storage;
pub mod types;

// Re-export commonly used types
pub use error::{Error, Result};
pub use types::*;

// Re-export ACP traits and implementations
pub use acp::{
    // Traits
    AgentClient, AgentConnection, AgentServer, AgentServerCommand,
    // ID types
    ConfigOptionId, ModelId, SessionModeId,
    // Session types
    ConfigValueType, LoadSessionResponse, NewSessionResponse, PromptMessage, PromptResult,
    SessionConfigOption, SessionInfo, SessionMode, SessionModel, SessionNotification,
    // Implementations
    AcpClient, AgentClientDelegate, AcpConnection, AcpMessage, ProtocolHandler, Session,
    SessionManager, AcpChannels, spawn_runtime_tasks_headless, spawn_runtime_tasks_with_ui,
};

// Re-export agent components
pub use agent::{
    AgentAdapterRegistry, AgentManager, AgentRegistry, AgentServerAdapter,
    ClaudeCodeAdapter, CodexAdapter, CustomAgentAdapter, GeminiAdapter, GooseAdapter,
};

// Re-export sandbox components
pub use sandbox::{
    FileOperation, FileSystemHandler, FileWatcher, PermissionManager, SecurityLevel,
    TerminalHandler,
};

// Re-export storage
pub use storage::Storage;
