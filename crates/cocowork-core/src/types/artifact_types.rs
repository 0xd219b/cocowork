//! Artifact and file change types

use serde::{Deserialize, Serialize};

/// Artifact type enumeration
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    FileCreated,
    FileModified,
    FileDeleted,
    FileMoved,
    DirectoryCreated,
    AnalysisResult,
    TerminalOutput,
}

/// Artifact preview type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PreviewType {
    Text,
    Image,
    Spreadsheet,
    Pdf,
    Markdown,
    Binary,
    None,
}

impl PreviewType {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            // Text files
            "txt" | "log" | "json" | "xml" | "yaml" | "yml" | "toml" | "ini" | "cfg" |
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "html" | "css" | "scss" |
            "go" | "java" | "c" | "cpp" | "h" | "hpp" | "swift" | "kt" | "rb" |
            "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd" => Self::Text,

            // Markdown
            "md" | "mdx" | "markdown" => Self::Markdown,

            // Images
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "ico" | "bmp" => Self::Image,

            // Spreadsheets
            "csv" | "tsv" | "xlsx" | "xls" | "ods" => Self::Spreadsheet,

            // PDF
            "pdf" => Self::Pdf,

            // Binary/Unknown
            _ => Self::Binary,
        }
    }
}

/// Artifact representing a task output
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub id: String,
    pub task_id: String,

    /// Type of artifact
    pub artifact_type: ArtifactType,

    /// File information (when applicable)
    pub file: Option<ArtifactFile>,

    /// For moved files, the original path
    pub old_path: Option<String>,

    /// Source tracking
    pub source: ArtifactSource,

    /// Preview support
    pub preview: ArtifactPreview,

    /// Summary or description
    pub summary: Option<String>,

    /// Referenced files (from analysis results)
    pub referenced_files: Vec<String>,

    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Artifact {
    pub fn new_file_created(
        task_id: String,
        path: String,
        size: u64,
        hash: String,
        source: ArtifactSource,
    ) -> Self {
        let file = ArtifactFile::new(path, size, hash);
        let preview = ArtifactPreview::from_file(&file);

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::FileCreated,
            file: Some(file),
            old_path: None,
            source,
            preview,
            summary: None,
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_file_modified(
        task_id: String,
        path: String,
        size: u64,
        hash: String,
        source: ArtifactSource,
    ) -> Self {
        let file = ArtifactFile::new(path, size, hash);
        let preview = ArtifactPreview::from_file(&file);

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::FileModified,
            file: Some(file),
            old_path: None,
            source,
            preview,
            summary: None,
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_file_deleted(
        task_id: String,
        path: String,
        source: ArtifactSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::FileDeleted,
            file: Some(ArtifactFile {
                path,
                name: String::new(),
                extension: String::new(),
                mime_type: String::new(),
                size: 0,
                hash: String::new(),
            }),
            old_path: None,
            source,
            preview: ArtifactPreview::unsupported(),
            summary: None,
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_file_moved(
        task_id: String,
        old_path: String,
        new_path: String,
        size: u64,
        hash: String,
        source: ArtifactSource,
    ) -> Self {
        let file = ArtifactFile::new(new_path, size, hash);
        let preview = ArtifactPreview::from_file(&file);

        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::FileMoved,
            file: Some(file),
            old_path: Some(old_path),
            source,
            preview,
            summary: None,
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_directory_created(
        task_id: String,
        path: String,
        source: ArtifactSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::DirectoryCreated,
            file: Some(ArtifactFile {
                path: path.clone(),
                name: std::path::Path::new(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                extension: String::new(),
                mime_type: "inode/directory".to_string(),
                size: 0,
                hash: String::new(),
            }),
            old_path: None,
            source,
            preview: ArtifactPreview::unsupported(),
            summary: None,
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_analysis_result(
        task_id: String,
        summary: String,
        referenced_files: Vec<String>,
        source: ArtifactSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::AnalysisResult,
            file: None,
            old_path: None,
            source,
            preview: ArtifactPreview::unsupported(),
            summary: Some(summary),
            referenced_files,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn new_terminal_output(
        task_id: String,
        command: String,
        output: String,
        source: ArtifactSource,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            task_id,
            artifact_type: ArtifactType::TerminalOutput,
            file: None,
            old_path: None,
            source,
            preview: ArtifactPreview::unsupported(),
            summary: Some(format!("$ {}\n{}", command, output)),
            referenced_files: Vec::new(),
            created_at: chrono::Utc::now(),
        }
    }
}

/// File metadata for artifacts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactFile {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub mime_type: String,
    pub size: u64,
    pub hash: String,
}

impl ArtifactFile {
    pub fn new(path: String, size: u64, hash: String) -> Self {
        let path_obj = std::path::Path::new(&path);
        let name = path_obj
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let extension = path_obj
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let mime_type = mime_guess::from_path(&path)
            .first_or_octet_stream()
            .to_string();

        Self {
            path,
            name,
            extension,
            mime_type,
            size,
            hash,
        }
    }
}

/// Artifact source tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSource {
    /// Which layer captured this artifact (1, 2, or 3)
    pub layer: u8,
    /// Associated tool call ID
    pub tool_call_id: Option<String>,
    /// ACP method that created this (fs/write_file, terminal/create, etc.)
    pub method: Option<String>,
    /// Terminal command (if from terminal)
    pub command: Option<String>,
}

impl ArtifactSource {
    pub fn from_acp(tool_call_id: String, method: String) -> Self {
        Self {
            layer: 1,
            tool_call_id: Some(tool_call_id),
            method: Some(method),
            command: None,
        }
    }

    pub fn from_terminal(tool_call_id: String, command: String) -> Self {
        Self {
            layer: 1,
            tool_call_id: Some(tool_call_id),
            method: Some("terminal/create".to_string()),
            command: Some(command),
        }
    }

    pub fn from_file_watcher(probable_tool_call_id: Option<String>) -> Self {
        Self {
            layer: 2,
            tool_call_id: probable_tool_call_id,
            method: None,
            command: None,
        }
    }

    pub fn from_semantic_extraction() -> Self {
        Self {
            layer: 3,
            tool_call_id: None,
            method: None,
            command: None,
        }
    }
}

/// Preview support information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPreview {
    pub supported: bool,
    pub preview_type: PreviewType,
    pub thumbnail_path: Option<String>,
}

impl ArtifactPreview {
    pub fn from_file(file: &ArtifactFile) -> Self {
        let preview_type = PreviewType::from_extension(&file.extension);
        Self {
            supported: !matches!(preview_type, PreviewType::Binary | PreviewType::None),
            preview_type,
            thumbnail_path: None,
        }
    }

    pub fn unsupported() -> Self {
        Self {
            supported: false,
            preview_type: PreviewType::None,
            thumbnail_path: None,
        }
    }
}

/// File change tracking for sandbox
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub id: String,
    pub task_id: String,
    pub path: String,
    pub change_type: FileChangeType,
    pub old_path: Option<String>,
    pub size_before: Option<u64>,
    pub size_after: Option<u64>,
    pub hash_before: Option<String>,
    pub hash_after: Option<String>,
    pub attribution: FileChangeAttribution,
    pub tool_call_id: Option<String>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileChangeType {
    Created,
    Modified,
    Deleted,
    Renamed,
    Moved,
}

/// Attribution for file changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FileChangeAttribution {
    /// Definitely from an ACP operation
    AcpOperation {
        tool_call_id: String,
        method: String,
    },
    /// Inferred from timing (happened during tool call)
    Inferred {
        probable_tool_call_id: Option<String>,
        confidence: f32,
    },
    /// User's own action (no active tool call)
    UserAction,
}
