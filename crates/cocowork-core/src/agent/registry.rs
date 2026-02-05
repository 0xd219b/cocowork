//! ACP Agent Registry client
//!
//! Future implementation for browsing and installing agents from
//! the ACP registry at agentclientprotocol.com/registry

use crate::error::Result;
use crate::types::AgentConfig;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Registry agent listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryAgent {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub install_method: InstallMethod,
    pub acp_version: String,
    pub capabilities: Vec<String>,
    pub tags: Vec<String>,
    pub downloads: u64,
    pub rating: f32,
}

/// Installation method for an agent
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallMethod {
    Npm { package: String },
    Pip { package: String },
    Cargo { crate_name: String },
    Binary { url: String, checksum: String },
    Manual { instructions: String },
}

/// Agent registry client
///
/// Note: This is a stub implementation. The `base_url` field is reserved
/// for future API integration with the ACP registry.
pub struct AgentRegistry {
    #[allow(dead_code)]
    base_url: String,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            base_url: "https://agentclientprotocol.com/api/registry".to_string(),
        }
    }

    pub fn with_url(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }

    /// Search for agents in the registry
    pub async fn search(&self, query: &str) -> Result<Vec<RegistryAgent>> {
        // TODO: Implement actual API call
        debug!("Searching registry for: {}", query);

        // Return mock data for now
        Ok(vec![])
    }

    /// List all agents in the registry
    pub async fn list(&self) -> Result<Vec<RegistryAgent>> {
        // TODO: Implement actual API call
        debug!("Listing all agents from registry");

        Ok(vec![])
    }

    /// Get agent details
    pub async fn get(&self, id: &str) -> Result<Option<RegistryAgent>> {
        // TODO: Implement actual API call
        debug!("Getting agent details: {}", id);

        Ok(None)
    }

    /// Install an agent from the registry
    pub async fn install(&self, agent: &RegistryAgent) -> Result<AgentConfig> {
        info!("Installing agent: {} v{}", agent.name, agent.version);

        match &agent.install_method {
            InstallMethod::Npm { package } => {
                self.install_npm(package).await?;
                Ok(self.create_config_from_registry(agent, package))
            }
            InstallMethod::Pip { package } => {
                self.install_pip(package).await?;
                Ok(self.create_config_from_registry(agent, package))
            }
            InstallMethod::Cargo { crate_name } => {
                self.install_cargo(crate_name).await?;
                Ok(self.create_config_from_registry(agent, crate_name))
            }
            InstallMethod::Binary { url, checksum } => {
                let path = self.install_binary(url, checksum).await?;
                Ok(self.create_config_from_registry(agent, &path))
            }
            InstallMethod::Manual { instructions } => {
                // Can't auto-install manual agents
                Err(crate::error::Error::Agent(
                    crate::error::AgentError::InvalidConfig(format!(
                        "Manual installation required: {}",
                        instructions
                    )),
                ))
            }
        }
    }

    async fn install_npm(&self, _package: &str) -> Result<()> {
        // TODO: Run npm install
        Ok(())
    }

    async fn install_pip(&self, _package: &str) -> Result<()> {
        // TODO: Run pip install
        Ok(())
    }

    async fn install_cargo(&self, _crate_name: &str) -> Result<()> {
        // TODO: Run cargo install
        Ok(())
    }

    async fn install_binary(&self, _url: &str, _checksum: &str) -> Result<String> {
        // TODO: Download and verify binary
        Ok(String::new())
    }

    fn create_config_from_registry(&self, agent: &RegistryAgent, command: &str) -> AgentConfig {
        AgentConfig {
            id: agent.id.clone(),
            name: agent.name.clone(),
            description: Some(agent.description.clone()),
            command: command.to_string(),
            args: vec!["--acp".to_string()],
            env: std::collections::HashMap::new(),
            icon: None,
            builtin: false,
            enabled: true,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = AgentRegistry::new();
        assert!(registry.base_url.contains("agentclientprotocol.com"));
    }

    #[test]
    fn test_custom_registry_url() {
        let registry = AgentRegistry::with_url("https://custom.registry.com");
        assert_eq!(registry.base_url, "https://custom.registry.com");
    }
}
