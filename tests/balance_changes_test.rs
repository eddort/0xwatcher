use alloy::{
    network::TransactionBuilder,
    node_bindings::Anvil,
    primitives::{address, utils::parse_ether, Address, U256},
    providers::{Provider, ProviderBuilder},
    rpc::types::TransactionRequest,
};
use Oxwatcher::{compare_balances, BalanceInfo, BalanceStorage, TokenBalance, IERC20};
use eyre::Result;

// USDT contract address on Ethereum mainnet
const USDT_ADDRESS: Address = address!("dAC17F958D2ee523a2206206994597C13D831ec7");
// Rich address with USDT balance (Binance hot wallet)
const RICH_ADDRESS: Address = address!("28C6c06298d514Db089934071355E5743bf21d60");

#[tokio::test]
async fn test_eth_balance_changes_detection() -> Result<()> {
    // Start Anvil with Ethereum mainnet fork and auto-impersonate
    let anvil = Anvil::new()
        .fork("https://ethereum.publicnode.com")
        .auto_impersonate()
        .try_spawn()?;
    let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    // Use a forked account with real balance
    let account = RICH_ADDRESS;
    let recipient = anvil.addresses()[0];

    // Get initial ETH balance
    let balance_initial = provider.get_balance(account).await?;
    println!("Initial ETH balance: {}", balance_initial);

    // Create initial balance info
    let initial_info = BalanceInfo {
        network_name: "Ethereum".to_string(),
        chain_id: 1,
        alias: "rich_account".to_string(),
        address: account,
        eth_balance: balance_initial,
        eth_formatted: format_units_manual(balance_initial, 18),
        token_balances: vec![],
    };

    // Create storage and store initial balance
    let mut storage = BalanceStorage::new();
    storage.update(&initial_info);

    // Send ETH (auto-impersonate enabled)
    let tx = TransactionRequest::default()
        .with_from(account)
        .with_to(recipient)
        .with_value(parse_ether("1.0")?);

    provider.send_transaction(tx).await?.watch().await?;

    // Get new balance after transfer
    let balance_new = provider.get_balance(account).await?;
    println!("New ETH balance: {}", balance_new);

    // Create new balance info
    let new_info = BalanceInfo {
        network_name: "Ethereum".to_string(),
        chain_id: 1,
        alias: "rich_account".to_string(),
        address: account,
        eth_balance: balance_new,
        eth_formatted: format_units_manual(balance_new, 18),
        token_balances: vec![],
    };

    // Compare balances and check that change was detected
    let changes = compare_balances(&new_info, &storage);

    // Verify change was detected
    assert!(changes.has_changes(), "ETH balance change should be detected");

    // Verify the change is a decrease
    let eth_change = changes.eth_change.expect("ETH change should exist");
    assert!(eth_change.new_balance < eth_change.old_balance, "Balance should decrease");

    println!("✓ ETH balance change detection test passed");
    Ok(())
}

#[tokio::test]
async fn test_token_balance_changes_detection() -> Result<()> {
    // Start Anvil with Ethereum mainnet fork and auto-impersonate
    let anvil = Anvil::new()
        .fork("https://ethereum.publicnode.com")
        .auto_impersonate()
        .try_spawn()?;
    let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    // Use rich address with USDT balance
    let account = RICH_ADDRESS;
    let recipient = anvil.addresses()[0];

    // Create USDT contract instance
    let usdt = IERC20::new(USDT_ADDRESS, &provider);

    // Get initial USDT balance
    let initial_balance = usdt.balanceOf(account).call().await?;
    println!("Initial USDT balance: {}", initial_balance);

    // Create initial balance info
    let initial_info = BalanceInfo {
        network_name: "Ethereum".to_string(),
        chain_id: 1,
        alias: "rich_account".to_string(),
        address: account,
        eth_balance: U256::ZERO,
        eth_formatted: "0".to_string(),
        token_balances: vec![TokenBalance {
            alias: "USDT".to_string(),
            balance: initial_balance,
            formatted: format_units_manual(initial_balance, 6), // USDT has 6 decimals
        }],
    };

    // Create storage and store initial balance
    let mut storage = BalanceStorage::new();
    storage.update(&initial_info);

    // Transfer USDT (auto-impersonate enabled)
    let transfer_amount = U256::from(1000000u64); // 1 USDT (6 decimals)
    usdt.transfer(recipient, transfer_amount)
        .from(account)
        .send()
        .await?
        .watch()
        .await?;

    // Get new balance after transfer
    let new_balance = usdt.balanceOf(account).call().await?;
    println!("New USDT balance: {}", new_balance);

    // Create new balance info
    let new_info = BalanceInfo {
        network_name: "Ethereum".to_string(),
        chain_id: 1,
        alias: "rich_account".to_string(),
        address: account,
        eth_balance: U256::ZERO,
        eth_formatted: "0".to_string(),
        token_balances: vec![TokenBalance {
            alias: "USDT".to_string(),
            balance: new_balance,
            formatted: format_units_manual(new_balance, 6),
        }],
    };

    // Compare balances and check that change was detected
    let changes = compare_balances(&new_info, &storage);

    // Verify change was detected
    assert!(changes.has_changes(), "Token balance change should be detected");

    // Verify the token change is a decrease
    assert!(!changes.token_changes.is_empty(), "Should have token changes");
    let token_change = &changes.token_changes[0];
    assert!(
        token_change.new_balance < token_change.old_balance,
        "Token balance should decrease"
    );

    println!("✓ Token balance change detection test passed");
    Ok(())
}

#[tokio::test]
async fn test_no_changes_detection() -> Result<()> {
    // Start Anvil with fork
    let anvil = Anvil::new()
        .fork("https://ethereum.publicnode.com")
        .try_spawn()?;
    let provider = ProviderBuilder::new().connect_http(anvil.endpoint_url());

    // Use rich account
    let account = RICH_ADDRESS;

    // Get balance
    let balance = provider.get_balance(account).await?;

    // Create balance info
    let info = BalanceInfo {
        network_name: "Ethereum".to_string(),
        chain_id: 1,
        alias: "account".to_string(),
        address: account,
        eth_balance: balance,
        eth_formatted: format_units_manual(balance, 18),
        token_balances: vec![],
    };

    // Create storage and store balance
    let mut storage = BalanceStorage::new();
    storage.update(&info);

    // Compare with same balance (no changes)
    let changes = compare_balances(&info, &storage);

    // Verify no changes detected
    assert!(!changes.has_changes(), "Should not detect changes when balance is the same");

    println!("✓ No changes detection test passed");
    Ok(())
}

// Helper function to format units manually
fn format_units_manual(value: U256, decimals: u8) -> String {
    let divisor = U256::from(10u128.pow(decimals as u32));
    let whole = value / divisor;
    let remainder = value % divisor;

    // Format with 4 decimal places
    let fractional = (remainder * U256::from(10000u128) / divisor).to::<u64>();
    format!("{}.{:04}", whole, fractional)
}
