//! Terminal command execution with policy checks

use crate::error::{Error, Result, SandboxError};
use crate::types::{TerminalExecuteResult, TerminalPolicy};
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;
use tracing::debug;

/// Terminal handler enforcing the configured policy
pub struct TerminalHandler;

impl TerminalHandler {
    pub async fn execute(
        policy: &TerminalPolicy,
        command: &str,
        args: &[String],
        cwd: Option<&str>,
        env: Option<&HashMap<String, String>>,
    ) -> Result<TerminalExecuteResult> {
        if !policy.enabled {
            return Err(Error::Sandbox(SandboxError::AccessDenied(
                "Terminal execution is disabled by policy".to_string(),
            )));
        }

        let cmd_name = Path::new(command)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(command);

        if !policy.allowed_commands.is_empty()
            && !policy.allowed_commands.iter().any(|c| c == cmd_name)
        {
            return Err(Error::Sandbox(SandboxError::AccessDenied(format!(
                "Command '{}' is not allowed by policy",
                cmd_name
            ))));
        }

        let full_cmd = if args.is_empty() {
            cmd_name.to_string()
        } else {
            format!("{} {}", cmd_name, args.join(" "))
        };

        if policy
            .blocked_patterns
            .iter()
            .any(|pat| full_cmd.contains(pat))
        {
            return Err(Error::Sandbox(SandboxError::AccessDenied(format!(
                "Command blocked by policy (matched blocked pattern): {}",
                full_cmd
            ))));
        }

        debug!("Executing command: {} (cwd: {:?})", full_cmd, cwd);

        let mut cmd = Command::new(command);
        cmd.args(args);

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        if let Some(envs) = env {
            cmd.envs(envs);
        }

        let output = cmd.output().await.map_err(|e| {
            Error::Sandbox(SandboxError::AccessDenied(format!(
                "Failed to execute command '{}': {}",
                cmd_name, e
            )))
        })?;

        Ok(TerminalExecuteResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}
