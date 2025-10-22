use crate::monitoring::BalanceInfo;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Storage for balance snapshots
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceStorage {
    /// Map of address alias to balance info
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

    /// Update with new balance info
    pub fn update(&mut self, info: &BalanceInfo) {
        self.balances.insert(info.alias.clone(), info.clone());
    }

    /// Get previous balance by alias
    pub fn get(&self, alias: &str) -> Option<&BalanceInfo> {
        self.balances.get(alias)
    }
}

impl Default for BalanceStorage {
    fn default() -> Self {
        Self::new()
    }
}
