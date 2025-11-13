use Oxwatcher::{
    compare_balances, create_fallback_provider, log_balance_changes, AlertSettings, BalanceMonitor,
    BalanceMonitorConfig, BalanceStorage, Config, FallbackConfig, NetworkConfig, TelegramNotifier,
};
use chrono::Local;
use eyre::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = "config.yaml";

    // Load configuration
    let config = Config::from_file(config_path)?;

    // Create data directory if it doesn't exist
    std::fs::create_dir_all(&config.data_dir)?;

    // Build storage path using data_dir from config
    let storage_path = format!("{}/balances.json", config.data_dir);

    // Print startup banner
    print_startup_banner(&config);

    // Load previous balance storage
    let storage = Arc::new(RwLock::new(BalanceStorage::load_from_file(&storage_path)?));

    // Initialize Telegram notifier if configured
    let telegram_notifier = if let Some(telegram_config) = &config.telegram {
        let notifier = TelegramNotifier::new(telegram_config, Arc::clone(&storage), &config.data_dir);

        // Count loaded chats
        let loaded_chats = notifier.get_registered_chats_count().await;
        if loaded_chats > 0 {
            println!("📲 Loaded {} authorized Telegram chat(s)", loaded_chats);
        }

        // Spawn command handler
        notifier.clone().spawn_command_handler();

        // Spawn daily report scheduler if configured
        if telegram_config.daily_report.is_some() {
            notifier.clone().spawn_daily_report_scheduler();
        }

        Some(Arc::new(notifier))
    } else {
        None
    };

    println!("✅ Balance monitoring started");
    println!("💾 Data directory: {}", config.data_dir);
    println!("💾 Storage file: {}", storage_path);
    println!();

    // Spawn monitoring task for each network
    let mut handles = Vec::new();

    let alert_settings = config.get_alert_settings();

    for network in config.networks.clone() {
        let storage_clone = Arc::clone(&storage);
        let telegram_clone = telegram_notifier.clone();
        let alert_settings_clone = alert_settings.clone();
        let interval = config.interval;
        let active_transport_count = config.active_transport_count;
        let storage_path_clone = storage_path.to_string();

        let handle = tokio::spawn(async move {
            if let Err(e) = monitor_network(
                network,
                storage_clone,
                telegram_clone,
                alert_settings_clone,
                interval,
                active_transport_count,
                storage_path_clone,
            )
            .await
            {
                eprintln!("❌ Network monitoring error: {}", e);
            }
        });

        handles.push(handle);
    }

    // Wait for all tasks to complete (they run indefinitely)
    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

fn print_startup_banner(config: &Config) {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║           Balance Monitor - Configuration Summary             ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    // Server time
    let now = Local::now();
    println!("🕐 Server Time: {}", now.format("%Y-%m-%d %H:%M:%S %Z"));
    println!();

    // Global settings
    println!("⚙️  Global Settings:");
    println!("   • Check interval: {} seconds", config.interval.as_secs());
    println!("   • Active RPC connections: {}", config.active_transport_count);
    println!();

    // Networks configuration
    println!("🌐 Networks ({}):", config.networks.len());
    for (idx, network) in config.networks.iter().enumerate() {
        println!("   {}. {} (Chain ID: {})", idx + 1, network.name, network.chain_id);
        println!("      • RPC nodes: {}", network.rpc_nodes.len());
        println!("      • Addresses to monitor: {}", network.addresses.len());

        // Show addresses with thresholds
        for addr in &network.addresses {
            if let Some(threshold) = addr.min_balance_eth {
                println!("         - {} (⚠️  Low balance alert: < {} ETH)", addr.alias, threshold);
            } else {
                println!("         - {}", addr.alias);
            }
        }

        if !network.tokens.is_empty() {
            println!("      • Tokens to monitor: {}", network.tokens.len());
            for token in &network.tokens {
                if let Some(threshold) = token.min_balance {
                    println!("         - {} (⚠️  Low balance alert: < {})", token.alias, threshold);
                } else {
                    println!("         - {}", token.alias);
                }
            }
        }

        if idx < config.networks.len() - 1 {
            println!();
        }
    }
    println!();

    // Telegram configuration
    if let Some(telegram) = &config.telegram {
        println!("📱 Telegram Notifications: ENABLED");

        // Check if public mode
        let is_public = telegram.allowed_users.iter().any(|u| u == "all");
        if is_public {
            println!("   • Access mode: 🌍 PUBLIC (anyone can use the bot)");
        } else {
            println!("   • Access mode: 🔒 PRIVATE");
            println!("   • Authorized users: {}", telegram.allowed_users.len());
            for user in &telegram.allowed_users {
                println!("      - @{}", user);
            }
        }
        println!();

        // Alert settings
        println!("   🔔 Alert Settings:");
        println!("      - Balance change alerts: {}",
            if telegram.alerts.balance_change { "✅ ENABLED" } else { "❌ DISABLED" });
        println!("      - Low balance alerts: {}",
            if telegram.alerts.low_balance { "✅ ENABLED" } else { "❌ DISABLED" });
        println!();

        // Daily report configuration
        println!("   📊 Daily Reports:");
        if let Some(daily_report) = &telegram.daily_report {
            if daily_report.enabled {
                println!("      - Status: ✅ ENABLED");
                println!("      - Report time: {} (24-hour format)", daily_report.time);
                println!("      - Next report: ~{} {}",
                    daily_report.time,
                    if now.format("%H:%M").to_string() < daily_report.time { "today" } else { "tomorrow" }
                );
            } else {
                println!("      - Status: ❌ DISABLED");
            }
        } else {
            println!("      - Status: NOT CONFIGURED");
        }
        println!();

        println!("   💬 Bot Commands:");
        println!("      - /balance - Show current balances");
        println!("      - /report - Get on-demand diff report");
    } else {
        println!("📱 Telegram Notifications: DISABLED");
    }

    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!();
}

async fn monitor_network(
    network: NetworkConfig,
    storage: Arc<RwLock<BalanceStorage>>,
    telegram_notifier: Option<Arc<TelegramNotifier>>,
    alert_settings: AlertSettings,
    interval: std::time::Duration,
    active_transport_count: std::num::NonZeroUsize,
    storage_path: String,
) -> Result<()> {
    println!("🌐 Starting monitor for network: {} (Chain ID: {})", network.name, network.chain_id);

    // Build threshold maps for low balance alerts
    let mut address_thresholds: HashMap<String, f64> = HashMap::new();
    for addr in &network.addresses {
        if let Some(threshold) = addr.min_balance_eth {
            address_thresholds.insert(addr.alias.clone(), threshold);
        }
    }

    let mut token_thresholds: HashMap<String, f64> = HashMap::new();
    for token in &network.tokens {
        if let Some(threshold) = token.min_balance {
            token_thresholds.insert(token.alias.clone(), threshold);
        }
    }

    // Create provider for this network
    let provider_config = FallbackConfig::new(network.rpc_nodes.clone(), active_transport_count);
    let provider = create_fallback_provider(provider_config)?;

    // Create monitor for this network
    let monitor_config = BalanceMonitorConfig::new(network.addresses.clone(), network.tokens.clone(), interval);
    let monitor = BalanceMonitor::new(provider, monitor_config);

    // Main monitoring loop for this network
    loop {
        let results = monitor.check(network.name.clone(), network.chain_id).await;
        let mut all_balances = Vec::new();

        // Process each result
        for result in results {
            match result {
                Ok(balance_info) => {
                    // Compare with previous balances
                    let changes = {
                        let storage_read = storage.read().await;
                        compare_balances(&balance_info, &storage_read)
                    };

                    // Log only if there are changes
                    if changes.has_changes() {
                        log_balance_changes(&changes);

                        // Send Telegram alert if enabled and balance_change alerts are enabled
                        if alert_settings.balance_change {
                            if let Some(ref notifier) = telegram_notifier {
                                if let Err(e) = notifier.send_alert(&changes).await {
                                    eprintln!("⚠️  Failed to send Telegram alert: {}", e);
                                }
                            }
                        }
                    }

                    // Check for low balance alerts if enabled
                    if alert_settings.low_balance {
                        if let Some(ref notifier) = telegram_notifier {
                            let eth_threshold = address_thresholds.get(&balance_info.alias).copied();
                            if let Err(e) = notifier.check_low_balance_alerts(&balance_info, eth_threshold, &token_thresholds).await {
                                eprintln!("⚠️  Failed to check low balance alerts: {}", e);
                            }
                        }
                    }

                    // Store balance for later
                    all_balances.push(balance_info.clone());

                    // Update storage with new balance
                    {
                        let mut storage_write = storage.write().await;
                        storage_write.update(&balance_info);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Error checking balance on {}: {}\n", network.name, e);
                }
            }
        }

        // Update Telegram notifier with latest balances
        if let Some(ref notifier) = telegram_notifier {
            notifier.update_balances(all_balances).await;
        }

        // Save storage to file after each check
        {
            let storage_read = storage.read().await;
            if let Err(e) = storage_read.save_to_file(&storage_path) {
                eprintln!("⚠️  Failed to save storage: {}", e);
            }
        }

        tokio::time::sleep(interval).await;
    }
}
