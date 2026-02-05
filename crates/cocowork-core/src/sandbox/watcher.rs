//! File system watcher for change detection

use crate::error::{Error, Result, SandboxError};
use crate::types::*;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, Debouncer};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/// File watcher for detecting changes in granted paths
pub struct FileWatcher {
    /// Active watchers by path
    watchers: HashMap<PathBuf, WatcherHandle>,
    /// Baseline snapshots for change detection
    baselines: HashMap<String, BaselineSnapshot>,
    /// Active tool calls for attribution
    active_tool_calls: Vec<ActiveToolCall>,
    /// Channel for file change events
    event_tx: Option<mpsc::Sender<FileChangeEvent>>,
}

struct WatcherHandle {
    _watcher: Debouncer<RecommendedWatcher>,
}

/// Baseline snapshot of a directory
#[derive(Debug, Clone)]
pub struct BaselineSnapshot {
    #[allow(dead_code)]
    pub session_id: String,
    pub path: PathBuf,
    pub files: HashMap<PathBuf, FileSnapshot>,
    #[allow(dead_code)]
    pub captured_at: chrono::DateTime<chrono::Utc>,
}

/// Snapshot of a single file
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub size: u64,
    pub modified: Option<chrono::DateTime<chrono::Utc>>,
    #[allow(dead_code)]
    pub hash: Option<String>,
}

/// Active tool call for attribution tracking
#[derive(Debug, Clone)]
struct ActiveToolCall {
    id: String,
    method: String,
    started_at: chrono::DateTime<chrono::Utc>,
    expected_paths: Vec<PathBuf>,
}

/// File change event
#[derive(Debug, Clone)]
pub struct FileChangeEvent {
    pub path: PathBuf,
    pub event_type: FileChangeEventType,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeEventType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

impl FileWatcher {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
            baselines: HashMap::new(),
            active_tool_calls: Vec::new(),
            event_tx: None,
        }
    }

    /// Set the event channel
    pub fn set_event_channel(&mut self, tx: mpsc::Sender<FileChangeEvent>) {
        self.event_tx = Some(tx);
    }

    /// Start watching a path
    pub fn watch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();

        if self.watchers.contains_key(&path) {
            debug!("Already watching: {:?}", path);
            return Ok(());
        }

        info!("Starting watch on: {:?}", path);

        let event_tx = self.event_tx.clone();

        let (tx, rx) = std::sync::mpsc::channel();

        let mut debouncer = new_debouncer(Duration::from_millis(500), tx).map_err(|e| {
            Error::Sandbox(SandboxError::WatchError(format!(
                "Failed to create watcher: {}",
                e
            )))
        })?;

        debouncer
            .watcher()
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|e| {
                Error::Sandbox(SandboxError::WatchError(format!(
                    "Failed to watch path {:?}: {}",
                    path, e
                )))
            })?;

        // Spawn event processing task
        std::thread::spawn(move || {
            while let Ok(events) = rx.recv() {
                match events {
                    Ok(events) => {
                        for event in events {
                            let change_event = FileChangeEvent {
                                path: event.path.clone(),
                                event_type: FileChangeEventType::Modified, // Simplified
                                timestamp: chrono::Utc::now(),
                            };

                            if let Some(ref tx) = event_tx {
                                let _ = tx.blocking_send(change_event);
                            }
                        }
                    }
                    Err(e) => {
                        error!("Watch error: {:?}", e);
                    }
                }
            }
        });

        self.watchers.insert(path, WatcherHandle { _watcher: debouncer });
        Ok(())
    }

    /// Stop watching a path
    pub fn unwatch(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref().to_path_buf();

        if self.watchers.remove(&path).is_some() {
            info!("Stopped watching: {:?}", path);
        }

        Ok(())
    }

    /// Create a baseline snapshot for a session
    pub async fn create_baseline(
        &mut self,
        session_id: String,
        path: impl AsRef<Path>,
    ) -> Result<()> {
        let path = path.as_ref().to_path_buf();
        info!("Creating baseline for session {} at {:?}", session_id, path);

        let files = Self::scan_directory(&path).await?;

        let snapshot = BaselineSnapshot {
            session_id: session_id.clone(),
            path,
            files,
            captured_at: chrono::Utc::now(),
        };

        self.baselines.insert(session_id, snapshot);
        Ok(())
    }

    /// Scan directory and create file snapshots
    async fn scan_directory(path: &Path) -> Result<HashMap<PathBuf, FileSnapshot>> {
        let mut files = HashMap::new();

        for entry in walkdir::WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                let metadata = entry.metadata().ok();

                let snapshot = FileSnapshot {
                    path: entry.path().to_path_buf(),
                    size: metadata.as_ref().map(|m| m.len()).unwrap_or(0),
                    modified: metadata.and_then(|m| m.modified().ok()).map(|t| t.into()),
                    hash: None, // Don't compute hash for baseline (expensive)
                };

                files.insert(entry.path().to_path_buf(), snapshot);
            }
        }

        Ok(files)
    }

    /// Compare current state with baseline and return changes
    pub async fn get_changes_since_baseline(
        &self,
        session_id: &str,
    ) -> Result<Vec<DetectedFileChange>> {
        let baseline = self.baselines.get(session_id).ok_or_else(|| {
            Error::Sandbox(SandboxError::WatchError(format!(
                "No baseline found for session {}",
                session_id
            )))
        })?;

        let current_files = Self::scan_directory(&baseline.path).await?;
        let mut changes = Vec::new();

        // Find created and modified files
        for (path, current) in &current_files {
            if let Some(original) = baseline.files.get(path) {
                // Check if modified
                if current.size != original.size
                    || current.modified != original.modified
                {
                    changes.push(DetectedFileChange {
                        path: path.clone(),
                        change_type: FileChangeType::Modified,
                        size_before: Some(original.size),
                        size_after: Some(current.size),
                    });
                }
            } else {
                // New file
                changes.push(DetectedFileChange {
                    path: path.clone(),
                    change_type: FileChangeType::Created,
                    size_before: None,
                    size_after: Some(current.size),
                });
            }
        }

        // Find deleted files
        for path in baseline.files.keys() {
            if !current_files.contains_key(path) {
                changes.push(DetectedFileChange {
                    path: path.clone(),
                    change_type: FileChangeType::Deleted,
                    size_before: baseline.files.get(path).map(|s| s.size),
                    size_after: None,
                });
            }
        }

        Ok(changes)
    }

    /// Register an active tool call for attribution
    pub fn register_tool_call(
        &mut self,
        tool_call_id: String,
        method: String,
        expected_paths: Vec<PathBuf>,
    ) {
        self.active_tool_calls.push(ActiveToolCall {
            id: tool_call_id,
            method,
            started_at: chrono::Utc::now(),
            expected_paths,
        });
    }

    /// Complete a tool call
    pub fn complete_tool_call(&mut self, tool_call_id: &str) {
        self.active_tool_calls.retain(|tc| tc.id != tool_call_id);
    }

    /// Attribute a file change to a tool call
    pub fn attribute_change(&self, path: &Path) -> FileChangeAttribution {
        let now = chrono::Utc::now();

        // Look for active tool calls that might have caused this change
        for tc in &self.active_tool_calls {
            // Check if this path was expected
            if tc.expected_paths.iter().any(|p| p == path) {
                return FileChangeAttribution::AcpOperation {
                    tool_call_id: tc.id.clone(),
                    method: tc.method.clone(),
                };
            }

            // Check if the change happened during the tool call (within reasonable time)
            let elapsed = now - tc.started_at;
            if elapsed < chrono::Duration::seconds(60) {
                return FileChangeAttribution::Inferred {
                    probable_tool_call_id: Some(tc.id.clone()),
                    confidence: 0.8,
                };
            }
        }

        // No matching tool call found
        if self.active_tool_calls.is_empty() {
            FileChangeAttribution::UserAction
        } else {
            FileChangeAttribution::Inferred {
                probable_tool_call_id: None,
                confidence: 0.3,
            }
        }
    }

    /// Clear baseline for a session
    pub fn clear_baseline(&mut self, session_id: &str) {
        self.baselines.remove(session_id);
    }

    /// Check if any paths are being watched
    pub fn is_watching(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        self.watchers.keys().any(|p| path.starts_with(p))
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Detected file change from baseline comparison
#[derive(Debug, Clone)]
pub struct DetectedFileChange {
    pub path: PathBuf,
    pub change_type: FileChangeType,
    pub size_before: Option<u64>,
    pub size_after: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_file_watcher_creation() {
        let watcher = FileWatcher::new();
        assert!(watcher.watchers.is_empty());
        assert!(watcher.baselines.is_empty());
    }

    #[test]
    fn test_attribution_user_action() {
        let watcher = FileWatcher::new();
        let attr = watcher.attribute_change(Path::new("/some/path"));

        match attr {
            FileChangeAttribution::UserAction => {}
            _ => panic!("Expected UserAction attribution"),
        }
    }

    #[test]
    fn test_attribution_with_tool_call() {
        let mut watcher = FileWatcher::new();
        let path = PathBuf::from("/test/file.txt");

        watcher.register_tool_call(
            "tc-1".to_string(),
            "fs/write_file".to_string(),
            vec![path.clone()],
        );

        let attr = watcher.attribute_change(&path);

        match attr {
            FileChangeAttribution::AcpOperation { tool_call_id, method } => {
                assert_eq!(tool_call_id, "tc-1");
                assert_eq!(method, "fs/write_file");
            }
            _ => panic!("Expected AcpOperation attribution"),
        }
    }

    #[tokio::test]
    async fn test_baseline_and_changes() {
        let dir = tempdir().unwrap();
        let mut watcher = FileWatcher::new();

        // Create initial file
        std::fs::write(dir.path().join("file1.txt"), "initial").unwrap();

        // Create baseline
        watcher
            .create_baseline("session-1".to_string(), dir.path())
            .await
            .unwrap();

        // Make changes
        std::fs::write(dir.path().join("file1.txt"), "modified content").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "new file").unwrap();

        // Get changes
        let changes = watcher.get_changes_since_baseline("session-1").await.unwrap();

        // Should have 2 changes: modified file1 and created file2
        assert_eq!(changes.len(), 2);

        let has_modified = changes
            .iter()
            .any(|c| c.change_type == FileChangeType::Modified);
        let has_created = changes
            .iter()
            .any(|c| c.change_type == FileChangeType::Created);

        assert!(has_modified);
        assert!(has_created);
    }
}
