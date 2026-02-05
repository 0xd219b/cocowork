//! Agent Client Delegate implementation
//!
//! This module provides an implementation of the AgentClient trait that delegates
//! file system, terminal, and permission requests to the appropriate handlers.

use super::traits::{AgentClient, SessionNotification};
use crate::error::Result;
use crate::sandbox::{FileOperation, FileSystemHandler, PermissionManager, TerminalHandler};
use crate::storage::Storage;
use crate::types::{FileMetadata, TerminalExecuteResult, TerminalPolicy};
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, warn};

/// Default implementation of AgentClient that uses the sandbox and storage systems
pub struct AgentClientDelegate {
    /// Permission manager for file access control
    permission_manager: Arc<RwLock<PermissionManager>>,
    /// Storage for settings
    storage: Arc<Storage>,
    /// Notification sender for UI updates
    notification_tx: Option<broadcast::Sender<SessionNotification>>,
}

impl AgentClientDelegate {
    /// Create a new delegate with the given permission manager and storage
    pub fn new(
        permission_manager: Arc<RwLock<PermissionManager>>,
        storage: Arc<Storage>,
    ) -> Self {
        Self {
            permission_manager,
            storage,
            notification_tx: None,
        }
    }

    /// Create a new delegate with notification support
    pub fn with_notifications(
        permission_manager: Arc<RwLock<PermissionManager>>,
        storage: Arc<Storage>,
        notification_tx: broadcast::Sender<SessionNotification>,
    ) -> Self {
        Self {
            permission_manager,
            storage,
            notification_tx: Some(notification_tx),
        }
    }

    /// Get the terminal policy from storage
    fn get_terminal_policy(&self) -> TerminalPolicy {
        let conn = match self.storage.connection() {
            Ok(c) => c,
            Err(e) => {
                warn!("Failed to get storage connection: {}", e);
                return TerminalPolicy::default();
            }
        };

        let raw = match crate::storage::get_setting(&conn, "terminal_policy") {
            Ok(r) => r,
            Err(e) => {
                warn!("Failed to get terminal policy: {}", e);
                return TerminalPolicy::default();
            }
        };

        raw.and_then(|v| serde_json::from_str::<TerminalPolicy>(&v).ok())
            .unwrap_or_default()
    }
}

#[async_trait]
impl AgentClient for AgentClientDelegate {
    async fn read_text_file(&self, session_id: &str, path: &str) -> Result<String> {
        debug!("Reading file for session {}: {}", session_id, path);
        let pm = self.permission_manager.read().await;
        FileSystemHandler::read_text_file(&pm, path).await
    }

    async fn write_text_file(&self, session_id: &str, path: &str, content: &str) -> Result<()> {
        debug!("Writing file for session {}: {}", session_id, path);
        let pm = self.permission_manager.read().await;

        if pm.requires_confirmation(path, FileOperation::Write) {
            return Err(crate::error::Error::Sandbox(
                crate::error::SandboxError::AccessDenied(format!(
                    "Write requires confirmation for: {}",
                    path
                )),
            ));
        }

        FileSystemHandler::write_file(&pm, path, content).await?;
        Ok(())
    }

    async fn list_directory(&self, session_id: &str, path: &str) -> Result<Vec<FileMetadata>> {
        debug!("Listing directory for session {}: {}", session_id, path);
        let pm = self.permission_manager.read().await;
        FileSystemHandler::list_directory(&pm, path).await
    }

    async fn delete_file(&self, session_id: &str, path: &str) -> Result<()> {
        debug!("Deleting file for session {}: {}", session_id, path);
        let pm = self.permission_manager.read().await;

        if pm.requires_confirmation(path, FileOperation::Delete) {
            return Err(crate::error::Error::Sandbox(
                crate::error::SandboxError::AccessDenied(format!(
                    "Delete requires confirmation for: {}",
                    path
                )),
            ));
        }

        FileSystemHandler::delete_file(&pm, path).await
    }

    async fn move_file(&self, session_id: &str, old_path: &str, new_path: &str) -> Result<()> {
        debug!(
            "Moving file for session {}: {} -> {}",
            session_id, old_path, new_path
        );
        let pm = self.permission_manager.read().await;

        if pm.requires_confirmation(old_path, FileOperation::Move)
            || pm.requires_confirmation(new_path, FileOperation::Move)
        {
            return Err(crate::error::Error::Sandbox(
                crate::error::SandboxError::AccessDenied(format!(
                    "Move requires confirmation: {} -> {}",
                    old_path, new_path
                )),
            ));
        }

        FileSystemHandler::move_file(&pm, old_path, new_path).await
    }

    async fn create_directory(&self, session_id: &str, path: &str) -> Result<()> {
        debug!("Creating directory for session {}: {}", session_id, path);
        let pm = self.permission_manager.read().await;

        if pm.requires_confirmation(path, FileOperation::Write) {
            return Err(crate::error::Error::Sandbox(
                crate::error::SandboxError::AccessDenied(format!(
                    "Create directory requires confirmation for: {}",
                    path
                )),
            ));
        }

        FileSystemHandler::create_directory(&pm, path).await
    }

    async fn execute_command(
        &self,
        session_id: &str,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        env: Option<&HashMap<String, String>>,
    ) -> Result<TerminalExecuteResult> {
        debug!(
            "Executing command for session {}: {} {:?}",
            session_id, command, args
        );

        // Validate cwd is inside granted paths when provided
        if let Some(cwd_path) = cwd {
            let pm = self.permission_manager.read().await;
            pm.validate_access(cwd_path)?;
        }

        let policy = self.get_terminal_policy();
        TerminalHandler::execute(&policy, command, args, cwd, env).await
    }

    async fn request_permission(
        &self,
        session_id: &str,
        operation: &str,
        resource: &str,
    ) -> Result<bool> {
        debug!(
            "Permission request for session {}: {} on {}",
            session_id, operation, resource
        );

        // For now, permissions are handled by the confirmation-based model
        // This method is a placeholder for future interactive permission requests
        let pm = self.permission_manager.read().await;

        let file_op = match operation {
            "read" => FileOperation::Read,
            "write" => FileOperation::Write,
            "delete" => FileOperation::Delete,
            "move" => FileOperation::Move,
            _ => FileOperation::Read,
        };

        // Return true if no confirmation is needed
        Ok(!pm.requires_confirmation(resource, file_op))
    }

    async fn on_session_notification(&self, notification: SessionNotification) -> Result<()> {
        if let Some(ref tx) = self.notification_tx {
            let _ = tx.send(notification);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_delegate_creation() {
        let pm = Arc::new(RwLock::new(PermissionManager::new()));
        let storage = Arc::new(Storage::in_memory().unwrap());

        let delegate = AgentClientDelegate::new(pm, storage);

        // Just verify it compiles and can be created
        assert!(delegate.notification_tx.is_none());
    }

    #[tokio::test]
    async fn test_delegate_with_notifications() {
        let pm = Arc::new(RwLock::new(PermissionManager::new()));
        let storage = Arc::new(Storage::in_memory().unwrap());
        let (tx, _rx) = broadcast::channel(16);

        let delegate = AgentClientDelegate::with_notifications(pm, storage, tx);

        assert!(delegate.notification_tx.is_some());
    }
}
