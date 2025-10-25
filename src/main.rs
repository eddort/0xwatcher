use balance_monitor::{
    compare_balances, create_fallback_provider, log_balance_changes, BalanceMonitor,
    BalanceMonitorConfig, BalanceStorage, Config, FallbackConfig, NetworkConfig, TelegramNotifier,
};
use eyre::Result;
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = "config.yaml";
    let storage_path = "balances.json";

    // Load configuration
    let config = Config::from_file(config_path)?;

    // Load previous balance storage
    let storage = Arc::new(RwLock::new(BalanceStorage::load_from_file(storage_path)?));

    // Initialize Telegram notifier if configured
    let telegram_notifier = if let Some(telegram_config) = &config.telegram {
        println!("Telegram notifications enabled");
        let notifier = TelegramNotifier::new(telegram_config);
        notifier.clone().spawn_command_handler();
        Some(Arc::new(notifier))
    } else {
        println!("Telegram notifications disabled (not configured)");
        None
    };

    println!("Balance monitoring started");
    println!("Storage file: {}", storage_path);
    println!("Monitoring {} network(s)", config.networks.len());
    println!("Changes will be logged when detected\n");

    // Spawn monitoring task for each network
    let mut handles = Vec::new();

    for network in config.networks {
        let storage_clone = Arc::clone(&storage);
        let telegram_clone = telegram_notifier.clone();
        let interval = config.interval;
        let active_transport_count = config.active_transport_count;
        let storage_path_clone = storage_path.to_string();

        let handle = tokio::spawn(async move {
            if let Err(e) = monitor_network(
                network,
                storage_clone,
                telegram_clone,
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

async fn monitor_network(
    network: NetworkConfig,
    storage: Arc<RwLock<BalanceStorage>>,
    telegram_notifier: Option<Arc<TelegramNotifier>>,
    interval: std::time::Duration,
    active_transport_count: std::num::NonZeroUsize,
    storage_path: String,
) -> Result<()> {
    println!("🌐 Starting monitor for network: {} (Chain ID: {})", network.name, network.chain_id);

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

                        // Send Telegram alert if enabled
                        if let Some(ref notifier) = telegram_notifier {
                            if let Err(e) = notifier.send_alert(&changes).await {
                                eprintln!("⚠️  Failed to send Telegram alert: {}", e);
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
