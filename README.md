
A Rust-based blockchain balance monitoring tool with Telegram notifications. Supports multiple EVM-compatible networks with fallback RPC endpoints and intelligent alert throttling.

## Features

- Multi-network support (Ethereum, Polygon, Gnosis, etc.)
- Multiple RPC fallback for high availability
- ERC20 token balance monitoring
- Telegram bot integration with customizable alerts
- Low balance alerts with smart throttling
- Daily balance diff reports
- Balance change notifications
- Persistent state management

## Prerequisites

- Rust 1.70 or higher
- Telegram Bot Token (get from [@BotFather](https://t.me/BotFather))

## Installation

1. Clone the repository
2. Build the project:
```bash
cargo build --release
```

## Configuration

Create a `config.yaml` file in the project root. See `config.example.yaml` for reference.

### Configuration Structure

#### Global Settings

```yaml
interval_secs: 60              # Balance check interval in seconds (default: 60)
active_transport_count: 3      # Number of concurrent RPC connections (default: 3)
```

- `interval_secs`: How often to check balances. Lower values = more frequent checks but higher RPC usage.
- `active_transport_count`: Number of concurrent RPC connections for fallback system. Higher values improve reliability.

#### Telegram Configuration

```yaml
telegram:
  bot_token: "YOUR_BOT_TOKEN"
  allowed_users:
    - "username1"
    - "username2"
    # OR use "all" for public access:
    # - "all"

  alerts:
    balance_change: true
    low_balance: true

  daily_report:
    enabled: true
    time: "09:00"

  show_full_address: false
```

**Fields:**

- `bot_token` (required): Your Telegram bot token from @BotFather
- `allowed_users` (optional): List of authorized Telegram usernames (without @)
  - Use `["all"]` to allow anyone to use the bot
  - Leave empty or specify usernames for private mode
- `alerts.balance_change` (default: true): Send alerts when balance changes are detected
- `alerts.low_balance` (default: true): Send alerts when balance drops below threshold
- `daily_report.enabled` (default: false): Enable daily balance diff reports
- `daily_report.time`: Time to send daily report in HH:MM format (24-hour)
- `show_full_address` (default: false): Display full addresses or shortened format (0xabcd...1234)

#### Network Configuration

```yaml
networks:
  - name: Ethereum
    chain_id: 1
    rpc_nodes:
      - https://eth.llamarpc.com
      - https://eth.drpc.org
      - https://ethereum.publicnode.com
    addresses:
      - alias: MyWallet
        address: 0xYourAddressHere
        min_balance_eth: 1.0
    tokens:
      - alias: USDT
        address: 0xdAC17F958D2ee523a2206206994597C13D831ec7
        min_balance: 100.0
```

**Fields:**

- `name` (required): Network display name
- `chain_id` (required): Network chain ID (1 for Ethereum, 137 for Polygon, etc.)
- `rpc_nodes` (required): List of RPC endpoints
  - First node is primary, others are fallbacks
  - System automatically switches on failure
  - Only HTTP/HTTPS endpoints supported (no WebSocket)
- `addresses` (required): List of addresses to monitor
  - `alias`: Human-readable name for the address
  - `address`: Ethereum address to monitor
  - `min_balance_eth` (optional): ETH balance threshold for low balance alerts
- `tokens` (optional): List of ERC20 tokens to monitor
  - `alias`: Token name (e.g., USDT, USDC)
  - `address`: Token contract address
  - `min_balance` (optional): Token balance threshold for low balance alerts

### Low Balance Alert Throttling

When balance drops below threshold, alerts are sent with increasing intervals to prevent spam:

1. Alert #1: Immediate
2. Alert #2: 10 minutes later
3. Alert #3: 1 hour later
4. Alert #4: 5 hours later
5. Alert #5+: Every 20 hours

Alerts reset when balance goes back above threshold.

## Running the Monitor

### Development Mode

```bash
cargo run
```

### Production Mode

```bash
cargo build --release
./target/release/balance-monitor
```

## Telegram Bot Commands

After starting the bot, users can interact with it using these commands:

- `/start` - Register for alerts
- `/balance` - Show current balances
- `/report` - Get on-demand balance diff report
- `/help` - Show help message

## File Structure

The application creates the following files:

- `balances.json` - Stores last known balances for change detection
- `telegram_chats.json` - Stores registered Telegram chats
- `alert_states.json` - Stores low balance alert throttling state

## Example Configuration

### Ethereum Mainnet Only

```yaml
interval_secs: 60
active_transport_count: 3

telegram:
  bot_token: "1234567890:ABCdefGHIjklMNOpqrsTUVwxyz"
  allowed_users:
    - "your_username"
  alerts:
    balance_change: false
    low_balance: true
  daily_report:
    enabled: true
    time: "09:00"

networks:
  - name: Ethereum
    chain_id: 1
    rpc_nodes:
      - https://eth.llamarpc.com
      - https://eth.drpc.org
    addresses:
      - alias: Main Wallet
        address: 0x742d35Cc6634C0532925a3b844Bc9e7595f0bEb
        min_balance_eth: 0.5
    tokens:
      - alias: USDC
        address: 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
        min_balance: 1000.0
```

### Multi-Network Setup

```yaml
interval_secs: 30
active_transport_count: 3

telegram:
  bot_token: "your_token_here"
  allowed_users:
    - "all"  # Public bot
  alerts:
    balance_change: true
    low_balance: true

networks:
  - name: Ethereum
    chain_id: 1
    rpc_nodes:
      - https://eth.llamarpc.com
    addresses:
      - alias: Hot Wallet
        address: 0xYourAddress1
        min_balance_eth: 1.0

  - name: Polygon
    chain_id: 137
    rpc_nodes:
      - https://polygon-rpc.com
    addresses:
      - alias: Cold Wallet
        address: 0xYourAddress2
        min_balance_eth: 10.0

  - name: Gnosis Chiado Testnet
    chain_id: 10200
    rpc_nodes:
      - https://rpc.chiado.gnosis.gateway.fm
    addresses:
      - alias: Test Wallet
        address: 0xYourAddress3
        min_balance_eth: 0.1
```

## Troubleshooting

### Bot Not Responding

1. Check that bot token is correct
2. Verify your username is in `allowed_users` list (or use "all")
3. Make sure you sent `/start` to the bot first

### RPC Connection Issues

1. Check RPC endpoint availability
2. Add more fallback RPC endpoints
3. Increase `active_transport_count` for better reliability
4. Check network connectivity

### Missing Balance Changes

1. Verify `interval_secs` is set appropriately
2. Check that `alerts.balance_change` is enabled
3. Review console logs for errors

### Too Many Low Balance Alerts

1. Alerts are automatically throttled (10min, 1hr, 5hr, 20hr intervals)
2. Disable with `alerts.low_balance: false`
3. Adjust `min_balance_eth` thresholds

## Security Considerations

- Never commit `config.yaml` with real bot tokens to version control
- Use `.gitignore` to exclude sensitive configuration files
- Store bot tokens in environment variables for production
- Limit bot access using `allowed_users` whitelist
- Regularly rotate bot tokens

## License

MIT License

## Support

For issues and feature requests, please use the GitHub issue tracker.
