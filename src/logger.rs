use crate::monitoring::BalanceInfo;
use eyre::Result;

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
