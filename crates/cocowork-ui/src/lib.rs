//! CocoWork UI Library
//!
//! GPUI-based desktop UI for CocoWork.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │ Window                                                              │
//! ├───────────────┬─────────────────────────────────┬───────────────────┤
//! │ Sidebar       │  MainPanel                      │ ContextPanel      │
//! │ (220px)       │  (flex-1)                       │ (280px)           │
//! │               │                                 │                   │
//! │ ┌───────────┐ │  ┌─────────────────────────┐   │ ┌───────────────┐ │
//! │ │SearchInput│ │  │SessionHeader            │   │ │Collapsible    │ │
//! │ └───────────┘ │  └─────────────────────────┘   │ │"State"        │ │
//! │               │                                 │ └───────────────┘ │
//! │ ┌───────────┐ │  ┌─────────────────────────┐   │                   │
//! │ │TopicsTree │ │  │MessageList              │   │ ┌───────────────┐ │
//! │ │           │ │  │                         │   │ │Collapsible    │ │
//! │ │ TreeItem  │ │  │                         │   │ │"Artifacts"    │ │
//! │ │ TreeItem  │ │  │                         │   │ └───────────────┘ │
//! │ │  └─Item   │ │  │                         │   │                   │
//! │ │ TreeItem  │ │  │                         │   │ ┌───────────────┐ │
//! │ │           │ │  └─────────────────────────┘   │ │Collapsible    │ │
//! │ └───────────┘ │                                 │ │"Context"      │ │
//! │               │  ┌─────────────────────────┐   │ └───────────────┘ │
//! │               │  │InputBar                 │   │                   │
//! │               │  │ [TextInput] [Dropdowns] │   │                   │
//! │               │  └─────────────────────────┘   │                   │
//! └───────────────┴─────────────────────────────────┴───────────────────┘
//! ```

pub mod acp_integration;
pub mod components;
pub mod panels;
pub mod state;
pub mod theme;
pub mod views;

// Re-exports
pub use acp_integration::{AcpManager, AcpModel, AcpSession, ConnectionState};
pub use state::{AppState, ContextTab, SessionState, SimpleAppState, TopicNode};
pub use theme::{layout, Rgba, Spacing, Theme, ThemeColors, Typography};
