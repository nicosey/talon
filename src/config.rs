use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct AgentConfig {
    pub prompt: String,
    pub backend: String,        // "anthropic" | "openai"
    pub model: String,
    pub system: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct Job {
    pub name: String,
    pub schedule: String,
    pub command: Option<String>,
    pub agent: Option<AgentConfig>,
}

// ── Backend configs ───────────────────────────────────────────────────────────

#[derive(Deserialize, Clone, Default)]
pub struct AnthropicConfig {
    pub api_key: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct OpenAiConfig {
    /// Base URL — works for OpenAI, Ollama, LM Studio, etc.
    #[serde(default = "default_openai_url")]
    pub url: String,
    pub api_key: Option<String>,
}

fn default_openai_url() -> String {
    "http://localhost:11434/v1".to_string()
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self { url: default_openai_url(), api_key: None }
    }
}

// ── Top-level config ──────────────────────────────────────────────────────────

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
    #[serde(default)]
    pub anthropic: AnthropicConfig,
    #[serde(default)]
    pub openai: OpenAiConfig,
    pub jobs: Vec<Job>,
}

fn default_timezone() -> String  { "UTC".to_string() }
fn default_log_level() -> String { "info".to_string() }
fn default_web_port() -> u16     { 3030 }

pub fn load() -> Result<Config> {
    let content = std::fs::read_to_string("config.toml")
        .context("config.toml not found. Please create it first.")?;

    let config: Config = toml::from_str(&content).context("Failed to parse config.toml")?;

    // Validate each job has exactly one of command or agent
    for job in &config.jobs {
        match (&job.command, &job.agent) {
            (None, None) => anyhow::bail!("Job '{}' must have either 'command' or 'agent'", job.name),
            (Some(_), Some(_)) => anyhow::bail!("Job '{}' cannot have both 'command' and 'agent'", job.name),
            _ => {}
        }
    }

    Ok(config)
}
