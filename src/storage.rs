use crate::monitoring::BalanceInfo;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Storage for balance snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceStorage {
    /// Map of "network:alias" to balance info
    pub balances: HashMap<String, BalanceInfo>,
}

impl BalanceStorage {
    /// Create new empty storage
    pub fn new() -> Self {
        Self {
            balances: HashMap::new(),
        }
    }

    /// Load from file, return empty storage if file doesn't exist
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Ok(Self::new());
        }

        let content = fs::read_to_string(path)?;
        let storage: BalanceStorage = serde_json::from_str(&content)?;
        Ok(storage)
    }

    /// Save to file
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Generate storage key from network name and alias
    fn make_key(network_name: &str, alias: &str) -> String {
        format!("{}:{}", network_name, alias)
    }

    /// Update with new balance info
    pub fn update(&mut self, info: &BalanceInfo) {
        let key = Self::make_key(&info.network_name, &info.alias);
        self.balances.insert(key, info.clone());
    }

    /// Get previous balance by network name and alias
    pub fn get(&self, network_name: &str, alias: &str) -> Option<&BalanceInfo> {
        let key = Self::make_key(network_name, alias);
        self.balances.get(&key)
    }
}

impl Default for BalanceStorage {
    fn default() -> Self {
        Self::new()
    }
}
