//! File system operations with permission checks

use super::permissions::PermissionManager;
use crate::error::{Error, Result, SandboxError};
use crate::types::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};
use walkdir::WalkDir;

/// File system handler with permission checking
pub struct FileSystemHandler;

impl FileSystemHandler {
    /// Read a text file with permission check
    pub async fn read_text_file(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<String> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Reading file: {:?}", path);

        let content = fs::read_to_string(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Sandbox(SandboxError::FileNotFound(path.to_string_lossy().to_string()))
            } else {
                Error::Io(e)
            }
        })?;

        Ok(content)
    }

    /// Read a file as bytes with permission check
    pub async fn read_file_bytes(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<Vec<u8>> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Reading file bytes: {:?}", path);

        let content = fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Sandbox(SandboxError::FileNotFound(path.to_string_lossy().to_string()))
            } else {
                Error::Io(e)
            }
        })?;

        Ok(content)
    }

    /// Write a file with permission check
    pub async fn write_file(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
        content: &str,
    ) -> Result<FileWriteResult> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Writing file: {:?}", path);

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        let existed_before = path.exists();
        let hash_before = if existed_before {
            Some(Self::compute_file_hash(path).await?)
        } else {
            None
        };

        fs::write(path, content).await?;

        let hash_after = Self::compute_file_hash(path).await?;
        let size = content.len() as u64;

        info!("Wrote {} bytes to {:?}", size, path);

        Ok(FileWriteResult {
            path: path.to_string_lossy().to_string(),
            created: !existed_before,
            size,
            hash_before,
            hash_after,
        })
    }

    /// Write bytes to a file with permission check
    pub async fn write_file_bytes(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
        content: &[u8],
    ) -> Result<FileWriteResult> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Writing file bytes: {:?}", path);

        if let Some(parent) = path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        let existed_before = path.exists();
        let hash_before = if existed_before {
            Some(Self::compute_file_hash(path).await?)
        } else {
            None
        };

        fs::write(path, content).await?;

        let hash_after = Self::compute_file_hash(path).await?;
        let size = content.len() as u64;

        Ok(FileWriteResult {
            path: path.to_string_lossy().to_string(),
            created: !existed_before,
            size,
            hash_before,
            hash_after,
        })
    }

    /// Delete a file with permission check
    pub async fn delete_file(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Deleting file: {:?}", path);

        if !path.exists() {
            return Err(Error::Sandbox(SandboxError::FileNotFound(
                path.to_string_lossy().to_string(),
            )));
        }

        if path.is_dir() {
            fs::remove_dir_all(path).await?;
        } else {
            fs::remove_file(path).await?;
        }

        info!("Deleted: {:?}", path);
        Ok(())
    }

    /// Move/rename a file with permission check
    pub async fn move_file(
        permission_manager: &PermissionManager,
        from: impl AsRef<Path>,
        to: impl AsRef<Path>,
    ) -> Result<()> {
        let from = from.as_ref();
        let to = to.as_ref();

        permission_manager.validate_access(from)?;
        permission_manager.validate_access(to)?;

        debug!("Moving {:?} to {:?}", from, to);

        if !from.exists() {
            return Err(Error::Sandbox(SandboxError::FileNotFound(
                from.to_string_lossy().to_string(),
            )));
        }

        // Create parent directories for destination
        if let Some(parent) = to.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }

        fs::rename(from, to).await?;
        info!("Moved {:?} to {:?}", from, to);

        Ok(())
    }

    /// Create a directory with permission check
    pub async fn create_directory(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        debug!("Creating directory: {:?}", path);
        fs::create_dir_all(path).await?;
        info!("Created directory: {:?}", path);

        Ok(())
    }

    /// List directory contents with permission check
    pub async fn list_directory(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<Vec<FileMetadata>> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        if !path.exists() {
            return Err(Error::Sandbox(SandboxError::DirectoryNotFound(
                path.to_string_lossy().to_string(),
            )));
        }

        if !path.is_dir() {
            return Err(Error::Sandbox(SandboxError::InvalidPath(
                format!("{:?} is not a directory", path),
            )));
        }

        debug!("Listing directory: {:?}", path);

        let mut entries = Vec::new();
        let mut read_dir = fs::read_dir(path).await?;

        while let Some(entry) = read_dir.next_entry().await? {
            let metadata = entry.metadata().await?;
            let entry_path = entry.path();

            let file_metadata = FileMetadata {
                path: entry_path.to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
                modified: metadata.modified().ok().map(|t| t.into()),
                mime_type: if metadata.is_file() {
                    Some(
                        mime_guess::from_path(&entry_path)
                            .first_or_octet_stream()
                            .to_string(),
                    )
                } else {
                    None
                },
            };

            entries.push(file_metadata);
        }

        // Sort: directories first, then by name
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        Ok(entries)
    }

    /// List directory recursively with depth limit
    pub fn list_directory_recursive(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
        max_depth: usize,
    ) -> Result<Vec<FileMetadata>> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        if !path.exists() {
            return Err(Error::Sandbox(SandboxError::DirectoryNotFound(
                path.to_string_lossy().to_string(),
            )));
        }

        let mut entries = Vec::new();

        for entry in WalkDir::new(path)
            .max_depth(max_depth)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let metadata = match entry.metadata() {
                Ok(m) => m,
                Err(_) => continue,
            };

            let entry_path = entry.path();

            // Skip the root directory itself
            if entry_path == path {
                continue;
            }

            let file_metadata = FileMetadata {
                path: entry_path.to_string_lossy().to_string(),
                name: entry.file_name().to_string_lossy().to_string(),
                is_dir: metadata.is_dir(),
                size: if metadata.is_file() {
                    Some(metadata.len())
                } else {
                    None
                },
                modified: metadata.modified().ok().map(|t| t.into()),
                mime_type: if metadata.is_file() {
                    Some(
                        mime_guess::from_path(entry_path)
                            .first_or_octet_stream()
                            .to_string(),
                    )
                } else {
                    None
                },
            };

            entries.push(file_metadata);
        }

        Ok(entries)
    }

    /// Get file metadata
    pub async fn get_metadata(
        permission_manager: &PermissionManager,
        path: impl AsRef<Path>,
    ) -> Result<FileMetadata> {
        let path = path.as_ref();
        permission_manager.validate_access(path)?;

        let metadata = fs::metadata(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                Error::Sandbox(SandboxError::FileNotFound(path.to_string_lossy().to_string()))
            } else {
                Error::Io(e)
            }
        })?;

        Ok(FileMetadata {
            path: path.to_string_lossy().to_string(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() {
                Some(metadata.len())
            } else {
                None
            },
            modified: metadata.modified().ok().map(|t| t.into()),
            mime_type: if metadata.is_file() {
                Some(
                    mime_guess::from_path(path)
                        .first_or_octet_stream()
                        .to_string(),
                )
            } else {
                None
            },
        })
    }

    /// Compute SHA256 hash of a file
    pub async fn compute_file_hash(path: impl AsRef<Path>) -> Result<String> {
        let path = path.as_ref();
        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hex::encode(hasher.finalize()))
    }
}

/// Result of a file write operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FileWriteResult {
    pub path: String,
    pub created: bool,
    pub size: u64,
    pub hash_before: Option<String>,
    pub hash_after: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_read_write_file() {
        let dir = tempdir().unwrap();
        let mut manager = PermissionManager::new();
        manager
            .grant_access(dir.path(), super::super::permissions::SecurityLevel::Trust)
            .unwrap();

        let file_path = dir.path().join("test.txt");
        let content = "Hello, World!";

        // Write
        let result = FileSystemHandler::write_file(&manager, &file_path, content)
            .await
            .unwrap();
        assert!(result.created);
        assert_eq!(result.size, content.len() as u64);

        // Read
        let read_content = FileSystemHandler::read_text_file(&manager, &file_path)
            .await
            .unwrap();
        assert_eq!(read_content, content);
    }

    #[tokio::test]
    async fn test_list_directory() {
        let dir = tempdir().unwrap();
        let mut manager = PermissionManager::new();
        manager
            .grant_access(dir.path(), super::super::permissions::SecurityLevel::Trust)
            .unwrap();

        // Create some files
        std::fs::write(dir.path().join("file1.txt"), "content1").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "content2").unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();

        let entries = FileSystemHandler::list_directory(&manager, dir.path())
            .await
            .unwrap();

        assert_eq!(entries.len(), 3);
        // Directories should come first
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "subdir");
    }

    #[tokio::test]
    async fn test_permission_denied() {
        let manager = PermissionManager::new();

        let result = FileSystemHandler::read_text_file(&manager, "/etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_move_file() {
        let dir = tempdir().unwrap();
        let mut manager = PermissionManager::new();
        manager
            .grant_access(dir.path(), super::super::permissions::SecurityLevel::Trust)
            .unwrap();

        let from_path = dir.path().join("original.txt");
        let to_path = dir.path().join("moved.txt");

        std::fs::write(&from_path, "content").unwrap();

        FileSystemHandler::move_file(&manager, &from_path, &to_path)
            .await
            .unwrap();

        assert!(!from_path.exists());
        assert!(to_path.exists());
    }

    #[tokio::test]
    async fn test_file_hash() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("hash_test.txt");
        std::fs::write(&file_path, "test content").unwrap();

        let hash = FileSystemHandler::compute_file_hash(&file_path)
            .await
            .unwrap();

        // SHA256 hash is 64 hex characters
        assert_eq!(hash.len(), 64);
    }
}
