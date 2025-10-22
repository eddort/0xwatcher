use crate::config::TelegramConfig;
use crate::logger::{BalanceChange, BalanceChangeSummary};
use crate::monitoring::BalanceInfo;
use alloy::primitives::U256;
use eyre::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use teloxide::utils::command::BotCommands;
use tokio::sync::RwLock;

/// Registration information for a chat
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChatRegistration {
    chat_id: i64,
    user_id: i64,
    username: String,
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
}

impl TelegramNotifier {
    pub fn new(config: &TelegramConfig) -> Self {
        let bot = Bot::new(&config.bot_token);
        let storage_path = "telegram_chats.json".to_string();

        // Load previously registered chats
        let storage = ChatStorage::load_from_file(&storage_path);

        // Filter only authorized users (auto-cleanup on startup)
        let registered_chats: HashMap<ChatId, ChatRegistration> = storage
            .registrations
            .into_iter()
            .filter(|reg| config.allowed_users.contains(&reg.username))
            .map(|reg| (ChatId(reg.chat_id), reg))
            .collect();

        let loaded_count = registered_chats.len();
        if loaded_count > 0 {
            println!("Loaded {} authorized Telegram chat(s)", loaded_count);
        }

        Self {
            bot,
            registered_chats: Arc::new(RwLock::new(registered_chats)),
            latest_balances: Arc::new(RwLock::new(Vec::new())),
            allowed_users: config.allowed_users.clone(),
            storage_path,
        }
    }

    /// Check if user is allowed to use the bot
    pub fn is_user_allowed(&self, username: Option<&str>) -> bool {
        // Check if username is in whitelist
        if let Some(username) = username {
            self.allowed_users.iter().any(|u| u == username)
        } else {
            false
        }
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

        for (&chat_id, registration) in chats.iter() {
            // Check if user is still authorized
            if !self.allowed_users.contains(&registration.username) {
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

        // Address with shortened format
        let short_addr = Self::shorten_address(&changes.address);
        message.push_str(&format!("📍 <b>{}</b>\n", changes.alias));
        message.push_str(&format!("<code>{}</code>\n\n", short_addr));

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
    fn format_balance_message(balances: &[BalanceInfo]) -> String {
        if balances.is_empty() {
            return "No balance data available yet.".to_string();
        }

        let mut message = String::from("💰 <b>Current Balances</b>\n\n");

        for balance in balances {
            let short_addr = Self::shorten_address(&format!("{:?}", balance.address));
            message.push_str(&format!("📍 <b>{}</b>\n", balance.alias));
            message.push_str(&format!("<code>{}</code>\n\n", short_addr));

            message.push_str(&format!("💵 ETH: <b>{}</b>\n", balance.eth_formatted));

            for token in &balance.token_balances {
                message.push_str(&format!("💵 {}: <b>{}</b>\n", token.alias, token.formatted));
            }
            message.push_str("\n");
        }

        message
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
            let message = TelegramNotifier::format_balance_message(&balances);
            bot.send_message(msg.chat.id, message)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
        Command::Help => {
            let help_text = "🤖 <b>Balance Monitor Bot</b>\n\n\
                             Available commands:\n\
                             /start - Register for balance alerts\n\
                             /balance - Show current balances\n\
                             /help - Show this message\n\n\
                             The bot will automatically send alerts when balance changes are detected.";
            bot.send_message(msg.chat.id, help_text)
                .parse_mode(teloxide::types::ParseMode::Html)
                .await?;
        }
    }

    Ok(())
}
