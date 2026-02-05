//! Agent Server Adapter Pattern (inspired by Zed)
//!
//! This module provides a unified interface for different ACP-compatible agents:
//! - Claude Code
//! - Gemini CLI
//! - Codex CLI
//! - Custom agents
//!
//! Each adapter implements the `AgentServerAdapter` trait which handles:
//! - Agent-specific configuration
//! - Connection establishment
//! - Default mode/model settings
//!
//! It also implements the new `AgentServer` trait from the traits module for
//! the refactored architecture.

use crate::acp::traits::{
    AgentClient, AgentConnection, AgentServer, AgentServerCommand, ModelId, SessionModeId,
};
use crate::acp::AcpConnection;
use crate::error::Result;
use crate::types::{AgentConfig, ClientCapabilities, FileSystemCapability, TerminalCapability};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::process::Command;
use tracing::{debug, info, warn};

/// Agent server adapter trait - similar to Zed's AgentServer trait
#[async_trait]
pub trait AgentServerAdapter: Send + Sync {
    /// Get the agent's display name
    fn name(&self) -> &str;

    /// Get the agent's unique identifier
    fn id(&self) -> &str;

    /// Get the agent's icon name
    fn icon(&self) -> &str {
        "terminal"
    }

    /// Get default mode for the agent (if supported)
    fn default_mode(&self) -> Option<String> {
        None
    }

    /// Get default model for the agent (if supported)
    fn default_model(&self) -> Option<String> {
        None
    }

    /// Get the command to spawn the agent
    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command>;

    /// Get additional environment variables for the agent
    fn get_env(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Check if the agent is available (installed)
    async fn is_available(&self) -> bool;

    /// Get agent configuration
    fn config(&self) -> AgentConfig;
}

// ============================================================================
// Claude Code Adapter
// ============================================================================

/// The NPM package that provides the Claude Code ACP bridge (from Zed)
const CLAUDE_CODE_ACP_PACKAGE: &str = "@zed-industries/claude-code-acp";
const CLAUDE_CODE_ACP_MIN_VERSION: &str = "0.5.0";

/// Claude Code adapter - uses the @anthropic-ai/claude-code NPM package as ACP bridge
///
/// Unlike other agents that use CLI flags directly, Claude Code requires a Node.js
/// bridge package that implements the ACP protocol. This adapter manages:
/// - Finding/installing the NPM package
/// - Locating the Node.js binary
/// - Launching the ACP bridge with proper stdio handling
pub struct ClaudeCodeAdapter {
    config: AgentConfig,
    /// Custom node path (from env or explicit setting)
    node_path: Option<String>,
    /// Custom path to the ACP bridge script
    acp_script_path: Option<PathBuf>,
    /// Directory where npm packages are installed
    npm_prefix: Option<PathBuf>,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self {
            config: AgentConfig {
                id: "claude-code".to_string(),
                name: "Claude Code".to_string(),
                description: Some("Anthropic's Claude Code CLI agent".to_string()),
                command: "node".to_string(),
                args: vec![],  // Will be filled with script path
                env: HashMap::new(),
                icon: Some("anthropic".to_string()),
                builtin: true,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            node_path: std::env::var("COCOWORK_NODE_PATH").ok(),
            acp_script_path: std::env::var("CLAUDE_CODE_ACP_PATH").ok().map(PathBuf::from),
            npm_prefix: Self::default_npm_prefix(),
        }
    }

    /// Get default npm prefix directory (where packages are installed)
    fn default_npm_prefix() -> Option<PathBuf> {
        dirs::data_dir().map(|d| d.join("cocowork").join("npm"))
    }

    /// Find the Node.js binary path
    async fn find_node_path(&self) -> Option<String> {
        // Check explicit setting first
        if let Some(path) = &self.node_path {
            return Some(path.clone());
        }

        // Try to find node in PATH
        if let Ok(output) = tokio::process::Command::new("which")
            .arg("node")
            .output()
            .await
        {
            if output.status.success() {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    return Some(path.trim().to_string());
                }
            }
        }

        None
    }

    /// Find the Claude Code ACP bridge script
    async fn find_acp_script(&self) -> Option<PathBuf> {
        // Check explicit setting first
        if let Some(path) = &self.acp_script_path {
            if path.exists() {
                return Some(path.clone());
            }
        }

        // Check our npm prefix directory
        if let Some(prefix) = &self.npm_prefix {
            let script_path = prefix
                .join("node_modules")
                .join("@zed-industries")
                .join("claude-code-acp")
                .join("dist")
                .join("index.js");
            if script_path.exists() {
                debug!("Found Claude Code ACP script at: {:?}", script_path);
                return Some(script_path);
            }
        }

        // Check global npm installation
        let global_paths = [
            // macOS/Linux global npm
            "/usr/local/lib/node_modules/@zed-industries/claude-code-acp/dist/index.js",
            "/usr/lib/node_modules/@zed-industries/claude-code-acp/dist/index.js",
            // User's npm prefix (npm config get prefix)
            &format!(
                "{}/.npm/lib/node_modules/@zed-industries/claude-code-acp/dist/index.js",
                std::env::var("HOME").unwrap_or_default()
            ),
            // nvm installations
            &format!(
                "{}/.nvm/versions/node/*/lib/node_modules/@zed-industries/claude-code-acp/dist/index.js",
                std::env::var("HOME").unwrap_or_default()
            ),
        ];

        for path_pattern in &global_paths {
            if path_pattern.contains('*') {
                // Handle glob patterns
                if let Ok(paths) = glob::glob(path_pattern) {
                    for entry in paths.flatten() {
                        if entry.exists() {
                            debug!("Found Claude Code ACP script at: {:?}", entry);
                            return Some(entry);
                        }
                    }
                }
            } else {
                let path = PathBuf::from(path_pattern);
                if path.exists() {
                    debug!("Found Claude Code ACP script at: {:?}", path);
                    return Some(path);
                }
            }
        }

        // Try to find via npm root
        if let Ok(output) = tokio::process::Command::new("npm")
            .args(["root", "-g"])
            .output()
            .await
        {
            if output.status.success() {
                if let Ok(root) = String::from_utf8(output.stdout) {
                    let script_path = PathBuf::from(root.trim())
                        .join("@zed-industries")
                        .join("claude-code-acp")
                        .join("dist")
                        .join("index.js");
                    if script_path.exists() {
                        debug!("Found Claude Code ACP script at: {:?}", script_path);
                        return Some(script_path);
                    }
                }
            }
        }

        None
    }

    /// Install the Claude Code ACP package if not present
    pub async fn ensure_acp_package_installed(&self) -> Result<PathBuf> {
        // First check if already installed
        if let Some(path) = self.find_acp_script().await {
            info!("Claude Code ACP package already installed at: {:?}", path);
            return Ok(path);
        }

        info!("Installing Claude Code ACP package...");

        // Create npm prefix directory if needed
        let prefix = self.npm_prefix.as_ref().ok_or_else(|| {
            crate::error::Error::Agent(crate::error::AgentError::SetupFailed(
                "Cannot determine npm prefix directory".to_string(),
            ))
        })?;

        std::fs::create_dir_all(prefix).map_err(|e| {
            crate::error::Error::Agent(crate::error::AgentError::SetupFailed(format!(
                "Failed to create npm prefix directory: {}",
                e
            )))
        })?;

        // Install the package
        let output = tokio::process::Command::new("npm")
            .args([
                "install",
                "--prefix",
                &prefix.to_string_lossy(),
                &format!("{}@>={}", CLAUDE_CODE_ACP_PACKAGE, CLAUDE_CODE_ACP_MIN_VERSION),
            ])
            .output()
            .await
            .map_err(|e| {
                crate::error::Error::Agent(crate::error::AgentError::SetupFailed(format!(
                    "Failed to run npm install: {}",
                    e
                )))
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("npm install failed: {}", stderr);
            return Err(crate::error::Error::Agent(
                crate::error::AgentError::SetupFailed(format!(
                    "Failed to install Claude Code ACP package: {}",
                    stderr
                )),
            ));
        }

        // Find the installed script
        self.find_acp_script().await.ok_or_else(|| {
            crate::error::Error::Agent(crate::error::AgentError::SetupFailed(
                "Package installed but script not found".to_string(),
            ))
        })
    }

    /// Get the command to launch Claude Code ACP
    async fn get_acp_command(&self) -> Result<(String, Vec<String>)> {
        let node_path = self.find_node_path().await.ok_or_else(|| {
            crate::error::Error::Agent(crate::error::AgentError::NotFound(
                "Node.js not found. Please install Node.js".to_string(),
            ))
        })?;

        let script_path = self.ensure_acp_package_installed().await?;

        Ok((
            node_path,
            vec![script_path.to_string_lossy().to_string()],
        ))
    }

    pub fn with_node_path(mut self, node_path: String) -> Self {
        self.node_path = Some(node_path);
        self
    }

    pub fn with_acp_script_path(mut self, path: PathBuf) -> Self {
        self.acp_script_path = Some(path);
        self
    }

    pub fn with_npm_prefix(mut self, prefix: PathBuf) -> Self {
        self.npm_prefix = Some(prefix);
        self
    }
}

impl Default for ClaudeCodeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentServerAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn id(&self) -> &str {
        "claude-code"
    }

    fn icon(&self) -> &str {
        "anthropic"
    }

    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command> {
        // This is a sync method, so we can't use async here
        // Return a placeholder command - actual command is built in connect()
        let mut cmd = Command::new("node");

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        Ok(cmd)
    }

    async fn is_available(&self) -> bool {
        // Check if Node.js is available
        let node_available = self.find_node_path().await.is_some();
        if !node_available {
            debug!("Claude Code not available: Node.js not found");
            return false;
        }

        // Check if the ACP script is installed or can be installed
        // For now, just check if node is available - we'll install the package on connect
        true
    }

    fn config(&self) -> AgentConfig {
        self.config.clone()
    }
}

#[async_trait]
impl AgentServer for ClaudeCodeAdapter {
    fn name(&self) -> &str {
        "Claude Code"
    }

    fn id(&self) -> &str {
        "claude-code"
    }

    fn icon(&self) -> &str {
        "anthropic"
    }

    fn default_mode(&self) -> Option<SessionModeId> {
        None // Claude Code doesn't have default modes
    }

    fn default_model(&self) -> Option<ModelId> {
        None // Uses default from claude CLI
    }

    fn get_command(&self) -> Option<AgentServerCommand> {
        // This returns a placeholder - actual command is built in connect()
        Some(AgentServerCommand::new("node"))
    }

    fn get_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        // Pass through ANTHROPIC_API_KEY if set
        if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
            env.insert("ANTHROPIC_API_KEY".to_string(), key);
        }
        env
    }

    async fn is_available(&self) -> bool {
        AgentServerAdapter::is_available(self).await
    }

    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        info!("Connecting to Claude Code...");

        // Get the actual command (this may install the package if needed)
        let (node_path, args) = self.get_acp_command().await?;
        info!("Using node: {}, args: {:?}", node_path, args);

        let cwd = root_dir.map(|p| p.to_string_lossy().to_string());

        let connection = AcpConnection::new(
            AgentServer::name(self),
            &node_path,
            &args,
            &AgentServer::get_env(self),
            cwd.as_deref(),
            delegate,
        )
        .await?;

        // Initialize the connection
        let client_caps = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };
        connection.initialize(client_caps).await?;

        Ok(Arc::new(connection))
    }
}

// ============================================================================
// Gemini CLI Adapter
// ============================================================================

/// Gemini CLI adapter - uses gemini CLI with experimental ACP support
pub struct GeminiAdapter {
    config: AgentConfig,
    api_key: Option<String>,
}

impl GeminiAdapter {
    pub fn new() -> Self {
        Self {
            config: AgentConfig {
                id: "gemini-cli".to_string(),
                name: "Gemini CLI".to_string(),
                description: Some("Google's Gemini CLI agent".to_string()),
                command: "gemini".to_string(),
                args: vec!["--experimental-acp".to_string()],
                env: HashMap::new(),
                icon: Some("google".to_string()),
                builtin: true,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            api_key: std::env::var("GEMINI_API_KEY").ok(),
        }
    }

    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.api_key = Some(api_key);
        self
    }
}

impl Default for GeminiAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentServerAdapter for GeminiAdapter {
    fn name(&self) -> &str {
        "Gemini CLI"
    }

    fn id(&self) -> &str {
        "gemini-cli"
    }

    fn icon(&self) -> &str {
        "google"
    }

    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command> {
        let mut cmd = Command::new("gemini");
        cmd.arg("--experimental-acp");

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        Ok(cmd)
    }

    fn get_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        if let Some(key) = &self.api_key {
            env.insert("GEMINI_API_KEY".to_string(), key.clone());
        }
        // Zed sets SURFACE=zed for telemetry
        env.insert("SURFACE".to_string(), "cocowork".to_string());
        env
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg("gemini")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn config(&self) -> AgentConfig {
        self.config.clone()
    }
}

#[async_trait]
impl AgentServer for GeminiAdapter {
    fn name(&self) -> &str {
        "Gemini CLI"
    }

    fn id(&self) -> &str {
        "gemini-cli"
    }

    fn icon(&self) -> &str {
        "google"
    }

    fn default_mode(&self) -> Option<SessionModeId> {
        None
    }

    fn default_model(&self) -> Option<ModelId> {
        None
    }

    fn get_command(&self) -> Option<AgentServerCommand> {
        Some(AgentServerCommand::new("gemini").with_args(vec!["--experimental-acp".to_string()]))
    }

    fn get_env(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        if let Some(key) = &self.api_key {
            env.insert("GEMINI_API_KEY".to_string(), key.clone());
        }
        env.insert("SURFACE".to_string(), "cocowork".to_string());
        env
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg("gemini")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        let cmd = AgentServer::get_command(self).expect("Command should be available");
        let cwd = root_dir.map(|p| p.to_string_lossy().to_string());

        let connection = AcpConnection::new(
            AgentServer::name(self),
            &cmd.command,
            &cmd.args,
            &AgentServer::get_env(self),
            cwd.as_deref(),
            delegate,
        )
        .await?;

        let client_caps = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };
        connection.initialize(client_caps).await?;

        Ok(Arc::new(connection))
    }
}

// ============================================================================
// Codex CLI Adapter
// ============================================================================

/// GitHub repo for the codex-acp binary (same as Zed uses)
const CODEX_ACP_REPO: &str = "zed-industries/codex-acp";
const CODEX_API_KEY_VAR: &str = "CODEX_API_KEY";
const OPEN_AI_API_KEY_VAR: &str = "OPEN_AI_API_KEY";

/// Codex adapter - uses the codex-acp binary from zed-industries/codex-acp
///
/// Unlike the simple `codex --acp` approach, Zed uses a dedicated `codex-acp`
/// binary that implements the ACP protocol. This adapter manages:
/// - Downloading the codex-acp binary from GitHub releases
/// - Finding the correct platform-specific binary
/// - Launching it with proper stdio handling
pub struct CodexAdapter {
    config: AgentConfig,
    /// Directory where codex-acp binary is installed
    install_dir: PathBuf,
    /// Custom binary path (override auto-download)
    custom_binary_path: Option<PathBuf>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        let install_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("cocowork")
            .join("codex");

        Self {
            config: AgentConfig {
                id: "codex-cli".to_string(),
                name: "Codex".to_string(),
                description: Some("OpenAI's Codex agent (via codex-acp)".to_string()),
                command: "codex-acp".to_string(),
                args: vec![],
                env: HashMap::new(),
                icon: Some("openai".to_string()),
                builtin: true,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
            install_dir,
            custom_binary_path: std::env::var("CODEX_ACP_PATH").ok().map(PathBuf::from),
        }
    }

    pub fn with_binary_path(mut self, path: PathBuf) -> Self {
        self.custom_binary_path = Some(path);
        self
    }

    /// Get the platform-specific asset name for GitHub releases
    fn asset_name(version: &str) -> Option<String> {
        let arch = if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            return None;
        };

        let platform = if cfg!(target_os = "macos") {
            "apple-darwin"
        } else if cfg!(target_os = "windows") {
            "pc-windows-msvc"
        } else if cfg!(target_os = "linux") {
            "unknown-linux-gnu"
        } else {
            return None;
        };

        let ext = if cfg!(target_os = "windows") {
            "zip"
        } else {
            "tar.gz"
        };

        Some(format!("codex-acp-{version}-{arch}-{platform}.{ext}"))
    }

    /// Binary name for the current platform
    fn bin_name() -> &'static str {
        if cfg!(target_os = "windows") {
            "codex-acp.exe"
        } else {
            "codex-acp"
        }
    }

    /// Find the latest locally installed version
    fn find_latest_local_version(&self) -> Option<PathBuf> {
        let dir = &self.install_dir;
        if !dir.exists() {
            return None;
        }

        let mut versions: Vec<(String, PathBuf)> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let bin_path = path.join(Self::bin_name());
                    if bin_path.exists() {
                        let version_str = entry.file_name().to_string_lossy().to_string();
                        versions.push((version_str, bin_path));
                    }
                }
            }
        }

        // Sort by version string (lexicographic is fine for semver with v prefix)
        versions.sort_by(|(a, _), (b, _)| a.cmp(b));
        versions.last().map(|(_, path)| path.clone())
    }

    /// Download and install the codex-acp binary from GitHub releases
    async fn download_latest(&self) -> std::result::Result<PathBuf, String> {
        info!("Fetching latest codex-acp release from {}...", CODEX_ACP_REPO);

        // Get the latest release info from GitHub API
        let output = tokio::process::Command::new("curl")
            .args([
                "-fsSL",
                "-H", "Accept: application/vnd.github+json",
                &format!("https://api.github.com/repos/{}/releases/latest", CODEX_ACP_REPO),
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to fetch release info: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "Failed to fetch release info: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        let release: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse release JSON: {}", e))?;

        let tag_name = release["tag_name"]
            .as_str()
            .ok_or("Missing tag_name in release")?;

        let version_number = tag_name.trim_start_matches('v');
        let version_dir = self.install_dir.join(tag_name);

        // Check if already installed
        let bin_path = version_dir.join(Self::bin_name());
        if bin_path.exists() {
            info!("codex-acp {} already installed", tag_name);
            return Ok(bin_path);
        }

        let asset_name = Self::asset_name(version_number)
            .ok_or("codex-acp is not supported for this architecture")?;

        // Find the download URL for the asset
        let assets = release["assets"]
            .as_array()
            .ok_or("Missing assets in release")?;

        let download_url = assets
            .iter()
            .find(|a| a["name"].as_str() == Some(&asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
            .ok_or_else(|| format!("Asset {} not found in release", asset_name))?;

        info!("Downloading codex-acp {} from {}...", tag_name, download_url);

        // Create version directory
        std::fs::create_dir_all(&version_dir)
            .map_err(|e| format!("Failed to create directory: {}", e))?;

        // Download and extract in one piped command: curl | tar
        let version_dir_str = version_dir.to_string_lossy().to_string();
        let extract_output = tokio::process::Command::new("sh")
            .args([
                "-c",
                &format!(
                    "curl -fsSL '{}' | tar xzf - -C '{}'",
                    download_url, version_dir_str
                ),
            ])
            .output()
            .await
            .map_err(|e| format!("Failed to download and extract: {}", e))?;

        if !extract_output.status.success() {
            return Err(format!(
                "Failed to download/extract codex-acp: {}",
                String::from_utf8_lossy(&extract_output.stderr)
            ));
        }

        if !bin_path.exists() {
            return Err(format!(
                "Binary not found at {} after extraction",
                bin_path.display()
            ));
        }

        // Make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin_path, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| format!("Failed to set permissions: {}", e))?;
        }

        // Clean up older versions
        if let Ok(entries) = std::fs::read_dir(&self.install_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path != version_dir {
                    let _ = std::fs::remove_dir_all(&path);
                }
            }
        }

        info!("codex-acp {} installed successfully", tag_name);
        Ok(bin_path)
    }

    /// Get the codex-acp binary path, installing if necessary
    async fn ensure_binary(&self) -> std::result::Result<PathBuf, String> {
        // Check custom path first
        if let Some(path) = &self.custom_binary_path {
            if path.exists() {
                return Ok(path.clone());
            }
            warn!("Custom codex-acp path {:?} not found, falling back", path);
        }

        // Check if already installed locally
        if let Some(path) = self.find_latest_local_version() {
            debug!("Found locally installed codex-acp at {:?}", path);
            return Ok(path);
        }

        // Download from GitHub
        self.download_latest().await
    }

    /// Get environment variables for Codex
    fn codex_env() -> HashMap<String, String> {
        let mut env = HashMap::new();
        if let Ok(key) = std::env::var(CODEX_API_KEY_VAR) {
            env.insert(CODEX_API_KEY_VAR.to_string(), key);
        }
        if let Ok(key) = std::env::var(OPEN_AI_API_KEY_VAR) {
            env.insert(OPEN_AI_API_KEY_VAR.to_string(), key);
        }
        env
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentServerAdapter for CodexAdapter {
    fn name(&self) -> &str {
        "Codex"
    }

    fn id(&self) -> &str {
        "codex-cli"
    }

    fn icon(&self) -> &str {
        "openai"
    }

    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command> {
        // Placeholder - actual binary path is resolved in connect()
        let mut cmd = Command::new("codex-acp");

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        Ok(cmd)
    }

    fn get_env(&self) -> HashMap<String, String> {
        Self::codex_env()
    }

    async fn is_available(&self) -> bool {
        // Check if binary is installed or can be downloaded
        if let Some(path) = &self.custom_binary_path {
            if path.exists() {
                return true;
            }
        }
        if self.find_latest_local_version().is_some() {
            return true;
        }
        // We can always try to download, so report as available
        // (download will fail with a clear error if it doesn't work)
        true
    }

    fn config(&self) -> AgentConfig {
        self.config.clone()
    }
}

#[async_trait]
impl AgentServer for CodexAdapter {
    fn name(&self) -> &str {
        "Codex"
    }

    fn id(&self) -> &str {
        "codex-cli"
    }

    fn icon(&self) -> &str {
        "openai"
    }

    fn default_mode(&self) -> Option<SessionModeId> {
        None
    }

    fn default_model(&self) -> Option<ModelId> {
        None
    }

    fn get_command(&self) -> Option<AgentServerCommand> {
        // Placeholder - actual binary path is resolved in connect()
        Some(AgentServerCommand::new("codex-acp"))
    }

    fn get_env(&self) -> HashMap<String, String> {
        Self::codex_env()
    }

    async fn is_available(&self) -> bool {
        AgentServerAdapter::is_available(self).await
    }

    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        info!("Connecting to Codex...");

        // Ensure the codex-acp binary is available (download if needed)
        let bin_path = self.ensure_binary().await.map_err(|e| {
            crate::error::Error::Agent(crate::error::AgentError::SetupFailed(e))
        })?;
        info!("Using codex-acp binary: {:?}", bin_path);

        let bin_path_str = bin_path.to_string_lossy().to_string();
        let cwd = root_dir.map(|p| p.to_string_lossy().to_string());

        let connection = AcpConnection::new(
            AgentServer::name(self),
            &bin_path_str,
            &[],
            &Self::codex_env(),
            cwd.as_deref(),
            delegate,
        )
        .await?;

        let client_caps = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };
        connection.initialize(client_caps).await?;

        Ok(Arc::new(connection))
    }
}

// ============================================================================
// Goose Adapter
// ============================================================================

/// Goose adapter - Block's Goose CLI agent
pub struct GooseAdapter {
    config: AgentConfig,
}

impl GooseAdapter {
    pub fn new() -> Self {
        Self {
            config: AgentConfig {
                id: "goose".to_string(),
                name: "Goose".to_string(),
                description: Some("Block's Goose CLI agent".to_string()),
                command: "goose".to_string(),
                args: vec!["--acp".to_string()],
                env: HashMap::new(),
                icon: Some("goose".to_string()),
                builtin: true,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        }
    }
}

impl Default for GooseAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentServerAdapter for GooseAdapter {
    fn name(&self) -> &str {
        "Goose"
    }

    fn id(&self) -> &str {
        "goose"
    }

    fn icon(&self) -> &str {
        "goose"
    }

    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command> {
        let mut cmd = Command::new("goose");
        cmd.arg("--acp");

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        Ok(cmd)
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg("goose")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn config(&self) -> AgentConfig {
        self.config.clone()
    }
}

#[async_trait]
impl AgentServer for GooseAdapter {
    fn name(&self) -> &str {
        "Goose"
    }

    fn id(&self) -> &str {
        "goose"
    }

    fn icon(&self) -> &str {
        "goose"
    }

    fn default_mode(&self) -> Option<SessionModeId> {
        None
    }

    fn default_model(&self) -> Option<ModelId> {
        None
    }

    fn get_command(&self) -> Option<AgentServerCommand> {
        Some(AgentServerCommand::new("goose").with_args(vec!["--acp".to_string()]))
    }

    fn get_env(&self) -> HashMap<String, String> {
        HashMap::new()
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg("goose")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        let cmd = AgentServer::get_command(self).expect("Command should be available");
        let cwd = root_dir.map(|p| p.to_string_lossy().to_string());

        let connection = AcpConnection::new(
            AgentServer::name(self),
            &cmd.command,
            &cmd.args,
            &AgentServer::get_env(self),
            cwd.as_deref(),
            delegate,
        )
        .await?;

        let client_caps = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };
        connection.initialize(client_caps).await?;

        Ok(Arc::new(connection))
    }
}

// ============================================================================
// Custom Agent Adapter
// ============================================================================

/// Custom agent adapter - for user-defined ACP-compatible agents
pub struct CustomAgentAdapter {
    config: AgentConfig,
}

impl CustomAgentAdapter {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            config: AgentConfig {
                id: id.into(),
                name: name.into(),
                description: None,
                command: command.into(),
                args,
                env: HashMap::new(),
                icon: None,
                builtin: false,
                enabled: true,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            },
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.config.description = Some(description.into());
        self
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.config.env = env;
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.config.icon = Some(icon.into());
        self
    }

    pub fn from_config(config: AgentConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl AgentServerAdapter for CustomAgentAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn id(&self) -> &str {
        &self.config.id
    }

    fn icon(&self) -> &str {
        self.config.icon.as_deref().unwrap_or("terminal")
    }

    fn get_command(&self, working_dir: Option<&Path>) -> Result<Command> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args);

        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        // Apply custom environment variables
        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        cmd.stdin(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        Ok(cmd)
    }

    fn get_env(&self) -> HashMap<String, String> {
        self.config.env.clone()
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg(&self.config.command)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn config(&self) -> AgentConfig {
        self.config.clone()
    }
}

#[async_trait]
impl AgentServer for CustomAgentAdapter {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn id(&self) -> &str {
        &self.config.id
    }

    fn icon(&self) -> &str {
        self.config.icon.as_deref().unwrap_or("terminal")
    }

    fn default_mode(&self) -> Option<SessionModeId> {
        None
    }

    fn default_model(&self) -> Option<ModelId> {
        None
    }

    fn get_command(&self) -> Option<AgentServerCommand> {
        Some(
            AgentServerCommand::new(&self.config.command)
                .with_args(self.config.args.clone()),
        )
    }

    fn get_env(&self) -> HashMap<String, String> {
        self.config.env.clone()
    }

    async fn is_available(&self) -> bool {
        tokio::process::Command::new("which")
            .arg(&self.config.command)
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    async fn connect(
        &self,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        let cmd = AgentServer::get_command(self).expect("Command should be available");
        let cwd = root_dir.map(|p| p.to_string_lossy().to_string());

        let connection = AcpConnection::new(
            AgentServer::name(self),
            &cmd.command,
            &cmd.args,
            &AgentServer::get_env(self),
            cwd.as_deref(),
            delegate,
        )
        .await?;

        let client_caps = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };
        connection.initialize(client_caps).await?;

        Ok(Arc::new(connection))
    }
}

// ============================================================================
// Agent Adapter Registry
// ============================================================================

/// Wrapper trait that combines AgentServerAdapter (legacy) and AgentServer (new)
/// This allows the registry to work with both traits during the migration
pub trait AgentAdapter: AgentServerAdapter + AgentServer {}

// Implement AgentAdapter for all types that implement both traits
impl<T: AgentServerAdapter + AgentServer> AgentAdapter for T {}

/// Registry of all available agent adapters
pub struct AgentAdapterRegistry {
    adapters: Vec<Box<dyn AgentAdapter>>,
}

impl AgentAdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Create registry with all builtin adapters
    pub fn with_builtins() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(ClaudeCodeAdapter::new()));
        registry.register(Box::new(GeminiAdapter::new()));
        registry.register(Box::new(CodexAdapter::new()));
        registry.register(Box::new(GooseAdapter::new()));
        registry
    }

    /// Register a new adapter
    pub fn register(&mut self, adapter: Box<dyn AgentAdapter>) {
        self.adapters.push(adapter);
    }

    /// Register a custom agent
    pub fn register_custom(&mut self, config: AgentConfig) {
        self.adapters.push(Box::new(CustomAgentAdapter::from_config(config)));
    }

    /// Get all adapters (legacy)
    pub fn all(&self) -> Vec<&dyn AgentServerAdapter> {
        self.adapters.iter().map(|a| a.as_ref() as &dyn AgentServerAdapter).collect()
    }

    /// Get adapter by ID (legacy)
    pub fn get(&self, id: &str) -> Option<&dyn AgentServerAdapter> {
        self.adapters
            .iter()
            .find(|a| AgentServerAdapter::id(a.as_ref()) == id)
            .map(|a| a.as_ref() as &dyn AgentServerAdapter)
    }

    /// Get adapter by ID as AgentServer (new architecture)
    pub fn get_server(&self, id: &str) -> Option<&dyn AgentServer> {
        self.adapters
            .iter()
            .find(|a| AgentServer::id(a.as_ref()) == id)
            .map(|a| a.as_ref() as &dyn AgentServer)
    }

    /// Get all adapter configs
    pub fn configs(&self) -> Vec<AgentConfig> {
        self.adapters.iter().map(|a| a.config()).collect()
    }

    /// Check which adapters are available
    pub async fn available_adapters(&self) -> Vec<&dyn AgentServerAdapter> {
        let mut available = Vec::new();
        for adapter in &self.adapters {
            if AgentServerAdapter::is_available(adapter.as_ref()).await {
                available.push(adapter.as_ref() as &dyn AgentServerAdapter);
            }
        }
        available
    }

    /// Connect to an agent by ID (new architecture)
    pub async fn connect(
        &self,
        agent_id: &str,
        root_dir: Option<&Path>,
        delegate: Arc<dyn AgentClient>,
    ) -> Result<Arc<dyn AgentConnection>> {
        let server = self.get_server(agent_id).ok_or_else(|| {
            crate::error::Error::Agent(crate::error::AgentError::NotFound(agent_id.to_string()))
        })?;
        server.connect(root_dir, delegate).await
    }
}

impl Default for AgentAdapterRegistry {
    fn default() -> Self {
        Self::with_builtins()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_registry() {
        let registry = AgentAdapterRegistry::with_builtins();
        assert_eq!(registry.all().len(), 4);

        assert!(registry.get("claude-code").is_some());
        assert!(registry.get("gemini-cli").is_some());
        assert!(registry.get("codex-cli").is_some());
        assert!(registry.get("goose").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_custom_adapter() {
        let mut registry = AgentAdapterRegistry::new();

        let custom = CustomAgentAdapter::new(
            "my-agent",
            "My Custom Agent",
            "my-agent-cli",
            vec!["--acp".to_string()],
        )
        .with_description("A custom agent");

        registry.register(Box::new(custom));

        let adapter = registry.get("my-agent").unwrap();
        assert_eq!(adapter.name(), "My Custom Agent");
        assert_eq!(adapter.id(), "my-agent");
    }

    #[test]
    fn test_claude_code_adapter() {
        let adapter = ClaudeCodeAdapter::new();
        assert_eq!(AgentServerAdapter::id(&adapter), "claude-code");
        assert_eq!(AgentServerAdapter::name(&adapter), "Claude Code");

        let config = AgentServerAdapter::config(&adapter);
        assert!(config.builtin);
        // Claude Code now uses node + npm package instead of --acp flag
        assert_eq!(config.command, "node");
    }
}
