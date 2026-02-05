//! Database migrations

use crate::error::Result;
use rusqlite::Connection;
use tracing::{debug, info};

/// Run all database migrations
pub fn run_migrations(conn: &Connection) -> Result<()> {
    info!("Running database migrations");

    // Enable foreign keys
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    // Create migrations table
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
        );
        "#,
    )?;

    // Run migrations in order
    let migrations: Vec<(&str, &str)> = vec![
        ("001_initial", MIGRATION_001_INITIAL),
        ("002_agents", MIGRATION_002_AGENTS),
        ("003_settings", MIGRATION_003_SETTINGS),
    ];

    for (name, sql) in migrations {
        if !migration_applied(conn, name)? {
            debug!("Applying migration: {}", name);
            conn.execute_batch(sql)?;
            mark_migration_applied(conn, name)?;
            info!("Applied migration: {}", name);
        }
    }

    info!("All migrations completed");
    Ok(())
}

fn migration_applied(conn: &Connection, name: &str) -> Result<bool> {
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM migrations WHERE name = ?",
        [name],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn mark_migration_applied(conn: &Connection, name: &str) -> Result<()> {
    conn.execute("INSERT INTO migrations (name) VALUES (?)", [name])?;
    Ok(())
}

const MIGRATION_001_INITIAL: &str = r#"
-- Tasks table
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    agent_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    stop_reason TEXT,
    error_message TEXT,
    prompt_text TEXT,
    working_dir TEXT NOT NULL,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    completed_at DATETIME
);

CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_tasks_session ON tasks(session_id);
CREATE INDEX IF NOT EXISTS idx_tasks_agent ON tasks(agent_id);

-- Messages table
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content_type TEXT NOT NULL,
    content TEXT NOT NULL,
    seq_order INTEGER NOT NULL,
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_task ON messages(task_id, seq_order);

-- Tool calls table
CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    title TEXT,
    kind TEXT,
    status TEXT NOT NULL,
    raw_input TEXT,
    raw_output TEXT,
    content TEXT,
    started_at DATETIME,
    completed_at DATETIME
);

CREATE INDEX IF NOT EXISTS idx_tool_calls_task ON tool_calls(task_id);

-- Artifacts table
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    artifact_type TEXT NOT NULL,
    file_path TEXT,
    file_name TEXT,
    file_ext TEXT,
    mime_type TEXT,
    file_size INTEGER,
    file_hash TEXT,
    old_path TEXT,
    source_layer INTEGER,
    tool_call_id TEXT,
    summary TEXT,
    referenced_files TEXT,
    created_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_artifacts_task ON artifacts(task_id);

-- Plan snapshots table
CREATE TABLE IF NOT EXISTS plan_snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    entries TEXT NOT NULL,
    captured_at DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_plan_snapshots_task ON plan_snapshots(task_id);

-- File changes table
CREATE TABLE IF NOT EXISTS file_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    change_type TEXT NOT NULL,
    old_path TEXT,
    size_before INTEGER,
    size_after INTEGER,
    hash_before TEXT,
    hash_after TEXT,
    attribution TEXT NOT NULL,
    tool_call_id TEXT,
    timestamp DATETIME NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_changes_task ON file_changes(task_id);
"#;

const MIGRATION_002_AGENTS: &str = r#"
-- Agents configuration table
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    command TEXT NOT NULL,
    args TEXT,
    env TEXT,
    icon TEXT,
    builtin INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Agent statistics table
CREATE TABLE IF NOT EXISTS agent_stats (
    agent_id TEXT PRIMARY KEY REFERENCES agents(id) ON DELETE CASCADE,
    total_sessions INTEGER DEFAULT 0,
    successful_sessions INTEGER DEFAULT 0,
    failed_sessions INTEGER DEFAULT 0,
    total_tasks INTEGER DEFAULT 0,
    completed_tasks INTEGER DEFAULT 0,
    total_tool_calls INTEGER DEFAULT 0,
    avg_session_duration_secs REAL DEFAULT 0.0,
    last_used DATETIME
);
"#;

const MIGRATION_003_SETTINGS: &str = r#"
-- Application settings table
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- MCP servers configuration table
CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    args TEXT,
    env TEXT,
    transport TEXT DEFAULT 'stdio',
    enabled INTEGER DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL
);

-- Granted paths table (persisted permissions)
CREATE TABLE IF NOT EXISTS granted_paths (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    security_level TEXT NOT NULL DEFAULT 'auto_accept_edits',
    granted_at DATETIME NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migrations_run_successfully() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"tasks".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"tool_calls".to_string()));
        assert!(tables.contains(&"artifacts".to_string()));
        assert!(tables.contains(&"agents".to_string()));
        assert!(tables.contains(&"settings".to_string()));
    }

    #[test]
    fn test_migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        // Run twice
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Should still work
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM migrations", [], |row| row.get(0))
            .unwrap();

        assert_eq!(count, 3); // 3 migrations
    }
}
