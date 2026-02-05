//! ACP runtime wiring for session updates and agent tool requests

use super::ProtocolHandler;
use crate::sandbox::{FileOperation, FileSystemHandler, PermissionManager, TerminalHandler};
use crate::storage::Storage;
use crate::types::*;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, error};

/// Shared channels used by all ACP clients.
///
/// Each spawned `AcpClient` should be given clones of these senders so the app can:
/// - Accumulate `session/update` into task state
/// - Handle agent requests (fs/*, terminal/*)
#[derive(Debug)]
pub struct AcpChannels {
    pub session_update_tx: mpsc::Sender<SessionUpdateNotification>,
    pub agent_request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
}

impl AcpChannels {
    pub fn new(
        buffer: usize,
    ) -> (
        Self,
        mpsc::Receiver<SessionUpdateNotification>,
        mpsc::Receiver<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
    ) {
        let (session_update_tx, session_update_rx) = mpsc::channel(buffer);
        let (agent_request_tx, agent_request_rx) = mpsc::channel(buffer);
        (
            Self {
                session_update_tx,
                agent_request_tx,
            },
            session_update_rx,
            agent_request_rx,
        )
    }
}

/// Spawn runtime tasks for headless (non-GUI) operation.
///
/// This version processes session updates and agent requests without
/// emitting GUI events. The session manager is updated directly.
pub fn spawn_runtime_tasks_headless(
    session_manager: Arc<Mutex<super::SessionManager>>,
    permission_manager: Arc<RwLock<PermissionManager>>,
    storage: Arc<Storage>,
    mut session_update_rx: mpsc::Receiver<SessionUpdateNotification>,
    agent_request_rx: mpsc::Receiver<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
) {
    // Session update handler: update in-memory state.
    let session_manager_updates = Arc::clone(&session_manager);
    tokio::spawn(async move {
        while let Some(notification) = session_update_rx.recv().await {
            let mut sm = session_manager_updates.lock().await;
            sm.process_update(notification);
        }
    });

    // Agent request handler
    spawn_agent_request_handler(permission_manager, storage, agent_request_rx);
}

/// Spawn runtime tasks with UI forwarding.
///
/// This version forwards session updates to both the session manager and the UI.
pub fn spawn_runtime_tasks_with_ui(
    session_manager: Arc<Mutex<super::SessionManager>>,
    permission_manager: Arc<RwLock<PermissionManager>>,
    storage: Arc<Storage>,
    mut session_update_rx: mpsc::Receiver<SessionUpdateNotification>,
    agent_request_rx: mpsc::Receiver<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
    ui_update_tx: mpsc::Sender<SessionUpdateNotification>,
) {
    // Session update handler: update in-memory state AND forward to UI.
    let session_manager_updates = Arc::clone(&session_manager);
    tokio::spawn(async move {
        while let Some(notification) = session_update_rx.recv().await {
            // Update session manager
            {
                let mut sm = session_manager_updates.lock().await;
                sm.process_update(notification.clone());
            }

            // Forward to UI
            if let Err(e) = ui_update_tx.send(notification).await {
                debug!("Failed to forward session update to UI: {}", e);
            }
        }
    });

    // Agent request handler
    spawn_agent_request_handler(permission_manager, storage, agent_request_rx);
}

fn spawn_agent_request_handler(
    permission_manager: Arc<RwLock<PermissionManager>>,
    storage: Arc<Storage>,
    agent_request_rx: mpsc::Receiver<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
) {
    let mut agent_request_rx = agent_request_rx;

    // Agent request handler: respond to fs/* and terminal/* calls.
    tokio::spawn(async move {
        let protocol = ProtocolHandler::new();

        while let Some((request, tx)) = agent_request_rx.recv().await {
            let request_id = request.id.clone().unwrap_or(serde_json::Value::Null);
            let method = request.method.clone();

            let response = match handle_agent_request(
                &protocol,
                &permission_manager,
                &storage,
                request,
            )
            .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    error!("Agent request handler error ({}): {}", method, e);
                    protocol.create_error_response(request_id, -32603, &e.to_string())
                }
            };

            debug!("Handled agent request: {}", method);
            let _ = tx.send(response);
        }
    });
}

async fn handle_agent_request(
    protocol: &ProtocolHandler,
    permission_manager: &Arc<RwLock<PermissionManager>>,
    storage: &Arc<Storage>,
    request: JsonRpcRequest,
) -> crate::Result<JsonRpcResponse> {
    let request_id = request.id.clone().unwrap_or(serde_json::Value::Null);
    let params = request.params.clone().unwrap_or(serde_json::Value::Null);

    match request.method.as_str() {
        "fs/read_text_file" => {
            let p: FsReadTextFileParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;
            let content = FileSystemHandler::read_text_file(&pm, &p.path).await?;
            Ok(protocol.create_fs_read_response(request_id, &content))
        }
        "fs/write_file" | "fs/write_text_file" => {
            let p: FsWriteFileParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;

            if pm.requires_confirmation(&p.path, FileOperation::Write) {
                return Ok(protocol.create_error_response(
                    request_id,
                    -32603,
                    "Write requires confirmation under current security policy",
                ));
            }

            let _ = FileSystemHandler::write_file(&pm, &p.path, &p.content).await?;
            Ok(protocol.create_fs_write_response(request_id))
        }
        "fs/list_directory" => {
            let p: FsListDirectoryParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;
            let entries = FileSystemHandler::list_directory(&pm, &p.path).await?;
            Ok(protocol.create_fs_list_response(request_id, entries))
        }
        "fs/delete_file" => {
            let p: FsDeleteFileParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;

            if pm.requires_confirmation(&p.path, FileOperation::Delete) {
                return Ok(protocol.create_error_response(
                    request_id,
                    -32603,
                    "Delete requires confirmation under current security policy",
                ));
            }

            FileSystemHandler::delete_file(&pm, &p.path).await?;
            Ok(protocol.create_fs_write_response(request_id))
        }
        "fs/move_file" => {
            let p: FsMoveFileParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;

            if pm.requires_confirmation(&p.old_path, FileOperation::Move)
                || pm.requires_confirmation(&p.new_path, FileOperation::Move)
            {
                return Ok(protocol.create_error_response(
                    request_id,
                    -32603,
                    "Move requires confirmation under current security policy",
                ));
            }

            FileSystemHandler::move_file(&pm, &p.old_path, &p.new_path).await?;
            Ok(protocol.create_fs_write_response(request_id))
        }
        "fs/create_directory" => {
            let p: FsCreateDirectoryParams = serde_json::from_value(params)?;
            let pm = permission_manager.read().await;

            if pm.requires_confirmation(&p.path, FileOperation::Write) {
                return Ok(protocol.create_error_response(
                    request_id,
                    -32603,
                    "Create directory requires confirmation under current security policy",
                ));
            }

            FileSystemHandler::create_directory(&pm, &p.path).await?;
            Ok(protocol.create_fs_write_response(request_id))
        }
        "terminal/execute" | "terminal/create" => {
            let p: TerminalExecuteParams = serde_json::from_value(params)?;

            // Validate cwd is inside granted paths when provided.
            if let Some(cwd) = p.cwd.as_deref() {
                let pm = permission_manager.read().await;
                pm.validate_access(cwd)?;
            }

            // Load terminal policy from settings.
            let policy = {
                let conn = storage.connection()?;
                let raw = crate::storage::get_setting(&conn, "terminal_policy")?;
                raw.and_then(|v| serde_json::from_str::<TerminalPolicy>(&v).ok())
                    .unwrap_or_default()
            };

            let args = p.args.unwrap_or_default();
            let result = TerminalHandler::execute(
                &policy,
                &p.command,
                &args,
                p.cwd.as_deref(),
                p.env.as_ref(),
            )
            .await?;

            Ok(protocol.create_terminal_response(request_id, result))
        }
        other => Ok(protocol.create_error_response(
            request_id,
            -32601,
            &format!("Unsupported method: {}", other),
        )),
    }
}
