use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub command: String,
    pub schedule: String, // cron expression: "sec min hour dom month dow year"
}

#[derive(Deserialize, Clone)]
pub struct Config {
    pub telegram_token: String,
    pub telegram_chat_id: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_web_port")]
    pub web_port: u16,
    pub jobs: Vec<Job>,
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn default_web_port() -> u16 {
    3030
}

pub fn load() -> Result<Config> {
    let content = std::fs::read_to_string("config.toml")
        .context("config.toml not found. Please create it first.")?;

    toml::from_str(&content).context("Failed to parse config.toml")
}
