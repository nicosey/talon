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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AnthropicConfig, OpenAiConfig};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn agent_cfg(backend: &str) -> AgentConfig {
        AgentConfig {
            prompt: "hello".to_string(),
            backend: backend.to_string(),
            model: "test-model".to_string(),
            system: None,
        }
    }

    fn test_config(anthropic_key: Option<&str>, openai_url: &str) -> Config {
        Config {
            telegram_token: "tok".to_string(),
            telegram_chat_id: "123".to_string(),
            timezone: "UTC".to_string(),
            log_level: "info".to_string(),
            web_port: 3030,
            anthropic: AnthropicConfig {
                api_key: anthropic_key.map(|s| s.to_string()),
            },
            openai: OpenAiConfig {
                url: openai_url.to_string(),
                api_key: None,
            },
            store_path: String::new(),
            jobs: vec![],
        }
    }

    // ── build_backend: anthropic ──────────────────────────────────────────────

    #[test]
    fn anthropic_backend_requires_api_key() {
        let config = test_config(None, "http://localhost:11434/v1");
        let err = build_backend(&agent_cfg("anthropic"), &config).err().unwrap().to_string();
        assert!(err.contains("api_key"), "got: {err}");
    }

    #[test]
    fn anthropic_empty_key_treated_as_missing() {
        let config = test_config(Some(""), "http://localhost:11434/v1");
        assert!(build_backend(&agent_cfg("anthropic"), &config).is_err());
    }

    #[test]
    fn anthropic_backend_with_key_succeeds() {
        let config = test_config(Some("sk-ant-test"), "http://localhost:11434/v1");
        assert!(build_backend(&agent_cfg("anthropic"), &config).is_ok());
    }

    #[test]
    fn claude_is_alias_for_anthropic() {
        let config = test_config(Some("sk-ant-test"), "http://localhost:11434/v1");
        assert!(build_backend(&agent_cfg("claude"), &config).is_ok());
    }

    // ── build_backend: openai-compatible ─────────────────────────────────────

    #[test]
    fn openai_backend_succeeds_without_key() {
        let config = test_config(None, "https://api.openai.com/v1");
        assert!(build_backend(&agent_cfg("openai"), &config).is_ok());
    }

    #[test]
    fn ollama_alias_succeeds() {
        let config = test_config(None, "http://localhost:11434/v1");
        assert!(build_backend(&agent_cfg("ollama"), &config).is_ok());
    }

    #[test]
    fn lmstudio_alias_succeeds() {
        let config = test_config(None, "http://localhost:1234/v1");
        assert!(build_backend(&agent_cfg("lmstudio"), &config).is_ok());
    }

    #[test]
    fn backend_matching_is_case_insensitive() {
        let config = test_config(Some("key"), "http://localhost:11434/v1");
        assert!(build_backend(&agent_cfg("ANTHROPIC"), &config).is_ok());
        assert!(build_backend(&agent_cfg("Ollama"), &config).is_ok());
    }

    #[test]
    fn unknown_backend_returns_error() {
        let config = test_config(None, "http://localhost:11434/v1");
        let err = build_backend(&agent_cfg("gemini"), &config).err().unwrap().to_string();
        assert!(err.contains("Unknown backend"), "got: {err}");
        assert!(err.contains("gemini"), "got: {err}");
    }

    // ── run() ─────────────────────────────────────────────────────────────────

    struct MockBackend {
        response: String,
    }

    #[async_trait]
    impl Backend for MockBackend {
        async fn complete(&self, _system: Option<&str>, _prompt: &str) -> Result<String> {
            Ok(self.response.clone())
        }
    }

    struct EchoBackend;

    #[async_trait]
    impl Backend for EchoBackend {
        async fn complete(&self, system: Option<&str>, prompt: &str) -> Result<String> {
            Ok(format!("sys={} prompt={}", system.unwrap_or("none"), prompt))
        }
    }

    #[tokio::test]
    async fn run_returns_backend_output() {
        let backend = MockBackend { response: "pong".to_string() };
        let cfg = agent_cfg("anthropic");
        assert_eq!(run(&cfg, &backend).await.unwrap(), "pong");
    }

    #[tokio::test]
    async fn run_passes_prompt_to_backend() {
        let mut cfg = agent_cfg("anthropic");
        cfg.prompt = "what is rust?".to_string();
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("what is rust?"), "got: {result}");
    }

    #[tokio::test]
    async fn run_passes_system_prompt_to_backend() {
        let mut cfg = agent_cfg("anthropic");
        cfg.system = Some("be concise".to_string());
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("be concise"), "got: {result}");
    }

    #[tokio::test]
    async fn run_passes_none_when_no_system_prompt() {
        let cfg = agent_cfg("anthropic"); // system is None
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("sys=none"), "got: {result}");
    }
}
