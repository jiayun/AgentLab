use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub provider: ProviderConfig,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
        }
    }
}

fn default_port() -> u16 {
    8080
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            api_url: default_api_url(),
            model: default_model(),
            api_key: None,
        }
    }
}

fn default_api_url() -> String {
    "http://localhost:11434/v1".to_string()
}

fn default_model() -> String {
    "llama3.1".to_string()
}

impl AppConfig {
    pub fn port(&self) -> u16 {
        self.server.port
    }

    pub fn load() -> Result<Self> {
        let config_path = Path::new("agentlab.toml");
        if config_path.exists() {
            let content =
                std::fs::read_to_string(config_path).context("Failed to read agentlab.toml")?;
            let mut config: AppConfig =
                toml::from_str(&content).context("Failed to parse agentlab.toml")?;

            // Environment variable overrides
            if let Ok(url) = std::env::var("AGENTLAB_API_URL") {
                config.provider.api_url = url;
            }
            if let Ok(model) = std::env::var("AGENTLAB_MODEL") {
                config.provider.model = model;
            }
            if let Ok(key) = std::env::var("AGENTLAB_API_KEY") {
                config.provider.api_key = Some(key);
            }
            if let Ok(port) = std::env::var("AGENTLAB_PORT") {
                config.server.port = port.parse().context("Invalid port")?;
            }

            Ok(config)
        } else {
            tracing::warn!("agentlab.toml not found, using defaults");
            Ok(AppConfig {
                server: ServerConfig::default(),
                provider: ProviderConfig::default(),
            })
        }
    }
}
