//! Error types for CocoWork Core

use thiserror::Error;

/// Main error type for CocoWork operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("ACP protocol error: {0}")]
    Acp(#[from] AcpError),

    #[error("Agent error: {0}")]
    Agent(#[from] AgentError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Sandbox error: {0}")]
    Sandbox(#[from] SandboxError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// ACP-specific errors
#[derive(Error, Debug)]
pub enum AcpError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Protocol version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u32, actual: u32 },

    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Session already exists: {0}")]
    SessionAlreadyExists(String),

    #[error("Request timeout")]
    Timeout,

    #[error("Agent not responding")]
    AgentNotResponding,

    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),
}

/// Agent management errors
#[derive(Error, Debug)]
pub enum AgentError {
    #[error("Agent not found: {0}")]
    NotFound(String),

    #[error("Agent already exists: {0}")]
    AlreadyExists(String),

    #[error("Agent not running: {0}")]
    NotRunning(String),

    #[error("Agent already running: {0}")]
    AlreadyRunning(String),

    #[error("Failed to start agent: {0}")]
    StartFailed(String),

    #[error("Failed to stop agent: {0}")]
    StopFailed(String),

    #[error("Invalid agent configuration: {0}")]
    InvalidConfig(String),

    #[error("Agent setup failed: {0}")]
    SetupFailed(String),
}

/// Storage errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Record not found: {0}")]
    NotFound(String),

    #[error("Duplicate key: {0}")]
    DuplicateKey(String),

    #[error("Migration failed: {0}")]
    MigrationFailed(String),

    #[error("Connection pool error: {0}")]
    Pool(String),
}

/// Sandbox/filesystem errors
#[derive(Error, Debug)]
pub enum SandboxError {
    #[error("Access denied: {0}")]
    AccessDenied(String),

    #[error("Path not granted: {0}")]
    PathNotGranted(String),

    #[error("Path outside sandbox: {0}")]
    PathOutsideSandbox(String),

    #[error("File not found: {0}")]
    FileNotFound(String),

    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),

    #[error("Watch error: {0}")]
    WatchError(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Self {
        Error::Storage(StorageError::Database(err.to_string()))
    }
}

impl From<r2d2::Error> for Error {
    fn from(err: r2d2::Error) -> Self {
        Error::Storage(StorageError::Pool(err.to_string()))
    }
}

impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;
