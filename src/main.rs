use balance_monitor::{
    create_fallback_provider, log_balances, BalanceMonitor, BalanceMonitorConfig, Config,
    FallbackConfig,
};
use eyre::Result;

#[tokio::main]
async fn main() -> Result<()> {
    let config_path = "config.yaml";

    // Load configuration
    let config = Config::from_file(config_path)?;

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
    println!("Monitoring started\n");

    // Main loop
    loop {
        let results = monitor.check().await;
        log_balances(&results);

        tokio::time::sleep(monitor.interval()).await;
    }
}
