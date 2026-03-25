use serde::Deserialize;

/// Dashboard configuration loaded from TOML.
#[derive(Debug, Clone, Deserialize)]
pub struct DashboardConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub agents: Vec<AgentConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    pub jwt_secret: String,
}

fn default_port() -> u16 {
    3001
}

/// Configuration for a connected bot agent.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub name: String,
    pub url: String,
    pub secret: String,
}

impl DashboardConfig {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Create a minimal config for testing.
    #[cfg(test)]
    pub fn test_config(jwt_secret: &str) -> Self {
        Self {
            server: ServerConfig {
                port: 0,
                jwt_secret: jwt_secret.to_string(),
            },
            agents: vec![],
        }
    }
}

#[cfg(test)]
#[path = "tests/config_tests.rs"]
mod tests;
