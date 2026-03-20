use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::config::{AgentConfig, Config};

// ── Message ───────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,     // "user" | "assistant"
    pub content: String,
}

// ── Trait ─────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait Backend: Send + Sync {
    async fn chat(&self, system: Option<&str>, messages: &[ChatMessage]) -> Result<String>;
}

// ── Anthropic (Claude) ────────────────────────────────────────────────────────

pub struct AnthropicBackend {
    pub api_key: String,
    pub model: String,
}

#[async_trait]
impl Backend for AnthropicBackend {
    async fn chat(&self, system: Option<&str>, messages: &[ChatMessage]) -> Result<String> {
        let client = reqwest::Client::new();

        let msgs: Vec<serde_json::Value> = messages.iter()
            .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 2048,
            "messages": msgs
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

        let json: serde_json::Value = resp.json().await
            .context("Failed to parse Anthropic response")?;

        json["content"][0]["text"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Unexpected Anthropic response: {}", json))
    }
}

// ── OpenAI-compatible (OpenAI, Ollama, LM Studio, …) ─────────────────────────

pub struct OpenAiBackend {
    pub url: String,
    pub api_key: Option<String>,
    pub model: String,
}

#[async_trait]
impl Backend for OpenAiBackend {
    async fn chat(&self, system: Option<&str>, messages: &[ChatMessage]) -> Result<String> {
        let client = reqwest::Client::new();

        let mut msgs = vec![];
        if let Some(sys) = system {
            msgs.push(serde_json::json!({"role": "system", "content": sys}));
        }
        for m in messages {
            msgs.push(serde_json::json!({"role": m.role, "content": m.content}));
        }

        let body = serde_json::json!({ "model": self.model, "messages": msgs });
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

/// Build a backend by name. Used by both scheduled agent jobs and the chat endpoint.
pub fn build_backend(backend: &str, model: &str, config: &Config) -> Result<Box<dyn Backend>> {
    match backend.to_lowercase().as_str() {
        "anthropic" | "claude" => {
            let api_key = config.anthropic.api_key.clone()
                .filter(|k| !k.is_empty())
                .ok_or_else(|| anyhow::anyhow!(
                    "Backend '{}' requires [anthropic] api_key in config.toml", backend
                ))?;
            Ok(Box::new(AnthropicBackend { api_key, model: model.to_string() }))
        }
        "openai" | "ollama" | "lmstudio" => {
            Ok(Box::new(OpenAiBackend {
                url: config.openai.url.clone(),
                api_key: config.openai.api_key.clone().filter(|k| !k.is_empty()),
                model: model.to_string(),
            }))
        }
        other => anyhow::bail!(
            "Unknown backend '{}'. Valid options: anthropic, openai, ollama, lmstudio", other
        ),
    }
}

// ── Run (scheduled agent jobs) ────────────────────────────────────────────────

pub async fn run(cfg: &AgentConfig, backend: &dyn Backend) -> Result<String> {
    backend.chat(cfg.system.as_deref(), &[ChatMessage {
        role: "user".to_string(),
        content: cfg.prompt.clone(),
    }]).await
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AnthropicConfig, OpenAiConfig};

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
            chat: None,
            jobs: vec![],
        }
    }

    // ── build_backend: anthropic ──────────────────────────────────────────────

    #[test]
    fn anthropic_backend_requires_api_key() {
        let config = test_config(None, "http://localhost:11434/v1");
        let err = build_backend("anthropic", "test-model", &config).err().unwrap().to_string();
        assert!(err.contains("api_key"), "got: {err}");
    }

    #[test]
    fn anthropic_empty_key_treated_as_missing() {
        let config = test_config(Some(""), "http://localhost:11434/v1");
        assert!(build_backend("anthropic", "test-model", &config).is_err());
    }

    #[test]
    fn anthropic_backend_with_key_succeeds() {
        let config = test_config(Some("sk-ant-test"), "http://localhost:11434/v1");
        assert!(build_backend("anthropic", "test-model", &config).is_ok());
    }

    #[test]
    fn claude_is_alias_for_anthropic() {
        let config = test_config(Some("sk-ant-test"), "http://localhost:11434/v1");
        assert!(build_backend("claude", "test-model", &config).is_ok());
    }

    // ── build_backend: openai-compatible ─────────────────────────────────────

    #[test]
    fn openai_backend_succeeds_without_key() {
        let config = test_config(None, "https://api.openai.com/v1");
        assert!(build_backend("openai", "gpt-4", &config).is_ok());
    }

    #[test]
    fn ollama_alias_succeeds() {
        let config = test_config(None, "http://localhost:11434/v1");
        assert!(build_backend("ollama", "qwen3:8b", &config).is_ok());
    }

    #[test]
    fn lmstudio_alias_succeeds() {
        let config = test_config(None, "http://localhost:1234/v1");
        assert!(build_backend("lmstudio", "phi3", &config).is_ok());
    }

    #[test]
    fn backend_matching_is_case_insensitive() {
        let config = test_config(Some("key"), "http://localhost:11434/v1");
        assert!(build_backend("ANTHROPIC", "m", &config).is_ok());
        assert!(build_backend("Ollama", "m", &config).is_ok());
    }

    #[test]
    fn unknown_backend_returns_error() {
        let config = test_config(None, "http://localhost:11434/v1");
        let err = build_backend("gemini", "m", &config).err().unwrap().to_string();
        assert!(err.contains("Unknown backend"), "got: {err}");
        assert!(err.contains("gemini"), "got: {err}");
    }

    // ── run() ─────────────────────────────────────────────────────────────────

    struct MockBackend { response: String }

    #[async_trait]
    impl Backend for MockBackend {
        async fn chat(&self, _: Option<&str>, _: &[ChatMessage]) -> Result<String> {
            Ok(self.response.clone())
        }
    }

    struct EchoBackend;

    #[async_trait]
    impl Backend for EchoBackend {
        async fn chat(&self, system: Option<&str>, messages: &[ChatMessage]) -> Result<String> {
            let last = messages.last().map(|m| m.content.as_str()).unwrap_or("");
            Ok(format!("sys={} prompt={}", system.unwrap_or("none"), last))
        }
    }

    fn agent_cfg() -> AgentConfig {
        AgentConfig {
            prompt: "hello".to_string(),
            backend: "ollama".to_string(),
            model: "qwen3:8b".to_string(),
            system: None,
        }
    }

    #[tokio::test]
    async fn run_returns_backend_output() {
        let backend = MockBackend { response: "pong".to_string() };
        assert_eq!(run(&agent_cfg(), &backend).await.unwrap(), "pong");
    }

    #[tokio::test]
    async fn run_passes_prompt_to_backend() {
        let mut cfg = agent_cfg();
        cfg.prompt = "what is rust?".to_string();
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("what is rust?"), "got: {result}");
    }

    #[tokio::test]
    async fn run_passes_system_prompt_to_backend() {
        let mut cfg = agent_cfg();
        cfg.system = Some("be concise".to_string());
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("be concise"), "got: {result}");
    }

    #[tokio::test]
    async fn run_passes_none_when_no_system_prompt() {
        let cfg = agent_cfg();
        let result = run(&cfg, &EchoBackend).await.unwrap();
        assert!(result.contains("sys=none"), "got: {result}");
    }

    #[tokio::test]
    async fn chat_multi_turn_passes_all_messages() {
        struct HistoryBackend;
        #[async_trait]
        impl Backend for HistoryBackend {
            async fn chat(&self, _: Option<&str>, messages: &[ChatMessage]) -> Result<String> {
                Ok(format!("turns={}", messages.len()))
            }
        }
        let cfg = agent_cfg();
        let messages = vec![
            ChatMessage { role: "user".to_string(), content: "hi".to_string() },
            ChatMessage { role: "assistant".to_string(), content: "hello".to_string() },
            ChatMessage { role: "user".to_string(), content: "how are you?".to_string() },
        ];
        let result = HistoryBackend.chat(None, &messages).await.unwrap();
        assert_eq!(result, "turns=3");
    }
}
