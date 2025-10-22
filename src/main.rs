use balance_monitor::{
    compare_balances, create_fallback_provider, log_balance_changes, BalanceMonitor,
    BalanceMonitorConfig, BalanceStorage, Config, FallbackConfig, TelegramNotifier,
};
use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = "config.yaml";
    let storage_path = "balances.json";

    // Load configuration
    let config = Config::from_file(config_path)?;

    // Load previous balance storage
    let mut storage = BalanceStorage::load_from_file(storage_path)?;

    // Create provider
    let provider_config = FallbackConfig::new(
        config.rpc_nodes.clone(),
        config.active_transport_count,
    );
    let provider = create_fallback_provider(provider_config)?;

    // Create monitor
    let monitor_config = BalanceMonitorConfig::new(
        config.addresses,
        config.tokens,
        config.interval_secs,
    );
    let monitor = BalanceMonitor::new(provider, monitor_config);

    // Initialize Telegram notifier if configured
    let telegram_notifier = if let Some(telegram_config) = &config.telegram {
        println!("Telegram notifications enabled");
        let notifier = TelegramNotifier::new(telegram_config);
        notifier.clone().spawn_command_handler();
        Some(notifier)
    } else {
        println!("Telegram notifications disabled (not configured)");
        None
    };

    println!("Balance monitoring started");
    println!("Storage file: {}", storage_path);
    println!("Changes will be logged when detected\n");

    // Main loop
    loop {
        let results = monitor.check().await;
        let mut all_balances = Vec::new();

        // Process each result
        for result in results {
            match result {
                Ok(balance_info) => {
                    // Compare with previous balances
                    let changes = compare_balances(&balance_info, &storage);

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
                    storage.update(&balance_info);
                }
                Err(e) => {
                    eprintln!("❌ Error checking balance: {}\n", e);
                }
            }
        }

        // Update Telegram notifier with latest balances
        if let Some(ref notifier) = telegram_notifier {
            notifier.update_balances(all_balances).await;
        }

        // Save storage to file after each check
        if let Err(e) = storage.save_to_file(storage_path) {
            eprintln!("⚠️  Failed to save storage: {}", e);
        }

        tokio::time::sleep(monitor.interval()).await;
    }
}
