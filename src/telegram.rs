use crate::config::{TelegramConfig, DailyReportConfig};
use crate::logger::{BalanceChange, BalanceChangeSummary};
use crate::monitoring::BalanceInfo;
use crate::storage::BalanceStorage;
use alloy::primitives::U256;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use teloxide::utils::command::BotCommands;
use tokio::sync::RwLock;
use chrono::{Local, NaiveTime};

/// Registration information for a chat
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatRegistration {
    chat_id: i64,
    user_id: i64,
    username: String,
}

/// Alert state for tracking when alerts were last sent
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlertState {
    /// Last time alert was sent (Unix timestamp in seconds)
    last_sent: u64,
    /// Number of alerts sent (used to determine next interval)
    alert_count: u32,
}

impl AlertState {
    fn new() -> Self {
        Self {
            last_sent: 0,
            alert_count: 0,
        }
    }

    /// Get the required interval before next alert based on alert count
    /// 1st: immediate, 2nd: 10min, 3rd: 1hr, 4th: 5hr, 5th: 20hr, 6th+: 20hr
    fn get_next_interval_secs(&self) -> u64 {
        match self.alert_count {
            0 => 0,           // First alert - immediate
            1 => 10 * 60,     // 10 minutes
            2 => 60 * 60,     // 1 hour
            3 => 5 * 60 * 60, // 5 hours
            _ => 20 * 60 * 60, // 20 hours (for 4th and beyond)
        }
    }

    /// Check if enough time has passed to send another alert
    fn should_send_alert(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let required_interval = self.get_next_interval_secs();
        now >= self.last_sent + required_interval
    }

    /// Record that an alert was sent
    fn record_alert_sent(&mut self) {
        self.last_sent = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.alert_count += 1;
    }

    /// Reset alert state (e.g., when balance goes back above threshold)
    fn reset(&mut self) {
        self.last_sent = 0;
        self.alert_count = 0;
    }
}

/// Storage for alert states
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AlertStateStorage {
    /// Map of "network:alias" to alert state
    states: HashMap<String, AlertState>,
}

impl AlertStateStorage {
    fn new() -> Self {
        Self {
            states: HashMap::new(),
        }
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if !path.exists() {
            return Self::new();
        }

        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_else(Self::new)
    }

    fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn make_key(network: &str, alias: &str) -> String {
        format!("{}:{}", network, alias)
    }

    fn get_or_create(&mut self, network: &str, alias: &str) -> &mut AlertState {
        let key = Self::make_key(network, alias);
        self.states.entry(key).or_insert_with(AlertState::new)
    }
}

/// Storage for registered chat IDs
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatStorage {
    registrations: Vec<ChatRegistration>,
}

impl ChatStorage {
    fn new() -> Self {
        Self {
            registrations: Vec::new(),
        }
    }

    fn load_from_file<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref();
        if !path.exists() {
            return Self::new();
        }

        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_else(Self::new)
    }

    fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_json::to_string_pretty(&self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

/// Telegram notifier for balance changes
#[derive(Clone)]
pub struct TelegramNotifier {
    bot: Bot,
    registered_chats: Arc<RwLock<HashMap<ChatId, ChatRegistration>>>,
    latest_balances: Arc<RwLock<Vec<BalanceInfo>>>,
    allowed_users: Vec<String>,
    storage_path: String,
    daily_report_config: Option<DailyReportConfig>,
    balance_storage: Arc<RwLock<BalanceStorage>>,
    show_full_address: bool,
    alert_state_storage: Arc<RwLock<AlertStateStorage>>,
    alert_state_path: String,
}

impl TelegramNotifier {
    pub fn new(config: &TelegramConfig, balance_storage: Arc<RwLock<BalanceStorage>>) -> Self {
        let bot = Bot::new(&config.bot_token);
        let storage_path = "telegram_chats.json".to_string();

        // Load previously registered chats
        let storage = ChatStorage::load_from_file(&storage_path);

        // Filter only authorized users (auto-cleanup on startup)
        // If "all" is in allowed_users, keep all registered chats
        let is_public = config.allowed_users.iter().any(|u| u == "all");
        let registered_chats: HashMap<ChatId, ChatRegistration> = storage
            .registrations
            .into_iter()
            .filter(|reg| is_public || config.allowed_users.contains(&reg.username))
            .map(|reg| (ChatId(reg.chat_id), reg))
            .collect();

        let alert_state_path = "alert_states.json".to_string();
        let alert_state_storage = AlertStateStorage::load_from_file(&alert_state_path);

        Self {
            bot,
            registered_chats: Arc::new(RwLock::new(registered_chats)),
            latest_balances: Arc::new(RwLock::new(Vec::new())),
            allowed_users: config.allowed_users.clone(),
            storage_path,
            daily_report_config: config.daily_report.clone(),
            balance_storage,
            show_full_address: config.show_full_address,
            alert_state_storage: Arc::new(RwLock::new(alert_state_storage)),
            alert_state_path,
        }
    }

    /// Check if user is allowed to use the bot
    pub fn is_user_allowed(&self, username: Option<&str>) -> bool {
        // Special case: if "all" is in allowed_users, allow everyone
        if self.allowed_users.iter().any(|u| u == "all") {
            return true;
        }

        // Check if username is in whitelist
        if let Some(username) = username {
            self.allowed_users.iter().any(|u| u == username)
        } else {
            false
        }
    }

    /// Check if bot is in public mode (allows all users)
    pub fn is_public_mode(&self) -> bool {
        self.allowed_users.iter().any(|u| u == "all")
    }

    /// Get count of registered chats
    pub async fn get_registered_chats_count(&self) -> usize {
        let chats = self.registered_chats.read().await;
        chats.len()
    }

    /// Register a chat for alerts
    pub async fn register_chat(&self, chat_id: ChatId, user: &teloxide::types::User) {
        let username = user.username.clone().unwrap_or_default();
        let registration = ChatRegistration {
            chat_id: chat_id.0,
            user_id: user.id.0 as i64,
            username,
        };

        let mut chats = self.registered_chats.write().await;
        let was_new = chats.insert(chat_id, registration).is_none();

        // Save to file if it's a new chat
        if was_new {
            drop(chats); // Release lock before file I/O
            if let Err(e) = self.save_chats().await {
                eprintln!("Failed to save telegram chats: {}", e);
            }
        }
    }

    /// Save registered chats to file
    async fn save_chats(&self) -> Result<()> {
        let chats = self.registered_chats.read().await;
        let registrations: Vec<ChatRegistration> = chats.values().cloned().collect();
        let storage = ChatStorage { registrations };
        storage.save_to_file(&self.storage_path)?;
        Ok(())
    }

    /// Check if chat is registered
    pub async fn is_registered(&self, chat_id: ChatId) -> bool {
        let chats = self.registered_chats.read().await;
        chats.contains_key(&chat_id)
    }

    /// Unregister a chat
    pub async fn unregister_chat(&self, chat_id: ChatId) {
        let mut chats = self.registered_chats.write().await;
        if chats.remove(&chat_id).is_some() {
            drop(chats);
            if let Err(e) = self.save_chats().await {
                eprintln!("Failed to save telegram chats after unregister: {}", e);
            }
        }
    }

    /// Send alert for balance changes to all registered chats
    pub async fn send_alert(&self, changes: &BalanceChangeSummary) -> Result<()> {
        if !changes.has_changes() {
            return Ok(());
        }

        let message = self.format_change_message(changes);
        let chats = self.registered_chats.read().await;
        let is_public = self.is_public_mode();

        for (&chat_id, registration) in chats.iter() {
            // Check if user is still authorized (skip check in public mode)
            if !is_public && !self.allowed_users.contains(&registration.username) {
                eprintln!("Skipping alert to chat {} (user '{}' no longer authorized)", chat_id, registration.username);
                continue;
            }

            if let Err(e) = self
                .bot
                .send_message(chat_id, message.clone())
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                eprintln!("Failed to send alert to chat {}: {}", chat_id, e);
            }
        }

        Ok(())
    }

    /// Update stored balances
    pub async fn update_balances(&self, balances: Vec<BalanceInfo>) {
        let mut stored = self.latest_balances.write().await;
        *stored = balances;
    }

    /// Get latest balances
    pub async fn get_balances(&self) -> Vec<BalanceInfo> {
        self.latest_balances.read().await.clone()
    }

    /// Format change message for Telegram
    fn format_change_message(&self, changes: &BalanceChangeSummary) -> String {
        let mut message = format!("🔔 <b>Balance Alert</b>\n\n");

        // Network and address (full or shortened)
        let display_addr = if self.show_full_address {
            changes.address.clone()
        } else {
            Self::shorten_address(&changes.address)
        };
        message.push_str(&format!("🌐 <b>{}</b> (Chain ID: {})\n", changes.network_name, changes.chain_id));
        message.push_str(&format!("📍 <b>{}</b>\n", changes.alias));
        message.push_str(&format!("<code>{}</code>\n\n", display_addr));

        // Format ETH changes
        if let Some(eth) = &changes.eth_change {
            if !matches!(eth.change, BalanceChange::NoChange) {
                let (emoji, sign) = match eth.change {
                    BalanceChange::Increase => ("📈", "+"),
                    BalanceChange::Decrease => ("📉", ""),
                    BalanceChange::NoChange => ("", ""),
                };

                let diff = Self::calculate_diff(&eth.new_balance, &eth.old_balance);
                let percent = Self::calculate_percent_change(&eth.new_balance, &eth.old_balance);

                message.push_str(&format!("💰 <b>ETH</b>\n"));
                if percent.abs() >= 0.01 {
                    message.push_str(&format!("{} <b>{}{}</b> ({:+.2}%)\n", emoji, sign, diff, percent));
                } else {
                    message.push_str(&format!("{} <b>{}{}</b>\n", emoji, sign, diff));
                }
                message.push_str(&format!("{} → {}\n\n", eth.old_formatted, eth.new_formatted));
            }
        }

        // Format token changes
        for token in &changes.token_changes {
            if !matches!(token.change, BalanceChange::NoChange) {
                let (emoji, sign) = match token.change {
                    BalanceChange::Increase => ("📈", "+"),
                    BalanceChange::Decrease => ("📉", ""),
                    BalanceChange::NoChange => ("", ""),
                };

                let diff = Self::calculate_diff(&token.new_balance, &token.old_balance);
                let percent = Self::calculate_percent_change(&token.new_balance, &token.old_balance);

                message.push_str(&format!("💰 <b>{}</b>\n", token.alias));
                if percent.abs() >= 0.01 {
                    message.push_str(&format!("{} <b>{}{}</b> ({:+.2}%)\n", emoji, sign, diff, percent));
                } else {
                    message.push_str(&format!("{} <b>{}{}</b>\n", emoji, sign, diff));
                }
                message.push_str(&format!("{} → {}\n\n", token.old_formatted, token.new_formatted));
            }
        }

        message
    }

    /// Shorten address for display (0xabcd...1234)
    fn shorten_address(address: &str) -> String {
        if address.len() > 10 {
            format!("{}...{}", &address[..6], &address[address.len()-4..])
        } else {
            address.to_string()
        }
    }

    /// Calculate difference between two U256 values as formatted string
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

    /// Format balance status message
    fn format_balance_message(&self, balances: &[BalanceInfo]) -> String {
        if balances.is_empty() {
            return "No balance data available yet.".to_string();
        }

        let mut message = String::from("💰 <b>Current Balances</b>\n\n");

        for balance in balances {
            let display_addr = if self.show_full_address {
                format!("{:?}", balance.address)
            } else {
                Self::shorten_address(&format!("{:?}", balance.address))
            };
            message.push_str(&format!("🌐 <b>{}</b> (Chain ID: {})\n", balance.network_name, balance.chain_id));
            message.push_str(&format!("📍 <b>{}</b>\n", balance.alias));
            message.push_str(&format!("<code>{}</code>\n\n", display_addr));

            message.push_str(&format!("💵 ETH: <b>{}</b>\n", balance.eth_formatted));

            for token in &balance.token_balances {
                message.push_str(&format!("💵 {}: <b>{}</b>\n", token.alias, token.formatted));
            }
            message.push_str("\n");
        }

        message
    }

    /// Generate daily diff report for all addresses and networks
    async fn format_daily_report(&self) -> String {
        let balances = self.latest_balances.read().await;
        let storage = self.balance_storage.read().await;

        if balances.is_empty() {
            return "📊 <b>Daily Balance Report</b>\n\nNo balance data available yet.".to_string();
        }

        let mut message = String::from("📊 <b>Daily Balance Report</b>\n");
        message.push_str(&format!("📅 {}\n\n", Local::now().format("%Y-%m-%d %H:%M:%S")));

        let mut total_changes = 0;
        let mut has_any_changes = false;

        for balance in balances.iter() {
            if let Some(previous) = storage.get(&balance.network_name, &balance.alias) {
                let display_addr = if self.show_full_address {
                    format!("{:?}", balance.address)
                } else {
                    Self::shorten_address(&format!("{:?}", balance.address))
                };
                let mut address_changes = Vec::new();

                // Check ETH balance changes
                if balance.eth_balance != previous.eth_balance {
                    let (emoji, sign) = if balance.eth_balance > previous.eth_balance {
                        ("📈", "+")
                    } else {
                        ("📉", "")
                    };
                    let diff = Self::calculate_diff(&balance.eth_balance, &previous.eth_balance);
                    let percent = Self::calculate_percent_change(&balance.eth_balance, &previous.eth_balance);

                    let change_str = if percent.abs() >= 0.01 {
                        format!("{} ETH: {}{} ({:+.2}%) | {} → {}",
                            emoji, sign, diff, percent, previous.eth_formatted, balance.eth_formatted)
                    } else {
                        format!("{} ETH: {}{} | {} → {}",
                            emoji, sign, diff, previous.eth_formatted, balance.eth_formatted)
                    };
                    address_changes.push(change_str);
                    total_changes += 1;
                }

                // Check token balance changes
                let previous_tokens: HashMap<_, _> = previous.token_balances.iter()
                    .map(|t| (t.alias.as_str(), t))
                    .collect();

                for token in &balance.token_balances {
                    if let Some(prev_token) = previous_tokens.get(token.alias.as_str()) {
                        if token.balance != prev_token.balance {
                            let (emoji, sign) = if token.balance > prev_token.balance {
                                ("📈", "+")
                            } else {
                                ("📉", "")
                            };
                            let diff = Self::calculate_diff(&token.balance, &prev_token.balance);
                            let percent = Self::calculate_percent_change(&token.balance, &prev_token.balance);

                            let change_str = if percent.abs() >= 0.01 {
                                format!("{} {}: {}{} ({:+.2}%) | {} → {}",
                                    emoji, token.alias, sign, diff, percent, prev_token.formatted, token.formatted)
                            } else {
                                format!("{} {}: {}{} | {} → {}",
                                    emoji, token.alias, sign, diff, prev_token.formatted, token.formatted)
                            };
                            address_changes.push(change_str);
                            total_changes += 1;
                        }
                    }
                }

                if !address_changes.is_empty() {
                    has_any_changes = true;
                    message.push_str(&format!("🌐 <b>{}</b> | 📍 <b>{}</b>\n", balance.network_name, balance.alias));
                    message.push_str(&format!("<code>{}</code>\n", display_addr));
                    for change in address_changes {
                        message.push_str(&format!("   {}\n", change));
                    }
                    message.push_str("\n");
                }
            }
        }

        if !has_any_changes {
            message.push_str("✅ No balance changes detected in the last period.\n");
        } else {
            message.push_str(&format!("📈 <b>Total changes:</b> {}\n", total_changes));
        }

        message
    }

    /// Check for low balance alerts and send if needed (with throttling)
    pub async fn check_low_balance_alerts(&self, balance: &BalanceInfo, min_eth_threshold: Option<f64>, token_thresholds: &HashMap<String, f64>) -> Result<()> {
        let display_addr = if self.show_full_address {
            format!("{:?}", balance.address)
        } else {
            Self::shorten_address(&format!("{:?}", balance.address))
        };

        // Check if we should send alert for this address
        let mut alert_storage = self.alert_state_storage.write().await;
        let alert_state = alert_storage.get_or_create(&balance.network_name, &balance.alias);

        // Check ETH balance
        let eth_is_low = if let Some(threshold) = min_eth_threshold {
            let eth_value: f64 = balance.eth_formatted.parse().unwrap_or(0.0);
            eth_value < threshold && eth_value > 0.0
        } else {
            false
        };

        // Check token balances
        let tokens_are_low = balance.token_balances.iter().any(|token| {
            if let Some(&threshold) = token_thresholds.get(&token.alias) {
                let token_value: f64 = token.formatted.parse().unwrap_or(0.0);
                token_value < threshold && token_value > 0.0
            } else {
                false
            }
        });

        let balance_is_low = eth_is_low || tokens_are_low;

        // If balance is back to normal, reset alert state
        if !balance_is_low {
            if alert_state.alert_count > 0 {
                alert_state.reset();
                // Save state
                if let Err(e) = alert_storage.save_to_file(&self.alert_state_path) {
                    eprintln!("Failed to save alert state: {}", e);
                }
            }
            return Ok(());
        }

        // Check if we should send alert based on throttling
        if !alert_state.should_send_alert() {
            return Ok(()); // Too soon to send another alert
        }

        // Build alert messages
        let mut alerts = Vec::new();

        if eth_is_low {
            if let Some(threshold) = min_eth_threshold {
                let next_interval = match alert_state.alert_count {
                    0 => "Next alert in 10 minutes".to_string(),
                    1 => "Next alert in 1 hour".to_string(),
                    2 => "Next alert in 5 hours".to_string(),
                    3 => "Next alert in 20 hours".to_string(),
                    _ => "Alerts every 20 hours".to_string(),
                };

                alerts.push(format!("⚠️ <b>LOW BALANCE ALERT #{}</b>\n\n\
                                    🌐 <b>{}</b> (Chain ID: {})\n\
                                    📍 <b>{}</b>\n\
                                    <code>{}</code>\n\n\
                                    💰 ETH: <b>{}</b>\n\
                                    📉 Below threshold: <b>{}</b> ETH\n\
                                    🚨 <b>Please top up your balance!</b>\n\n\
                                    ⏰ {}",
                    alert_state.alert_count + 1,
                    balance.network_name,
                    balance.chain_id,
                    balance.alias,
                    display_addr,
                    balance.eth_formatted,
                    threshold,
                    next_interval
                ));
            }
        }

        for token in &balance.token_balances {
            if let Some(&threshold) = token_thresholds.get(&token.alias) {
                let token_value: f64 = token.formatted.parse().unwrap_or(0.0);
                if token_value < threshold && token_value > 0.0 {
                    let next_interval = match alert_state.alert_count {
                        0 => "Next alert in 10 minutes".to_string(),
                        1 => "Next alert in 1 hour".to_string(),
                        2 => "Next alert in 5 hours".to_string(),
                        3 => "Next alert in 20 hours".to_string(),
                        _ => "Alerts every 20 hours".to_string(),
                    };

                    alerts.push(format!("⚠️ <b>LOW BALANCE ALERT #{}</b>\n\n\
                                        🌐 <b>{}</b> (Chain ID: {})\n\
                                        📍 <b>{}</b>\n\
                                        <code>{}</code>\n\n\
                                        💰 {}: <b>{}</b>\n\
                                        📉 Below threshold: <b>{}</b>\n\
                                        🚨 <b>Please top up your balance!</b>\n\n\
                                        ⏰ {}",
                        alert_state.alert_count + 1,
                        balance.network_name,
                        balance.chain_id,
                        balance.alias,
                        display_addr,
                        token.alias,
                        token.formatted,
                        threshold,
                        next_interval
                    ));
                }
            }
        }

        // Send alerts
        if !alerts.is_empty() {
            let chats = self.registered_chats.read().await;
            let is_public = self.is_public_mode();

            for (&chat_id, registration) in chats.iter() {
                if !is_public && !self.allowed_users.contains(&registration.username) {
                    continue;
                }

                for alert in &alerts {
                    if let Err(e) = self
                        .bot
                        .send_message(chat_id, alert.clone())
                        .parse_mode(teloxide::types::ParseMode::Html)
                        .await
                    {
                        eprintln!("Failed to send low balance alert to chat {}: {}", chat_id, e);
                    }
                }
            }

            // Record that alert was sent
            alert_state.record_alert_sent();

            // Save state
            if let Err(e) = alert_storage.save_to_file(&self.alert_state_path) {
                eprintln!("Failed to save alert state: {}", e);
            }
        }

        Ok(())
    }

    /// Send daily report to all registered chats
    async fn send_daily_report(&self) -> Result<()> {
        let message = self.format_daily_report().await;
        let chats = self.registered_chats.read().await;
        let is_public = self.is_public_mode();

        for (&chat_id, registration) in chats.iter() {
            if !is_public && !self.allowed_users.contains(&registration.username) {
                continue;
            }

            if let Err(e) = self
                .bot
                .send_message(chat_id, message.clone())
                .parse_mode(teloxide::types::ParseMode::Html)
                .await
            {
                eprintln!("Failed to send daily report to chat {}: {}", chat_id, e);
            }
        }

        Ok(())
    }

    /// Start daily report scheduler
    pub fn spawn_daily_report_scheduler(self) {
        if let Some(ref report_config) = self.daily_report_config {
            if !report_config.enabled {
                return;
            }

            let report_time = report_config.time.clone();
            tokio::spawn(async move {
                loop {
                    // Parse target time (HH:MM)
                    let target_time = if let Ok(time) = NaiveTime::parse_from_str(&report_time, "%H:%M") {
                        time
                    } else {
                        eprintln!("Invalid daily report time format: {}. Expected HH:MM", report_time);
                        return;
                    };

                    // Calculate sleep duration until next report time
                    let now = Local::now();
                    let target_datetime = now.date_naive().and_time(target_time);

                    let duration = if now.time() < target_time {
                        // Target is today
                        (target_datetime - now.naive_local()).to_std().unwrap()
                    } else {
                        // Target is tomorrow
                        let tomorrow = now.date_naive().succ_opt().unwrap().and_time(target_time);
                        (tomorrow - now.naive_local()).to_std().unwrap()
                    };

                    println!("Next daily report scheduled in {} hours", duration.as_secs() / 3600);
                    tokio::time::sleep(duration).await;

                    // Send report
                    if let Err(e) = self.send_daily_report().await {
                        eprintln!("Failed to send daily report: {}", e);
                    }

                    // Sleep for a minute to avoid sending multiple reports
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            });
        }
    }

    /// Start bot command handler in background
    pub fn spawn_command_handler(self) {
        tokio::spawn(async move {
            let handler = Update::filter_message()
                .filter_command::<Command>()
                .endpoint(handle_command);

            let mut dispatcher = Dispatcher::builder(self.bot.clone(), handler)
                .dependencies(dptree::deps![self.clone()])
                .default_handler(|_| async {})
                .build();

            dispatcher.dispatch().await;
        });
    }
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Available commands:")]
enum Command {
    #[command(description = "Start bot and register for alerts")]
    Start,
    #[command(description = "Show current balances")]
    Balance,
    #[command(description = "Generate and send balance diff report")]
    Report,
    #[command(description = "Show help")]
    Help,
}

async fn handle_command(
    bot: Bot,
    msg: Message,
    cmd: Command,
    notifier: TelegramNotifier,
) -> Result<(), teloxide::RequestError> {
    // Check if user is authorized
    let user = match msg.from.as_ref() {
        Some(user) => user,
        None => return Ok(()), // Ignore messages without user
    };

    // Centralized authorization check for all commands except Help
    if !matches!(cmd, Command::Help) {
        if !notifier.is_user_allowed(user.username.as_deref()) {
            let message = if user.username.is_none() {
                "❌ Sorry, you need to set a Telegram username to use this bot."
            } else {
                "❌ Sorry, you are not authorized to use this bot."
            };
            bot.send_message(msg.chat.id, message).await?;

            // Unregister chat if it was previously registered
            notifier.unregister_chat(msg.chat.id).await;

            return Ok(());
        }
    }

    match cmd {
        Command::Start => {
            notifier.register_chat(msg.chat.id, user).await;
            let welcome_text = "👋 <b>Welcome to Balance Monitor!</b>\n\n\
                                You will now receive alerts when balance changes are detected.\n\n\
                                Use /balance to see current balances.\n\
                                Use /report to get a diff report.\n\
                                Use /help for more information.";
            bot.send_message(msg.chat.id, welcome_text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Command::Balance => {
            if !notifier.is_registered(msg.chat.id).await {
                bot.send_message(
                    msg.chat.id,
                    "Please start the bot first with /start to receive updates.",
                )
                .await?;
                return Ok(());
            }

            let balances = notifier.get_balances().await;
            let message = notifier.format_balance_message(&balances);
            bot.send_message(msg.chat.id, message)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Command::Report => {
            if !notifier.is_registered(msg.chat.id).await {
                bot.send_message(
                    msg.chat.id,
                    "Please start the bot first with /start to receive updates.",
                )
                .await?;
                return Ok(());
            }

            let report = notifier.format_daily_report().await;
            bot.send_message(msg.chat.id, report)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Command::Help => {
            let help_text = "🤖 <b>Balance Monitor Bot</b>\n\n\
                             Available commands:\n\
                             /start - Register for balance alerts\n\
                             /balance - Show current balances\n\
                             /report - Get balance diff report (cumulative across all addresses and networks)\n\
                             /help - Show this message\n\n\
                             The bot will automatically send alerts when balance changes are detected.\n\
                             If enabled in config, daily reports will be sent automatically.";
            bot.send_message(msg.chat.id, help_text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
    }

    Ok(())
}
