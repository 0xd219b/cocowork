//! File system sandbox and permission management
//!
//! This module provides:
//! - Permission management for file access
//! - File system operations with permission checks
//! - File watching for change detection

mod filesystem;
pub mod permissions;
mod terminal;
mod watcher;

pub use filesystem::FileSystemHandler;
pub use permissions::{PermissionManager, SecurityLevel, FileOperation, PermissionEntry};
pub use terminal::TerminalHandler;
pub use watcher::FileWatcher;
