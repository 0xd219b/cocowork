//! ACP (Agent Client Protocol) implementation
//!
//! This module implements the client side of the ACP protocol for communicating
//! with AI agents via JSON-RPC over stdin/stdout.
//!
//! # Architecture
//!
//! The ACP implementation follows a trait-based architecture:
//!
//! - `AgentServer` - Represents an agent that can be connected to
//! - `AgentConnection` - An active connection to an agent
//! - `AgentClient` - Callback interface for handling agent requests
//!
//! The main implementation is `AcpConnection` which implements `AgentConnection`.

mod client_delegate;
mod connection;
mod protocol;
mod runtime;
mod session;
pub mod traits;
mod transport;

// Re-export core traits
pub use traits::{
    AgentClient, AgentConnection, AgentServer, AgentServerCommand, ConfigOptionId,
    ConfigValueType, LoadSessionResponse, ModelId, NewSessionResponse, PromptMessage,
    PromptResult, SessionConfigOption, SessionInfo, SessionMode, SessionModeId, SessionModel,
    SessionNotification,
};

// Re-export implementations
pub use client_delegate::AgentClientDelegate;
pub use connection::AcpConnection;
pub use protocol::{AcpMessage, ProtocolHandler};
pub use runtime::{spawn_runtime_tasks_headless, spawn_runtime_tasks_with_ui, AcpChannels};
pub use session::{Session, SessionManager};
pub use transport::Transport;

// Backward compatibility alias
pub use connection::AcpClient;
