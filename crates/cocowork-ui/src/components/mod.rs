//! Reusable UI components
//!
//! Basic building blocks for the CocoWork UI.

pub mod icon;
pub mod text_input;

pub use icon::{svg_icon, IconName, IconSize, chevron, status, agent, tool};
// Keep old exports for backward compatibility during migration
#[allow(deprecated)]
pub use icon::{icon, icons, OldIconSize};
pub use text_input::{TextInput, register_bindings as register_text_input_bindings};
