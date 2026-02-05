//! Permission management for file system access

use crate::error::{Error, Result, SandboxError};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::info;

/// Security level for file operations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecurityLevel {
    /// Every write operation requires confirmation
    Strict,
    /// Auto-accept edits, but confirm deletes
    AutoAcceptEdits,
    /// Trust all operations (use with caution)
    Trust,
}

impl Default for SecurityLevel {
    fn default() -> Self {
        Self::AutoAcceptEdits
    }
}

/// Permission entry for a granted path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub path: PathBuf,
    pub security_level: SecurityLevel,
    pub granted_at: chrono::DateTime<chrono::Utc>,
    pub session_scoped: bool,
}

/// Permission manager for file system access
#[derive(Debug, Default)]
pub struct PermissionManager {
    /// Granted paths with their permission settings
    granted_paths: HashSet<PathBuf>,
    /// Permission entries with metadata
    entries: Vec<PermissionEntry>,
    /// Default security level for new paths
    default_security_level: SecurityLevel,
}

impl PermissionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant access to a path
    pub fn grant_access(&mut self, path: impl AsRef<Path>, security_level: SecurityLevel) -> Result<()> {
        let path = Self::normalize_path(path.as_ref())?;

        if self.granted_paths.contains(&path) {
            // Update existing entry
            if let Some(entry) = self.entries.iter_mut().find(|e| e.path == path) {
                entry.security_level = security_level;
            }
            return Ok(());
        }

        info!("Granting access to: {:?}", path);

        self.granted_paths.insert(path.clone());
        self.entries.push(PermissionEntry {
            path,
            security_level,
            granted_at: chrono::Utc::now(),
            session_scoped: false,
        });

        Ok(())
    }

    /// Revoke access to a path
    pub fn revoke_access(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = Self::normalize_path(path.as_ref())?;

        if self.granted_paths.remove(&path) {
            self.entries.retain(|e| e.path != path);
            info!("Revoked access to: {:?}", path);
        }

        Ok(())
    }

    /// Check if a path is accessible
    pub fn check_access(&self, path: impl AsRef<Path>) -> Result<bool> {
        let path = Self::normalize_path(path.as_ref())?;
        Ok(self.is_path_granted(&path))
    }

    /// Check if a path is within any granted path
    fn is_path_granted(&self, path: &Path) -> bool {
        for granted in &self.granted_paths {
            if path.starts_with(granted) {
                return true;
            }
        }
        false
    }

    /// Get security level for a path
    pub fn get_security_level(&self, path: impl AsRef<Path>) -> SecurityLevel {
        if let Ok(path) = Self::normalize_path(path.as_ref()) {
            for entry in &self.entries {
                if path.starts_with(&entry.path) {
                    return entry.security_level;
                }
            }
        }
        self.default_security_level
    }

    /// Validate access to a path, returning an error if denied
    pub fn validate_access(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = Self::normalize_path(path.as_ref())?;

        if !self.is_path_granted(&path) {
            return Err(Error::Sandbox(SandboxError::PathNotGranted(
                path.to_string_lossy().to_string(),
            )));
        }

        Ok(())
    }

    /// Check if operation requires confirmation
    pub fn requires_confirmation(&self, path: impl AsRef<Path>, operation: FileOperation) -> bool {
        let security = self.get_security_level(path);

        match (security, operation) {
            (SecurityLevel::Trust, _) => false,
            (SecurityLevel::AutoAcceptEdits, FileOperation::Write) => false,
            (SecurityLevel::AutoAcceptEdits, FileOperation::Read) => false,
            (SecurityLevel::AutoAcceptEdits, FileOperation::List) => false,
            (SecurityLevel::AutoAcceptEdits, FileOperation::Delete) => true,
            (SecurityLevel::AutoAcceptEdits, FileOperation::Move) => false,
            (SecurityLevel::AutoAcceptEdits, FileOperation::Execute) => true,
            (SecurityLevel::Strict, _) => operation != FileOperation::Read && operation != FileOperation::List,
        }
    }

    /// List all granted paths
    pub fn list_granted_paths(&self) -> Vec<PathBuf> {
        self.entries.iter().map(|e| e.path.clone()).collect()
    }

    /// Get all permission entries
    pub fn get_entries(&self) -> &[PermissionEntry] {
        &self.entries
    }

    /// Clear all session-scoped permissions
    pub fn clear_session_permissions(&mut self) {
        self.entries.retain(|e| !e.session_scoped);
        self.granted_paths = self.entries.iter().map(|e| e.path.clone()).collect();
    }

    /// Set default security level
    pub fn set_default_security_level(&mut self, level: SecurityLevel) {
        self.default_security_level = level;
    }

    /// Normalize and canonicalize a path
    fn normalize_path(path: &Path) -> Result<PathBuf> {
        // Expand home directory
        let expanded = if path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                home.join(path.strip_prefix("~").unwrap())
            } else {
                path.to_path_buf()
            }
        } else {
            path.to_path_buf()
        };

        // Canonicalize if the path exists
        if expanded.exists() {
            expanded.canonicalize().map_err(|e| {
                Error::Sandbox(SandboxError::InvalidPath(format!(
                    "Failed to canonicalize path {:?}: {}",
                    expanded, e
                )))
            })
        } else {
            // For non-existent paths, try to canonicalize the parent and append the rest
            // This handles cases where /tmp is a symlink to /private/tmp on macOS
            let mut current = expanded.clone();
            let mut remaining = Vec::new();

            while !current.exists() && current.parent().is_some() {
                if let Some(name) = current.file_name() {
                    remaining.push(name.to_owned());
                }
                current = current.parent().unwrap().to_path_buf();
            }

            let base = if current.exists() {
                current.canonicalize().unwrap_or(current)
            } else {
                current
            };

            // Reconstruct the path with canonicalized base
            let mut result = base;
            for part in remaining.into_iter().rev() {
                result = result.join(part);
            }

            Ok(Self::clean_path(&result))
        }
    }

    /// Clean a path without requiring it to exist
    fn clean_path(path: &Path) -> PathBuf {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    components.pop();
                }
                std::path::Component::CurDir => {}
                c => components.push(c),
            }
        }

        components.iter().collect()
    }
}

/// File operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileOperation {
    Read,
    Write,
    Delete,
    List,
    Move,
    Execute,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_permission_manager_grant_and_check() {
        let mut manager = PermissionManager::new();
        let dir = tempdir().unwrap();
        let path = dir.path();

        // Initially not granted
        assert!(!manager.check_access(path).unwrap());

        // Grant access
        manager.grant_access(path, SecurityLevel::AutoAcceptEdits).unwrap();
        assert!(manager.check_access(path).unwrap());

        // Subpath should also be accessible
        let subpath = path.join("subdir/file.txt");
        assert!(manager.check_access(&subpath).unwrap());
    }

    #[test]
    fn test_permission_manager_revoke() {
        let mut manager = PermissionManager::new();
        let dir = tempdir().unwrap();
        let path = dir.path();

        manager.grant_access(path, SecurityLevel::Trust).unwrap();
        assert!(manager.check_access(path).unwrap());

        manager.revoke_access(path).unwrap();
        assert!(!manager.check_access(path).unwrap());
    }

    #[test]
    fn test_security_levels() {
        let mut manager = PermissionManager::new();
        let dir = tempdir().unwrap();
        let path = dir.path();

        manager.grant_access(path, SecurityLevel::Strict).unwrap();

        // Strict requires confirmation for writes
        assert!(manager.requires_confirmation(path, FileOperation::Write));
        assert!(manager.requires_confirmation(path, FileOperation::Delete));
        assert!(!manager.requires_confirmation(path, FileOperation::Read));

        // Change to trust
        manager.grant_access(path, SecurityLevel::Trust).unwrap();
        assert!(!manager.requires_confirmation(path, FileOperation::Write));
        assert!(!manager.requires_confirmation(path, FileOperation::Delete));
    }

    #[test]
    fn test_validate_access_error() {
        let manager = PermissionManager::new();

        let result = manager.validate_access("/some/random/path");
        assert!(result.is_err());

        if let Err(Error::Sandbox(SandboxError::PathNotGranted(p))) = result {
            assert!(p.contains("random"));
        } else {
            panic!("Expected PathNotGranted error");
        }
    }

    #[test]
    fn test_list_granted_paths() {
        let mut manager = PermissionManager::new();
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        manager.grant_access(dir1.path(), SecurityLevel::Trust).unwrap();
        manager.grant_access(dir2.path(), SecurityLevel::Strict).unwrap();

        let paths = manager.list_granted_paths();
        assert_eq!(paths.len(), 2);
    }
}
