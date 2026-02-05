//! Database query implementations

use crate::error::Result;
use crate::types::*;
use rusqlite::{params, Connection, OptionalExtension};

// ===== Task Queries =====

/// Insert a new task
pub fn insert_task(conn: &Connection, state: &TaskState) -> Result<()> {
    let prompt_text: String = state
        .prompt
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    conn.execute(
        r#"
        INSERT INTO tasks (id, session_id, agent_id, status, prompt_text, working_dir, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            state.id,
            state.session_id,
            state.agent_id,
            format!("{:?}", state.status).to_lowercase(),
            prompt_text,
            state.working_directory,
            state.created_at.to_rfc3339(),
            state.updated_at.to_rfc3339(),
        ],
    )?;

    Ok(())
}

/// Update task status
pub fn update_task_status(
    conn: &Connection,
    task_id: &str,
    status: TaskStatus,
    stop_reason: Option<StopReason>,
    error_message: Option<&str>,
) -> Result<()> {
    let completed_at = if status.is_terminal() {
        Some(chrono::Utc::now().to_rfc3339())
    } else {
        None
    };

    conn.execute(
        r#"
        UPDATE tasks
        SET status = ?, stop_reason = ?, error_message = ?, updated_at = ?, completed_at = ?
        WHERE id = ?
        "#,
        params![
            format!("{:?}", status).to_lowercase(),
            stop_reason.map(|r| format!("{:?}", r).to_lowercase()),
            error_message,
            chrono::Utc::now().to_rfc3339(),
            completed_at,
            task_id,
        ],
    )?;

    Ok(())
}

/// Get task by ID
pub fn get_task(conn: &Connection, task_id: &str) -> Result<Option<TaskSummary>> {
    let result = conn
        .query_row(
            r#"
            SELECT id, session_id, agent_id, status, prompt_text, created_at, updated_at,
                   (SELECT COUNT(*) FROM artifacts WHERE task_id = tasks.id) as artifact_count,
                   (SELECT COUNT(*) FROM file_changes WHERE task_id = tasks.id) as file_change_count
            FROM tasks
            WHERE id = ?
            "#,
            params![task_id],
            |row| {
                Ok(TaskSummary {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    agent_id: row.get::<_, String>(2)?.clone(),
                    agent_name: row.get::<_, String>(2)?, // Same as agent_id for now
                    prompt_preview: row.get::<_, String>(4)?.chars().take(100).collect(),
                    status: parse_task_status(&row.get::<_, String>(3)?),
                    artifact_count: row.get(7)?,
                    file_change_count: row.get(8)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                        .unwrap()
                        .with_timezone(&chrono::Utc),
                })
            },
        )
        .optional()?;

    Ok(result)
}

/// List tasks with pagination
pub fn list_tasks(conn: &Connection, limit: usize, offset: usize) -> Result<Vec<TaskSummary>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, session_id, agent_id, status, prompt_text, created_at, updated_at,
               (SELECT COUNT(*) FROM artifacts WHERE task_id = tasks.id) as artifact_count,
               (SELECT COUNT(*) FROM file_changes WHERE task_id = tasks.id) as file_change_count
        FROM tasks
        ORDER BY updated_at DESC
        LIMIT ? OFFSET ?
        "#,
    )?;

    let tasks = stmt
        .query_map(params![limit as i64, offset as i64], |row| {
            Ok(TaskSummary {
                id: row.get(0)?,
                session_id: row.get(1)?,
                agent_id: row.get::<_, String>(2)?.clone(),
                agent_name: row.get::<_, String>(2)?,
                prompt_preview: row.get::<_, String>(4)?.chars().take(100).collect(),
                status: parse_task_status(&row.get::<_, String>(3)?),
                artifact_count: row.get(7)?,
                file_change_count: row.get(8)?,
                created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tasks)
}

/// Delete a task and all related data
pub fn delete_task(conn: &Connection, task_id: &str) -> Result<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?", params![task_id])?;
    Ok(())
}

// ===== Message Queries =====

/// Insert a message
pub fn insert_message(
    conn: &Connection,
    task_id: &str,
    message: &MessageBlock,
    seq_order: i32,
) -> Result<i64> {
    let (role, content_type, content) = match message {
        MessageBlock::User { content, .. } => {
            ("user", "content_blocks", serde_json::to_string(content)?)
        }
        MessageBlock::Agent { content, .. } => {
            ("agent", "content_blocks", serde_json::to_string(content)?)
        }
        MessageBlock::Thought { content, .. } => {
            ("thought", "content_blocks", serde_json::to_string(content)?)
        }
        MessageBlock::System { content, .. } => ("system", "text", content.clone()),
    };

    conn.execute(
        r#"
        INSERT INTO messages (task_id, role, content_type, content, seq_order, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            task_id,
            role,
            content_type,
            content,
            seq_order,
            message.timestamp().to_rfc3339(),
        ],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Get messages for a task
pub fn get_task_messages(conn: &Connection, task_id: &str) -> Result<Vec<MessageBlock>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT role, content_type, content, created_at
        FROM messages
        WHERE task_id = ?
        ORDER BY seq_order
        "#,
    )?;

    let messages = stmt
        .query_map(params![task_id], |row| {
            let role: String = row.get(0)?;
            let content_type: String = row.get(1)?;
            let content: String = row.get(2)?;
            let created_at: String = row.get(3)?;

            let timestamp = chrono::DateTime::parse_from_rfc3339(&created_at)
                .unwrap()
                .with_timezone(&chrono::Utc);

            let message = match (role.as_str(), content_type.as_str()) {
                ("user", "content_blocks") => MessageBlock::User {
                    content: serde_json::from_str(&content).unwrap_or_default(),
                    timestamp,
                },
                ("agent", "content_blocks") => MessageBlock::Agent {
                    content: serde_json::from_str(&content).unwrap_or_default(),
                    timestamp,
                },
                ("thought", "content_blocks") => MessageBlock::Thought {
                    content: serde_json::from_str(&content).unwrap_or_default(),
                    timestamp,
                },
                ("system", _) => MessageBlock::System { content, timestamp },
                _ => MessageBlock::System {
                    content: "Unknown message type".to_string(),
                    timestamp,
                },
            };

            Ok(message)
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(messages)
}

// ===== Tool Call Queries =====

/// Insert a tool call
pub fn insert_tool_call(conn: &Connection, task_id: &str, tc: &ToolCallState) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO tool_calls (id, task_id, title, kind, status, raw_input, content, started_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            tc.id,
            task_id,
            tc.title,
            tc.kind.map(|k| format!("{:?}", k).to_lowercase()),
            format!("{:?}", tc.status).to_lowercase(),
            tc.input.as_ref().map(|v| v.to_string()),
            serde_json::to_string(&tc.content)?,
            tc.started_at.to_rfc3339(),
        ],
    )?;

    Ok(())
}

/// Update a tool call
pub fn update_tool_call(
    conn: &Connection,
    tool_call_id: &str,
    status: ToolCallStatus,
    output: Option<&serde_json::Value>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<()> {
    conn.execute(
        r#"
        UPDATE tool_calls
        SET status = ?, raw_output = ?, completed_at = ?
        WHERE id = ?
        "#,
        params![
            format!("{:?}", status).to_lowercase(),
            output.map(|v| v.to_string()),
            completed_at.map(|t| t.to_rfc3339()),
            tool_call_id,
        ],
    )?;

    Ok(())
}

/// Get tool calls for a task
pub fn get_task_tool_calls(conn: &Connection, task_id: &str) -> Result<Vec<ToolCallState>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, title, kind, status, raw_input, raw_output, content, started_at, completed_at
        FROM tool_calls
        WHERE task_id = ?
        ORDER BY started_at
        "#,
    )?;

    let tool_calls = stmt
        .query_map(params![task_id], |row| {
            let id: String = row.get(0)?;
            let title: Option<String> = row.get(1)?;
            let kind: Option<String> = row.get(2)?;
            let status: String = row.get(3)?;
            let input: Option<String> = row.get(4)?;
            let output: Option<String> = row.get(5)?;
            let content: String = row.get(6)?;
            let started_at: String = row.get(7)?;
            let completed_at: Option<String> = row.get(8)?;

            Ok(ToolCallState {
                id,
                title,
                kind: kind.and_then(|k| parse_tool_call_kind(&k)),
                status: parse_tool_call_status(&status),
                input: input.and_then(|s| serde_json::from_str(&s).ok()),
                output: output.and_then(|s| serde_json::from_str(&s).ok()),
                content: serde_json::from_str(&content).unwrap_or_default(),
                started_at: chrono::DateTime::parse_from_rfc3339(&started_at)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                completed_at: completed_at.and_then(|s| {
                    chrono::DateTime::parse_from_rfc3339(&s)
                        .ok()
                        .map(|t| t.with_timezone(&chrono::Utc))
                }),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(tool_calls)
}

// ===== Artifact Queries =====

/// Insert an artifact
pub fn insert_artifact(conn: &Connection, artifact: &Artifact) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO artifacts (
            id, task_id, artifact_type, file_path, file_name, file_ext,
            mime_type, file_size, file_hash, old_path, source_layer,
            tool_call_id, summary, referenced_files, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            artifact.id,
            artifact.task_id,
            format!("{:?}", artifact.artifact_type).to_lowercase(),
            artifact.file.as_ref().map(|f| &f.path),
            artifact.file.as_ref().map(|f| &f.name),
            artifact.file.as_ref().map(|f| &f.extension),
            artifact.file.as_ref().map(|f| &f.mime_type),
            artifact.file.as_ref().map(|f| f.size as i64),
            artifact.file.as_ref().map(|f| &f.hash),
            artifact.old_path,
            artifact.source.layer as i32,
            artifact.source.tool_call_id,
            artifact.summary,
            serde_json::to_string(&artifact.referenced_files)?,
            artifact.created_at.to_rfc3339(),
        ],
    )?;

    Ok(())
}

/// Get artifacts for a task
pub fn get_task_artifacts(conn: &Connection, task_id: &str) -> Result<Vec<Artifact>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, artifact_type, file_path, file_name, file_ext, mime_type,
               file_size, file_hash, old_path, source_layer, tool_call_id,
               summary, referenced_files, created_at
        FROM artifacts
        WHERE task_id = ?
        ORDER BY created_at
        "#,
    )?;

    let artifacts = stmt
        .query_map(params![task_id], |row| {
            let id: String = row.get(0)?;
            let artifact_type: String = row.get(1)?;
            let file_path: Option<String> = row.get(2)?;
            let file_name: Option<String> = row.get(3)?;
            let file_ext: Option<String> = row.get(4)?;
            let mime_type: Option<String> = row.get(5)?;
            let file_size: Option<i64> = row.get(6)?;
            let file_hash: Option<String> = row.get(7)?;
            let old_path: Option<String> = row.get(8)?;
            let source_layer: i32 = row.get(9)?;
            let tool_call_id: Option<String> = row.get(10)?;
            let summary: Option<String> = row.get(11)?;
            let referenced_files: String = row.get(12)?;
            let created_at: String = row.get(13)?;

            let file = file_path.map(|path| ArtifactFile {
                path,
                name: file_name.unwrap_or_default(),
                extension: file_ext.unwrap_or_default(),
                mime_type: mime_type.unwrap_or_default(),
                size: file_size.unwrap_or(0) as u64,
                hash: file_hash.unwrap_or_default(),
            });

            let preview = file
                .as_ref()
                .map(ArtifactPreview::from_file)
                .unwrap_or_else(ArtifactPreview::unsupported);

            Ok(Artifact {
                id,
                task_id: task_id.to_string(),
                artifact_type: parse_artifact_type(&artifact_type),
                file,
                old_path,
                source: ArtifactSource {
                    layer: source_layer as u8,
                    tool_call_id,
                    method: None,
                    command: None,
                },
                preview,
                summary,
                referenced_files: serde_json::from_str(&referenced_files).unwrap_or_default(),
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(artifacts)
}

// ===== Agent Queries =====

/// Insert or update an agent configuration
pub fn upsert_agent(conn: &Connection, config: &AgentConfig) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO agents (id, name, description, command, args, env, icon, builtin, enabled, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            description = excluded.description,
            command = excluded.command,
            args = excluded.args,
            env = excluded.env,
            icon = excluded.icon,
            enabled = excluded.enabled,
            updated_at = excluded.updated_at
        "#,
        params![
            config.id,
            config.name,
            config.description,
            config.command,
            serde_json::to_string(&config.args)?,
            serde_json::to_string(&config.env)?,
            config.icon,
            config.builtin as i32,
            config.enabled as i32,
            config.created_at.to_rfc3339(),
            config.updated_at.to_rfc3339(),
        ],
    )?;

    Ok(())
}

/// Get all agents
pub fn get_all_agents(conn: &Connection) -> Result<Vec<AgentConfig>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, description, command, args, env, icon, builtin, enabled, created_at, updated_at
        FROM agents
        ORDER BY builtin DESC, name
        "#,
    )?;

    let agents = stmt
        .query_map([], |row| {
            let id: String = row.get(0)?;
            let name: String = row.get(1)?;
            let description: Option<String> = row.get(2)?;
            let command: String = row.get(3)?;
            let args: String = row.get(4)?;
            let env: String = row.get(5)?;
            let icon: Option<String> = row.get(6)?;
            let builtin: i32 = row.get(7)?;
            let enabled: i32 = row.get(8)?;
            let created_at: String = row.get(9)?;
            let updated_at: String = row.get(10)?;

            Ok(AgentConfig {
                id,
                name,
                description,
                command,
                args: serde_json::from_str(&args).unwrap_or_default(),
                env: serde_json::from_str(&env).unwrap_or_default(),
                icon,
                builtin: builtin != 0,
                enabled: enabled != 0,
                created_at: chrono::DateTime::parse_from_rfc3339(&created_at)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at)
                    .unwrap()
                    .with_timezone(&chrono::Utc),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(agents)
}

/// Delete an agent
pub fn delete_agent(conn: &Connection, agent_id: &str) -> Result<()> {
    conn.execute("DELETE FROM agents WHERE id = ? AND builtin = 0", params![agent_id])?;
    Ok(())
}

// ===== Settings Queries =====

/// Get a setting value
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    let result = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?",
            params![key],
            |row| row.get(0),
        )
        .optional()?;

    Ok(result)
}

/// Set a setting value
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?, ?, CURRENT_TIMESTAMP)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP
        "#,
        params![key, value],
    )?;

    Ok(())
}

/// Get all settings
pub fn get_all_settings(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT key, value FROM settings")?;

    let settings = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((key, value))
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(settings)
}

// ===== Helper Functions =====

fn parse_task_status(s: &str) -> TaskStatus {
    match s {
        "pending" => TaskStatus::Pending,
        "planning" => TaskStatus::Planning,
        "executing" => TaskStatus::Executing,
        "progressing" => TaskStatus::Progressing,
        "completed" => TaskStatus::Completed,
        "cancelled" => TaskStatus::Cancelled,
        "error" => TaskStatus::Error,
        _ => TaskStatus::Pending,
    }
}

fn parse_tool_call_status(s: &str) -> ToolCallStatus {
    match s {
        "pending" => ToolCallStatus::Pending,
        "in_progress" => ToolCallStatus::InProgress,
        "completed" => ToolCallStatus::Completed,
        "failed" => ToolCallStatus::Failed,
        "cancelled" => ToolCallStatus::Cancelled,
        _ => ToolCallStatus::Pending,
    }
}

fn parse_tool_call_kind(s: &str) -> Option<ToolCallKind> {
    match s {
        "read" => Some(ToolCallKind::Read),
        "write" => Some(ToolCallKind::Write),
        "delete" => Some(ToolCallKind::Delete),
        "move" => Some(ToolCallKind::Move),
        "execute" => Some(ToolCallKind::Execute),
        "fetch" => Some(ToolCallKind::Fetch),
        "other" => Some(ToolCallKind::Other),
        _ => None,
    }
}

fn parse_artifact_type(s: &str) -> ArtifactType {
    match s {
        "file_created" => ArtifactType::FileCreated,
        "file_modified" => ArtifactType::FileModified,
        "file_deleted" => ArtifactType::FileDeleted,
        "file_moved" => ArtifactType::FileMoved,
        "directory_created" => ArtifactType::DirectoryCreated,
        "analysis_result" => ArtifactType::AnalysisResult,
        "terminal_output" => ArtifactType::TerminalOutput,
        _ => ArtifactType::FileCreated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::storage::run_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn test_task_crud() {
        let conn = setup_db();

        let state = TaskState::new(
            "task-1".to_string(),
            "session-1".to_string(),
            "agent-1".to_string(),
            vec![ContentBlock::Text {
                text: "Test prompt".to_string(),
            }],
            "/home/user".to_string(),
        );

        // Insert
        insert_task(&conn, &state).unwrap();

        // Get
        let retrieved = get_task(&conn, "task-1").unwrap();
        assert!(retrieved.is_some());
        let task = retrieved.unwrap();
        assert_eq!(task.id, "task-1");
        assert_eq!(task.status, TaskStatus::Pending);

        // Update status
        update_task_status(&conn, "task-1", TaskStatus::Completed, Some(StopReason::EndTurn), None).unwrap();

        let updated = get_task(&conn, "task-1").unwrap().unwrap();
        assert_eq!(updated.status, TaskStatus::Completed);

        // Delete
        delete_task(&conn, "task-1").unwrap();
        assert!(get_task(&conn, "task-1").unwrap().is_none());
    }

    #[test]
    fn test_messages() {
        let conn = setup_db();

        let state = TaskState::new(
            "task-1".to_string(),
            "session-1".to_string(),
            "agent-1".to_string(),
            vec![],
            "/home".to_string(),
        );
        insert_task(&conn, &state).unwrap();

        let msg = MessageBlock::agent(vec![ContentBlock::Text {
            text: "Hello".to_string(),
        }]);
        insert_message(&conn, "task-1", &msg, 0).unwrap();

        let messages = get_task_messages(&conn, "task-1").unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_settings() {
        let conn = setup_db();

        // Set
        set_setting(&conn, "theme", "dark").unwrap();

        // Get
        let value = get_setting(&conn, "theme").unwrap();
        assert_eq!(value, Some("dark".to_string()));

        // Get non-existent
        let none = get_setting(&conn, "nonexistent").unwrap();
        assert!(none.is_none());
    }
}
