//! Agent lifecycle manager

use crate::acp::{AcpClient, AgentConnection};
use crate::error::{AgentError, Error, Result};
use crate::types::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

/// Manages agent lifecycle and state
pub struct AgentManager {
    /// Agent configurations
    configs: HashMap<String, AgentConfig>,
    /// Running agent states
    states: HashMap<String, AgentState>,
    /// Active ACP clients
    clients: HashMap<String, Arc<AcpClient>>,
}

impl AgentManager {
    pub fn new() -> Self {
        let mut manager = Self {
            configs: HashMap::new(),
            states: HashMap::new(),
            clients: HashMap::new(),
        };

        // Register built-in agents
        for config in AgentConfig::builtin_agents() {
            manager.configs.insert(config.id.clone(), config.clone());
            manager.states.insert(config.id.clone(), AgentState::new(config));
        }

        manager
    }

    /// List all registered agents
    pub fn list_agents(&self) -> Vec<AgentState> {
        self.states.values().cloned().collect()
    }

    /// Get agent by ID
    pub fn get_agent(&self, id: &str) -> Option<&AgentState> {
        self.states.get(id)
    }

    /// Get agent configuration
    pub fn get_config(&self, id: &str) -> Option<&AgentConfig> {
        self.configs.get(id)
    }

    /// Add a new agent configuration
    pub fn add_agent(&mut self, config: AgentConfig) -> Result<()> {
        if self.configs.contains_key(&config.id) {
            return Err(Error::Agent(AgentError::AlreadyExists(config.id)));
        }

        info!("Adding agent: {} ({})", config.name, config.id);

        self.states
            .insert(config.id.clone(), AgentState::new(config.clone()));
        self.configs.insert(config.id.clone(), config);

        Ok(())
    }

    /// Update agent configuration
    pub fn update_agent(&mut self, id: &str, config: AgentConfig) -> Result<()> {
        if !self.configs.contains_key(id) {
            return Err(Error::Agent(AgentError::NotFound(id.to_string())));
        }

        // Don't allow updating running agents
        if let Some(state) = self.states.get(id) {
            if state.status == AgentStatus::Running {
                return Err(Error::Agent(AgentError::AlreadyRunning(id.to_string())));
            }
        }

        info!("Updating agent: {}", id);

        self.configs.insert(id.to_string(), config.clone());
        if let Some(state) = self.states.get_mut(id) {
            state.config = config;
        }

        Ok(())
    }

    /// Remove an agent
    pub fn remove_agent(&mut self, id: &str) -> Result<()> {
        let config = self.configs.get(id).ok_or_else(|| {
            Error::Agent(AgentError::NotFound(id.to_string()))
        })?;

        // Don't allow removing built-in agents
        if config.builtin {
            return Err(Error::Agent(AgentError::InvalidConfig(
                "Cannot remove built-in agent".to_string(),
            )));
        }

        // Don't allow removing running agents
        if let Some(state) = self.states.get(id) {
            if state.status == AgentStatus::Running {
                return Err(Error::Agent(AgentError::AlreadyRunning(id.to_string())));
            }
        }

        info!("Removing agent: {}", id);

        self.configs.remove(id);
        self.states.remove(id);
        self.clients.remove(id);

        Ok(())
    }

    /// Start an agent with AcpChannels
    pub async fn start_agent(
        &mut self,
        id: &str,
        cwd: Option<&str>,
        channels: &crate::acp::AcpChannels,
    ) -> Result<Arc<AcpClient>> {
        self.start_agent_with_channels(
            id,
            cwd,
            channels.session_update_tx.clone(),
            channels.agent_request_tx.clone(),
        )
        .await
    }

    /// Start an agent with explicit channel senders
    pub async fn start_agent_with_channels(
        &mut self,
        id: &str,
        cwd: Option<&str>,
        update_tx: mpsc::Sender<SessionUpdateNotification>,
        request_tx: mpsc::Sender<(JsonRpcRequest, oneshot::Sender<JsonRpcResponse>)>,
    ) -> Result<Arc<AcpClient>> {
        let config = self.configs.get(id).ok_or_else(|| {
            Error::Agent(AgentError::NotFound(id.to_string()))
        })?.clone();

        // Check if already running
        if let Some(client) = self.clients.get(id) {
            if client.is_running().await {
                return Err(Error::Agent(AgentError::AlreadyRunning(id.to_string())));
            }
        }

        info!("Starting agent: {} ({})", config.name, config.command);

        // Update state to starting
        if let Some(state) = self.states.get_mut(id) {
            state.status = AgentStatus::Starting;
            state.error_message = None;
        }

        // Create ACP client
        let client = match AcpClient::connect(config.clone(), cwd, update_tx, request_tx).await {
            Ok(client) => Arc::new(client),
            Err(e) => {
                if let Some(state) = self.states.get_mut(id) {
                    state.status = AgentStatus::Error;
                    state.error_message = Some(e.to_string());
                }
                return Err(e);
            }
        };

        // Update state to initializing
        if let Some(state) = self.states.get_mut(id) {
            state.status = AgentStatus::Initializing;
        }

        // Initialize ACP connection
        let capabilities = ClientCapabilities {
            file_system: Some(FileSystemCapability {
                read: true,
                write: true,
                list: true,
            }),
            terminal: Some(TerminalCapability { execute: true }),
            mcp: None,
            load_session: Some(true),
        };

        if let Err(e) = client.initialize(capabilities).await {
            if let Some(state) = self.states.get_mut(id) {
                state.status = AgentStatus::Error;
                state.error_message = Some(e.to_string());
            }
            let _ = client.terminate().await;
            return Err(e);
        }

        // Update state to running
        if let Some(state) = self.states.get_mut(id) {
            state.status = AgentStatus::Running;
            state.started_at = Some(chrono::Utc::now());
            state.capabilities = client.capabilities().await;
            state.agent_info = client.agent_info().await;
        }

        self.clients.insert(id.to_string(), Arc::clone(&client));

        info!("Agent started successfully: {}", id);
        Ok(client)
    }

    /// Stop an agent
    pub async fn stop_agent(&mut self, id: &str) -> Result<()> {
        let client = self.clients.remove(id).ok_or_else(|| {
            Error::Agent(AgentError::NotRunning(id.to_string()))
        })?;

        info!("Stopping agent: {}", id);

        // Update state to stopping
        if let Some(state) = self.states.get_mut(id) {
            state.status = AgentStatus::Stopping;
        }

        // Terminate the process
        if let Err(e) = client.terminate().await {
            warn!("Error terminating agent {}: {}", id, e);
        }

        // Update state to stopped
        if let Some(state) = self.states.get_mut(id) {
            state.status = AgentStatus::Stopped;
            state.started_at = None;
            state.last_activity = Some(chrono::Utc::now());
        }

        info!("Agent stopped: {}", id);
        Ok(())
    }

    /// Get agent status
    pub fn get_status(&self, id: &str) -> Option<AgentStatus> {
        self.states.get(id).map(|s| s.status)
    }

    /// Get active client for an agent (returns Option)
    pub fn get_client_opt(&self, id: &str) -> Option<Arc<AcpClient>> {
        self.clients.get(id).cloned()
    }

    /// Get active client for an agent (returns Result)
    pub fn get_client(&self, id: &str) -> Result<Arc<AcpClient>> {
        self.clients
            .get(id)
            .cloned()
            .ok_or_else(|| Error::Agent(AgentError::NotRunning(id.to_string())))
    }

    /// Check if an agent is running
    pub async fn is_running(&self, id: &str) -> bool {
        if let Some(client) = self.clients.get(id) {
            client.is_running().await
        } else {
            false
        }
    }

    /// Record activity for an agent
    pub fn record_activity(&mut self, id: &str) {
        if let Some(state) = self.states.get_mut(id) {
            state.last_activity = Some(chrono::Utc::now());
        }
    }

    /// Increment session count for an agent
    pub fn increment_session_count(&mut self, id: &str) {
        if let Some(state) = self.states.get_mut(id) {
            state.session_count += 1;
        }
    }

    /// Get all running agents
    pub fn running_agents(&self) -> Vec<&str> {
        self.states
            .iter()
            .filter(|(_, s)| s.status == AgentStatus::Running)
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Stop all running agents
    pub async fn stop_all(&mut self) -> Result<()> {
        let running: Vec<String> = self.running_agents().iter().map(|s| s.to_string()).collect();

        for id in running {
            if let Err(e) = self.stop_agent(&id).await {
                error!("Error stopping agent {}: {}", id, e);
            }
        }

        Ok(())
    }
}

impl Default for AgentManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_manager_builtin_agents() {
        let manager = AgentManager::new();
        let agents = manager.list_agents();

        // Should have 4 built-in agents
        assert_eq!(agents.len(), 4);

        // Check all are present
        assert!(manager.get_agent("claude-code").is_some());
        assert!(manager.get_agent("gemini-cli").is_some());
        assert!(manager.get_agent("codex-cli").is_some());
        assert!(manager.get_agent("goose").is_some());
    }

    #[test]
    fn test_add_custom_agent() {
        let mut manager = AgentManager::new();

        let config = AgentConfig::new("custom-agent", "Custom Agent", "/usr/bin/custom");
        manager.add_agent(config).unwrap();

        assert!(manager.get_agent("custom-agent").is_some());
        assert_eq!(manager.list_agents().len(), 5);
    }

    #[test]
    fn test_add_duplicate_agent() {
        let mut manager = AgentManager::new();

        let config = AgentConfig::new("claude-code", "Duplicate", "/usr/bin/dup");
        let result = manager.add_agent(config);

        assert!(result.is_err());
        if let Err(Error::Agent(AgentError::AlreadyExists(id))) = result {
            assert_eq!(id, "claude-code");
        }
    }

    #[test]
    fn test_remove_custom_agent() {
        let mut manager = AgentManager::new();

        let config = AgentConfig::new("custom-agent", "Custom Agent", "/usr/bin/custom");
        manager.add_agent(config).unwrap();

        manager.remove_agent("custom-agent").unwrap();
        assert!(manager.get_agent("custom-agent").is_none());
    }

    #[test]
    fn test_cannot_remove_builtin_agent() {
        let mut manager = AgentManager::new();

        let result = manager.remove_agent("claude-code");
        assert!(result.is_err());
    }

    #[test]
    fn test_agent_status_initial() {
        let manager = AgentManager::new();

        let status = manager.get_status("claude-code");
        assert_eq!(status, Some(AgentStatus::Stopped));
    }
}
