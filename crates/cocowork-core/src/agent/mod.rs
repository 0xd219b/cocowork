//! Agent lifecycle management
//!
//! This module handles:
//! - Agent configuration and registration
//! - Agent process lifecycle (start/stop)
//! - Agent status tracking
//! - Agent server adapters (Claude Code, Gemini, Codex, Custom)

mod adapter;
mod manager;
mod registry;

pub use adapter::{
    AgentAdapterRegistry, AgentServerAdapter,
    ClaudeCodeAdapter, CodexAdapter, CustomAgentAdapter, GeminiAdapter, GooseAdapter,
};
pub use manager::AgentManager;
pub use registry::AgentRegistry;
