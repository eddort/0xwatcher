use eyre::Result;
use serde::{Deserialize, Serialize};
use std::fs;

/// Address with an alias
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressConfig {
    pub alias: String,
    pub address: String,
}

/// Token to monitor
pub type TokenConfig = AddressConfig;

/// Telegram configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    /// List of allowed Telegram usernames (without @)
    /// If empty, bot is public and anyone can use it
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

/// Application configuration from YAML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub rpc_nodes: Vec<String>,
    pub addresses: Vec<AddressConfig>,
    pub tokens: Vec<TokenConfig>,
    pub interval_secs: u64,
    #[serde(default = "default_active_transport_count")]
    pub active_transport_count: usize,
    pub telegram: Option<TelegramConfig>,
}

fn default_active_transport_count() -> usize {
    3
}

impl Config {
    /// Load configuration from a YAML file
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }
}
