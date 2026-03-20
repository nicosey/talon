use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::config::{AgentConfig, Config};

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Backend: Send + Sync {
    async fn complete(&self, system: Option<&str>, prompt: &str) -> Result<String>;
}

// ── Anthropic (Claude) ────────────────────────────────────────────────────────

pub struct AnthropicBackend {
    pub api_key: String,
    pub model: String,
}

#[async_trait]
impl Backend for AnthropicBackend {
    async fn complete(&self, system: Option<&str>, prompt: &str) -> Result<String> {
        let client = reqwest::Client::new();

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 2048,
            "messages": [{ "role": "user", "content": prompt }]
        });
        if let Some(sys) = system {
            body["system"] = serde_json::Value::String(sys.to_string());
        }

        let resp = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic API request failed")?;

        let json: serde_json::Value = resp.json().await.context("Failed to parse Anthropic response")?;

        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Unexpected Anthropic response: {}", json))
    }
}

// ── OpenAI-compatible (OpenAI, Ollama, LM Studio, …) ─────────────────────────

pub struct OpenAiBackend {
    pub url: String,            // e.g. "http://localhost:11434/v1"
    pub api_key: Option<String>,
    pub model: String,
}

#[async_trait]
impl Backend for OpenAiBackend {
    async fn complete(&self, system: Option<&str>, prompt: &str) -> Result<String> {
        let client = reqwest::Client::new();

        let mut messages = vec![];
        if let Some(sys) = system {
            messages.push(serde_json::json!({ "role": "system", "content": sys }));
        }
        messages.push(serde_json::json!({ "role": "user", "content": prompt }));

        let body = serde_json::json!({
            "model": self.model,
            "messages": messages,
        });

        let url = format!("{}/chat/completions", self.url.trim_end_matches('/'));
        let mut req = client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.header("Authorization", format!("Bearer {}", key));
        }

        let resp = req.send().await.context("OpenAI-compatible API request failed")?;
        let json: serde_json::Value = resp.json().await.context("Failed to parse response")?;

        json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Unexpected response: {}", json))
    }
}

// ── Factory ───────────────────────────────────────────────────────────────────

pub fn build_backend(agent_cfg: &AgentConfig, config: &Config) -> Result<Box<dyn Backend>> {
    match agent_cfg.backend.to_lowercase().as_str() {
        "anthropic" | "claude" => {
            let api_key = config.anthropic.api_key.clone()
                .filter(|k| !k.is_empty())
                .ok_or_else(|| anyhow::anyhow!(
                    "Job '{}' uses anthropic backend but [anthropic] api_key is not set", agent_cfg.backend
                ))?;
            Ok(Box::new(AnthropicBackend { api_key, model: agent_cfg.model.clone() }))
        }
        "openai" | "ollama" | "lmstudio" => {
            Ok(Box::new(OpenAiBackend {
                url: config.openai.url.clone(),
                api_key: config.openai.api_key.clone().filter(|k| !k.is_empty()),
                model: agent_cfg.model.clone(),
            }))
        }
        other => anyhow::bail!(
            "Unknown backend '{}'. Valid options: anthropic, openai, ollama, lmstudio", other
        ),
    }
}

// ── Run ───────────────────────────────────────────────────────────────────────

pub async fn run(cfg: &AgentConfig, backend: &dyn Backend) -> Result<String> {
    backend.complete(cfg.system.as_deref(), &cfg.prompt).await
}
