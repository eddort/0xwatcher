use alloy::primitives::Address;
use eyre::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DurationSeconds};
use std::fs;
use std::num::NonZeroUsize;
use std::time::Duration;

/// Address configuration with alias
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressConfig {
    pub alias: String,
    pub address: Address,
}

/// Token configuration (same as address)
pub type TokenConfig = AddressConfig;

/// Telegram configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

fn default_active_transport_count() -> NonZeroUsize {
    NonZeroUsize::new(3).unwrap()
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "rpc_nodes")]
    pub rpc_urls: Vec<Url>,
    pub addresses: Vec<AddressConfig>,
    #[serde(default)]
    pub tokens: Vec<TokenConfig>,
    #[serde(rename = "interval_secs")]
    #[serde_as(as = "DurationSeconds<u64>")]
    pub interval: Duration,
    #[serde(default = "default_active_transport_count")]
    pub active_transport_count: NonZeroUsize,
    pub telegram: Option<TelegramConfig>,
}

impl Config {
    pub fn from_file(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = serde_yaml::from_str(&content)?;

        // Validation
        if config.rpc_urls.is_empty() {
            eyre::bail!("rpc_nodes list cannot be empty");
        }
        if config.addresses.is_empty() {
            eyre::bail!("addresses list cannot be empty");
        }
        if let Some(ref telegram) = config.telegram {
            if telegram.bot_token.is_empty() {
                eyre::bail!("telegram bot_token cannot be empty");
            }
        }

        Ok(config)
    }
}
