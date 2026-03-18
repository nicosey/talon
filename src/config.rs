use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub telegram_token: String,
    pub telegram_chat_id: String,
    pub command: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

pub fn load() -> Result<Config> {
    let content = std::fs::read_to_string("config.toml")
        .context("config.toml not found. Please create it first.")?;

    toml::from_str(&content).context("Failed to parse config.toml")
}
