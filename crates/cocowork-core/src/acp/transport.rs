//! JSON-RPC transport over stdin/stdout

use crate::error::{AcpError, Error, Result};
use crate::types::{JsonRpcRequest, JsonRpcResponse};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, error, trace, warn};

/// Transport layer for ACP communication
/// Uses channels to avoid lock contention between send and receive
pub struct Transport {
    /// Channel to send data to stdin writer task
    stdin_tx: mpsc::Sender<String>,
    /// Channel to receive data from stdout reader task
    stdout_rx: Mutex<mpsc::Receiver<String>>,
    /// Background tasks
    _stdin_task: tokio::task::JoinHandle<()>,
    _stdout_task: tokio::task::JoinHandle<()>,
    _stderr_task: tokio::task::JoinHandle<()>,
}

impl Transport {
    /// Spawn a new agent process and create transport
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &std::collections::HashMap<String, String>,
        cwd: Option<&str>,
    ) -> Result<(Self, Child)> {
        debug!(
            "Spawning agent: {} {:?} (cwd: {:?})",
            command, args, cwd
        );

        let mut cmd = Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let mut child = cmd.spawn().map_err(|e| {
            Error::Acp(AcpError::ConnectionFailed(format!(
                "Failed to spawn agent process: {}",
                e
            )))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            Error::Acp(AcpError::ConnectionFailed(
                "Failed to capture stdin".to_string(),
            ))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            Error::Acp(AcpError::ConnectionFailed(
                "Failed to capture stdout".to_string(),
            ))
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            Error::Acp(AcpError::ConnectionFailed(
                "Failed to capture stderr".to_string(),
            ))
        })?;

        // Create channels for stdin/stdout
        let (stdin_tx, stdin_rx) = mpsc::channel::<String>(100);
        let (stdout_tx, stdout_rx) = mpsc::channel::<String>(100);

        // Spawn task to write to stdin
        let stdin_task = tokio::spawn(Self::write_stdin_task(stdin, stdin_rx));

        // Spawn task to read from stdout
        let stdout_task = tokio::spawn(Self::read_stdout_task(stdout, stdout_tx));

        // Spawn task to drain stderr so the agent can't deadlock on a full pipe.
        let stderr_task = tokio::spawn(Self::read_stderr_task(stderr));

        Ok((
            Self {
                stdin_tx,
                stdout_rx: Mutex::new(stdout_rx),
                _stdin_task: stdin_task,
                _stdout_task: stdout_task,
                _stderr_task: stderr_task,
            },
            child,
        ))
    }

    /// Background task to write to stdin
    async fn write_stdin_task(mut stdin: ChildStdin, mut rx: mpsc::Receiver<String>) {
        while let Some(data) = rx.recv().await {
            trace!("Sending to stdin: {}", data);
            if let Err(e) = stdin.write_all(data.as_bytes()).await {
                error!("Failed to write to stdin: {}", e);
                break;
            }
            if let Err(e) = stdin.write_all(b"\n").await {
                error!("Failed to write newline to stdin: {}", e);
                break;
            }
            if let Err(e) = stdin.flush().await {
                error!("Failed to flush stdin: {}", e);
                break;
            }
        }
        debug!("Stdin writer task ended");
    }

    /// Background task to read stdout lines
    async fn read_stdout_task(stdout: ChildStdout, tx: mpsc::Sender<String>) {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("Agent stdout closed");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        trace!("Agent stdout: {}", trimmed);
                        if tx.send(trimmed.to_string()).await.is_err() {
                            warn!("Failed to send stdout line, channel closed");
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading agent stdout: {}", e);
                    break;
                }
            }
        }
    }

    /// Background task to drain stderr.
    async fn read_stderr_task(stderr: ChildStderr) {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("Agent stderr closed");
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        // Avoid treating stderr as fatal; agents often log here.
                        warn!("Agent stderr: {}", trimmed);
                    }
                }
                Err(e) => {
                    error!("Error reading agent stderr: {}", e);
                    break;
                }
            }
        }
    }

    /// Send a JSON-RPC request (non-blocking)
    pub async fn send_request(&self, request: &JsonRpcRequest) -> Result<()> {
        let json = serde_json::to_string(request)?;
        trace!("Sending request: {}", json);
        self.stdin_tx
            .send(json)
            .await
            .map_err(|e| Error::Acp(AcpError::ConnectionFailed(format!("Failed to send: {}", e))))?;
        Ok(())
    }

    /// Receive next raw stdout line from the agent.
    pub async fn recv_line(&self) -> Option<String> {
        let mut rx = self.stdout_rx.lock().await;
        rx.recv().await
    }

    /// Receive with timeout.
    pub async fn recv_line_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Option<String>> {
        let mut rx = self.stdout_rx.lock().await;
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(v) => Ok(v),
            Err(_) => Err(Error::Acp(AcpError::Timeout)),
        }
    }

    /// Send a JSON-RPC response (non-blocking)
    pub async fn send_response(&self, response: &JsonRpcResponse) -> Result<()> {
        let json = serde_json::to_string(response)?;
        trace!("Sending response: {}", json);
        self.stdin_tx
            .send(json)
            .await
            .map_err(|e| Error::Acp(AcpError::ConnectionFailed(format!("Failed to send: {}", e))))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_transport_spawn_invalid_command() {
        let result = Transport::spawn(
            "nonexistent_command_12345",
            &[],
            &std::collections::HashMap::new(),
            None,
        )
        .await;

        assert!(result.is_err());
        if let Err(Error::Acp(AcpError::ConnectionFailed(msg))) = result {
            assert!(msg.contains("Failed to spawn"));
        } else {
            panic!("Expected ConnectionFailed error");
        }
    }

    #[tokio::test]
    async fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest::new(1, "test_method", Some(serde_json::json!({"key": "value"})));

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"test_method\""));
        assert!(json.contains("\"id\":1"));
    }
}
