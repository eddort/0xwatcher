use crate::monitoring::BalanceInfo;
use crate::storage::BalanceStorage;
use alloy::primitives::U256;
use eyre::Result;
use std::collections::HashMap;

/// Represents a change in balance
#[derive(Debug)]
pub enum BalanceChange {
    Increase,
    Decrease,
    NoChange,
}

/// Token balance change details
#[derive(Debug)]
pub struct TokenBalanceChange {
    pub alias: String,
    pub old_balance: U256,
    pub new_balance: U256,
    pub old_formatted: String,
    pub new_formatted: String,
    pub change: BalanceChange,
}

/// Balance change summary for an address
#[derive(Debug)]
pub struct BalanceChangeSummary {
    pub alias: String,
    pub address: String,
    pub eth_change: Option<TokenBalanceChange>,
    pub token_changes: Vec<TokenBalanceChange>,
}

impl BalanceChangeSummary {
    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        let eth_changed = self.eth_change.as_ref()
            .map(|c| !matches!(c.change, BalanceChange::NoChange))
            .unwrap_or(false);

        let tokens_changed = self.token_changes.iter()
            .any(|c| !matches!(c.change, BalanceChange::NoChange));

        eth_changed || tokens_changed
    }
}

/// Compare balances and detect changes
pub fn compare_balances(
    current: &BalanceInfo,
    storage: &BalanceStorage
) -> BalanceChangeSummary {
    let mut eth_change = None;
    let mut token_changes = Vec::new();

    if let Some(previous) = storage.get(&current.alias) {
        // Compare ETH balance
        let change = if current.eth_balance > previous.eth_balance {
            BalanceChange::Increase
        } else if current.eth_balance < previous.eth_balance {
            BalanceChange::Decrease
        } else {
            BalanceChange::NoChange
        };

        eth_change = Some(TokenBalanceChange {
            alias: "ETH".to_string(),
            old_balance: previous.eth_balance,
            new_balance: current.eth_balance,
            old_formatted: previous.eth_formatted.clone(),
            new_formatted: current.eth_formatted.clone(),
            change,
        });

        // Compare token balances
        let previous_tokens: HashMap<_, _> = previous.token_balances.iter()
            .map(|t| (t.alias.as_str(), t))
            .collect();

        for current_token in &current.token_balances {
            if let Some(previous_token) = previous_tokens.get(current_token.alias.as_str()) {
                let change = if current_token.balance > previous_token.balance {
                    BalanceChange::Increase
                } else if current_token.balance < previous_token.balance {
                    BalanceChange::Decrease
                } else {
                    BalanceChange::NoChange
                };

                token_changes.push(TokenBalanceChange {
                    alias: current_token.alias.clone(),
                    old_balance: previous_token.balance,
                    new_balance: current_token.balance,
                    old_formatted: previous_token.formatted.clone(),
                    new_formatted: current_token.formatted.clone(),
                    change,
                });
            } else {
                // New token (first time seeing it)
                token_changes.push(TokenBalanceChange {
                    alias: current_token.alias.clone(),
                    old_balance: U256::ZERO,
                    new_balance: current_token.balance,
                    old_formatted: "0".to_string(),
                    new_formatted: current_token.formatted.clone(),
                    change: if current_token.balance > U256::ZERO {
                        BalanceChange::Increase
                    } else {
                        BalanceChange::NoChange
                    },
                });
            }
        }
    }

    BalanceChangeSummary {
        alias: current.alias.clone(),
        address: format!("{:?}", current.address),
        eth_change,
        token_changes,
    }
}

/// Log only balance changes
pub fn log_balance_changes(change_summary: &BalanceChangeSummary) {
    if !change_summary.has_changes() {
        return;
    }

    println!("🔔 Balance Alert: {} ({})", change_summary.alias, shorten_address(&change_summary.address));

    // Log ETH changes
    if let Some(eth) = &change_summary.eth_change {
        if !matches!(eth.change, BalanceChange::NoChange) {
            let (symbol, sign) = match eth.change {
                BalanceChange::Increase => ("📈", "+"),
                BalanceChange::Decrease => ("📉", ""),
                BalanceChange::NoChange => ("  ", ""),
            };

            let diff = calculate_diff(&eth.new_balance, &eth.old_balance);
            let percent = calculate_percent_change(&eth.new_balance, &eth.old_balance);

            if percent.abs() >= 0.01 {
                println!("   {} ETH: {}{} ({:+.2}%) | {} → {}",
                    symbol,
                    sign,
                    diff,
                    percent,
                    eth.old_formatted,
                    eth.new_formatted
                );
            } else {
                println!("   {} ETH: {}{} | {} → {}",
                    symbol,
                    sign,
                    diff,
                    eth.old_formatted,
                    eth.new_formatted
                );
            }
        }
    }

    // Log token changes
    for token in &change_summary.token_changes {
        if !matches!(token.change, BalanceChange::NoChange) {
            let (symbol, sign) = match token.change {
                BalanceChange::Increase => ("📈", "+"),
                BalanceChange::Decrease => ("📉", ""),
                BalanceChange::NoChange => ("  ", ""),
            };

            let diff = calculate_diff(&token.new_balance, &token.old_balance);
            let percent = calculate_percent_change(&token.new_balance, &token.old_balance);

            if percent.abs() >= 0.01 {
                println!("   {} {}: {}{} ({:+.2}%) | {} → {}",
                    symbol,
                    token.alias,
                    sign,
                    diff,
                    percent,
                    token.old_formatted,
                    token.new_formatted
                );
            } else {
                println!("   {} {}: {}{} | {} → {}",
                    symbol,
                    token.alias,
                    sign,
                    diff,
                    token.old_formatted,
                    token.new_formatted
                );
            }
        }
    }
    println!();
}

/// Shorten address for display
fn shorten_address(address: &str) -> String {
    if address.len() > 10 {
        format!("{}...{}", &address[..6], &address[address.len()-4..])
    } else {
        address.to_string()
    }
}

/// Calculate difference between two U256 values
fn calculate_diff(new: &U256, old: &U256) -> String {
    use alloy::primitives::utils::format_units;

    if new > old {
        let diff = *new - *old;
        format_units(diff, 18).unwrap_or_else(|_| diff.to_string())
    } else {
        let diff = *old - *new;
        format_units(diff, 18).unwrap_or_else(|_| diff.to_string())
    }
}

/// Calculate percent change
fn calculate_percent_change(new: &U256, old: &U256) -> f64 {
    if *old == U256::ZERO {
        return 0.0;
    }

    let old_f64 = old.to_string().parse::<f64>().unwrap_or(0.0);
    let new_f64 = new.to_string().parse::<f64>().unwrap_or(0.0);

    if old_f64 == 0.0 {
        return 0.0;
    }

    ((new_f64 - old_f64) / old_f64) * 100.0
}

/// Simple console logging
pub fn log_balances(results: &[Result<BalanceInfo>]) {
    println!("=== Balance Check ===\n");

    for result in results {
        match result {
            Ok(info) => {
                println!("📌 {} ({})", info.alias, info.address);
                println!("   ETH: {}", info.eth_formatted);

                for token_balance in &info.token_balances {
                    println!("   {}: {}", token_balance.alias, token_balance.formatted);
                }
                println!();
            }
            Err(e) => {
                println!("❌ Error: {}\n", e);
            }
        }
    }
}

/// JSON logging
pub fn log_balances_json(results: &[Result<BalanceInfo>]) -> Result<()> {
    use serde_json::json;

    for result in results {
        if let Ok(info) = result {
            let mut tokens = serde_json::Map::new();
            for token in &info.token_balances {
                tokens.insert(token.alias.clone(), json!(token.formatted));
            }

            let log = json!({
                "alias": info.alias,
                "address": format!("{}", info.address),
                "eth": info.eth_formatted,
                "tokens": tokens,
            });

            println!("{}", serde_json::to_string(&log)?);
        }
    }

    Ok(())
}
