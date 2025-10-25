use alloy::{
    primitives::{Address, utils::format_units, U256},
    providers::Provider,
};
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::{AddressConfig, TokenConfig};
use crate::contracts::IERC20;

/// Configuration for balance monitoring
#[derive(Debug, Clone)]
pub struct BalanceMonitorConfig {
    pub addresses: Vec<AddressConfig>,
    pub tokens: Vec<TokenConfig>,
    pub interval: Duration,
}

impl BalanceMonitorConfig {
    pub fn new(addresses: Vec<AddressConfig>, tokens: Vec<TokenConfig>, interval: Duration) -> Self {
        Self {
            addresses,
            tokens,
            interval,
        }
    }
}

/// Token balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub alias: String,
    #[serde(with = "u256_serde")]
    pub balance: U256,
    pub formatted: String,
}

/// Balance check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceInfo {
    pub alias: String,
    #[serde(with = "address_serde")]
    pub address: Address,
    #[serde(with = "u256_serde")]
    pub eth_balance: U256,
    pub eth_formatted: String,
    pub token_balances: Vec<TokenBalance>,
}

// Custom serialization for U256
mod u256_serde {
    use alloy::primitives::U256;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &U256, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<U256, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// Custom serialization for Address
mod address_serde {
    use alloy::primitives::Address;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Address, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:?}", value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Address, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Balance monitoring
pub struct BalanceMonitor<P> {
    provider: P,
    config: BalanceMonitorConfig,
}

impl<P: Provider> BalanceMonitor<P> {
    pub fn new(provider: P, config: BalanceMonitorConfig) -> Self {
        Self { provider, config }
    }

    /// Get balance for a single address
    pub async fn get_balance(&self, alias: String, address: Address) -> Result<BalanceInfo> {
    // ETH balance
        let eth_balance = self.provider.get_balance(address).await?;
        let eth_formatted = format_units(eth_balance, "ether")?;

    // Token balances
        let mut token_balances = Vec::new();
        for token in &self.config.tokens {
            let token_contract = IERC20::new(token.address, &self.provider);

            match token_contract.balanceOf(address).call().await {
                Ok(balance) => {
                    let formatted = format_units(balance, 18)
                        .unwrap_or_else(|_| balance.to_string());

                    token_balances.push(TokenBalance {
                        alias: token.alias.clone(),
                        balance,
                        formatted,
                    });
                }
                Err(e) => {
                    eprintln!("Error getting balance {} for {}: {}", token.alias, address, e);
                }
            }
        }

        Ok(BalanceInfo {
            alias,
            address,
            eth_balance,
            eth_formatted,
            token_balances,
        })
    }

    /// Check balances for all addresses
    pub async fn check(&self) -> Vec<Result<BalanceInfo>> {
        let mut results = Vec::new();

        for addr_config in &self.config.addresses {
            let result = self.get_balance(addr_config.alias.clone(), addr_config.address).await;
            results.push(result);
        }

        results
    }

    /// Check interval from configuration
    pub fn interval(&self) -> Duration {
        self.config.interval
    }
}
