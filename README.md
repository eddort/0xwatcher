# Balance Monitor

Scalable monitoring of Ethereum address balances in Rust using Alloy v1.0.


## Project structure

```
src/
├── main.rs              # Entry point with logging
├── lib.rs               # Public API
├── config.rs            # Load configuration from YAML
├── logger.rs            # Logging module
├── contracts/           # Smart contract definitions
│   ├── mod.rs
│   └── erc20.rs         # ERC-20 interface
├── providers/           # RPC providers
│   ├── mod.rs
│   └── fallback.rs      # Fallback provider
└── monitoring/          # Monitoring logic (no logging)
    ├── mod.rs
    # Balance Monitor

    Scalable monitoring of Ethereum address balances in Rust using Alloy v1.0.

    ## Features

    - 🔄 **Fallback provider** - automatic switching between RPC nodes
    - 💰 **ETH + ERC-20** - monitor native ETH and any tokens
    - 🏷️ **Aliases** - friendly names for addresses and tokens
    - ⚙️ **YAML configuration** - single source of truth
    - 📦 **Modular architecture** - separation of logic and logging
    - 📝 **Flexible logging** - console, JSON, or custom format

    ## Project structure

    ```
    src/
    ├── main.rs              # Entry point with logging
    ├── lib.rs               # Public API
    ├── config.rs            # Load configuration from YAML
    ├── logger.rs            # Logging module
    ├── contracts/           # Smart contract definitions
    │   ├── mod.rs
    │   └── erc20.rs         # ERC-20 interface
    ├── providers/           # RPC providers
    │   ├── mod.rs
    │   └── fallback.rs      # Fallback provider
    └── monitoring/          # Monitoring logic (no logging)
        ├── mod.rs
        └── balance.rs       # Monitoring logic
    ```

    ## Quick start

    ### 1. Create a configuration file

    ```bash
    cp config.yaml.example config.yaml
    ```

    Edit `config.yaml`:

    ```yaml
    # RPC nodes for fallback
    rpc_nodes:
      - https://eth.llamarpc.com
      - https://eth.drpc.org
      - https://ethereum.publicnode.com

    # Addresses to monitor
    addresses:
      - alias: My Wallet
        address: 0xYourAddress

    # ERC-20 tokens to monitor
    tokens:
      - alias: USDT
        address: 0xdAC17F958D2ee523a2206206994597C13D831ec7

    # Check interval (seconds)
    interval_secs: 10

    # Number of active RPC transports
    active_transport_count: 3
    ```

    ### 2. Run monitoring

    ```bash
    cargo run
    ```

    Output:

    ```
    Monitoring started
    Tracked addresses: 1
    Tokens: 2
    Interval: 10 sec

    === Balance Check ===

    📌 My Wallet (0xYour...)
       ETH: 1.234567
       USDT: 500.00
    ```

    ## Configuration

    ### RPC nodes

    Add multiple RPC endpoints for resiliency:

    ```yaml
    rpc_nodes:
      - https://eth.llamarpc.com
      - https://rpc.ankr.com/eth
      - https://ethereum.publicnode.com
      - https://cloudflare-eth.com
    ```

    ### Addresses

    Each address has an alias for convenience:

    ```yaml
    addresses:
      - alias: Personal Wallet
        address: 0x...
      - alias: Trading Wallet
        address: 0x...
      - alias: Cold Storage
        address: 0x...
    ```

    ### Tokens

    Add any ERC-20 tokens:

    ```yaml
    tokens:
      - alias: USDC
        address: 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
      - alias: DAI
        address: 0x6B175474E89094C44Da98b954EedeAC495271d0F
      - alias: WETH
        address: 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2
    ```

    ## Using as a library

    ### Basic example

    ```rust
    use balance_monitor::{
        Config, create_fallback_provider, FallbackConfig,
        BalanceMonitor, BalanceMonitorConfig, log_balances,
    };

    #[tokio::main]
    async fn main() -> eyre::Result<()> {
        let config = Config::from_file("config.yaml")?;

        let provider_config = FallbackConfig::new(
            config.rpc_nodes,
            config.active_transport_count,
        );
        let provider = create_fallback_provider(provider_config)?;

        let monitor_config = BalanceMonitorConfig::new(
            config.addresses,
            config.tokens,
            config.interval_secs,
        );
        let monitor = BalanceMonitor::new(provider, monitor_config);

        // Fetch data
        let results = monitor.check().await;

        // Log (optional)
        log_balances(&results);

        Ok(())
    }
    ```

    ### Custom logging

    ```rust
    // Monitor returns Vec<Result<BalanceInfo>>
    let results = monitor.check().await;

    for result in results {
        match result {
            Ok(info) => {
                // Your processing
                println!("{}: {} ETH", info.alias, info.eth_formatted);

                // Send to DB
                // Send to Telegram
                // Save to a file
            }
            Err(e) => eprintln!("Error: {}", e),
        }
    }
    ```

    ### JSON logging

    ```rust
    use balance_monitor::log_balances_json;

    loop {
        let results = monitor.check().await;
        log_balances_json(&results)?;

        tokio::time::sleep(monitor.interval()).await;
    }
    ```

    ## Examples

    ### Single check

    ```bash
    cargo run --example single_check
    ```

    ### JSON logger

    ```bash
    cargo run --example json_logger
    ```

    ## Run

    ```bash
    # Development
    cargo run

    # Release
    cargo build --release
    ./target/release/balance-monitor
    ```

    ## Architecture

    ### Modules

    - **config** - Load and validate YAML configuration
    - **contracts** - Smart contract definitions (ERC-20)
    - **providers** - Create RPC providers with fallback
    - **monitoring** - Balance collection logic (no logging)
    - **logger** - Displaying results (console, JSON)

    ### Separation of concerns

    **BalanceMonitor** only collects data:
    ```rust
    let results: Vec<Result<BalanceInfo>> = monitor.check().await;
    ```

    **Logger** only displays:
    ```rust
    log_balances(&results);       // Console
    log_balances_json(&results)?; // JSON
    ```

    You can add your own logger without changing monitoring.

    ### Single source of truth

    All configuration lives in `config.yaml`. No hardcoded values and reasonable defaults.

    ## Extending

    ### Adding a new token

    Add to `config.yaml`:

    ```yaml
    tokens:
      - alias: MyToken
        address: 0xTokenAddress
    ```

    ### Adding a new contract type

    1. Create a file in `src/contracts/`
    2. Define the interface with the `sol!` macro
    3. Export it in `mod.rs`

    Example:

    ```rust
    // src/contracts/uniswap.rs
    use alloy::sol;

    sol! {
        #[sol(rpc)]
        interface IUniswapV2Pair {
            function getReserves() external view
                returns (uint112, uint112, uint32);
        }
    }
    ```

    ## Dependencies

    - `alloy 1.0` - Modern Ethereum library
    - `tokio` - Async runtime
    - `eyre` - Error handling
    - `tower` - Middleware for fallback
    - `serde` + `serde_yaml` - Configuration handling

    ## License

    MIT
