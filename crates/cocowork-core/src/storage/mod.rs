//! SQLite-based persistence layer
//!
//! This module provides:
//! - Database initialization and migrations
//! - CRUD operations for tasks, messages, artifacts, etc.
//! - Connection pooling

mod migrations;
mod queries;

pub use migrations::run_migrations;
pub use queries::*;

use crate::error::{Error, Result, StorageError};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::{Path, PathBuf};
use tracing::info;

/// Database connection pool type
pub type DbPool = Pool<SqliteConnectionManager>;

/// Storage manager for database operations
pub struct Storage {
    pool: DbPool,
    db_path: PathBuf,
}

impl Storage {
    /// Create a new storage instance with a directory path
    pub fn new_with_path(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref();

        // Create directory if it doesn't exist
        std::fs::create_dir_all(data_dir).map_err(|e| {
            Error::Storage(StorageError::Database(format!(
                "Failed to create data directory: {}",
                e
            )))
        })?;

        let db_path = data_dir.join("cocowork.db");
        info!("Database path: {:?}", db_path);

        Self::from_path(db_path)
    }

    /// Create storage from a specific path (useful for testing)
    pub fn from_path(db_path: PathBuf) -> Result<Self> {
        let manager = SqliteConnectionManager::file(&db_path);
        let pool = Pool::builder()
            .max_size(10)
            .build(manager)
            .map_err(|e| Error::Storage(StorageError::Pool(e.to_string())))?;

        let storage = Self { pool, db_path };

        // Run migrations
        storage.initialize()?;

        Ok(storage)
    }

    /// Create in-memory storage (for testing)
    pub fn in_memory() -> Result<Self> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder()
            .max_size(1)
            .build(manager)
            .map_err(|e| Error::Storage(StorageError::Pool(e.to_string())))?;

        let storage = Self {
            pool,
            db_path: PathBuf::from(":memory:"),
        };

        storage.initialize()?;

        Ok(storage)
    }

    /// Initialize database with migrations
    fn initialize(&self) -> Result<()> {
        let conn = self.pool.get()?;
        run_migrations(&conn)?;
        info!("Database initialized successfully");
        Ok(())
    }

    /// Get a connection from the pool
    pub fn connection(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>> {
        self.pool.get().map_err(|e| {
            Error::Storage(StorageError::Pool(e.to_string()))
        })
    }

    /// Get the database path
    pub fn db_path(&self) -> &PathBuf {
        &self.db_path
    }

    /// Get the connection pool
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_storage() {
        let storage = Storage::in_memory().unwrap();
        assert!(storage.connection().is_ok());
    }
}
