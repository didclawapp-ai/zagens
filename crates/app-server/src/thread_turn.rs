//! PR5 — single-shot LLM turn for `ThreadRequest::Message` (app-server path).

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use deepseek_config::{CliRuntimeOverrides, ConfigToml};
use deepseek_core::{ThreadMessageTurnPort, ThreadMessageTurnRequest, ThreadMessageTurnResult};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::{Value, json};
use tokio::sync::RwLock;

pub struct AppServerLlmTurnPort {
    config: Arc<RwLock<ConfigToml>>,
}

impl AppServerLlmTurnPort {
    pub fn new(config: Arc<RwLock<ConfigToml>>) -> Self {
        Self { config }
    }

    fn chat_completions_url(base_url: &str) -> String {
        let trimmed = base_url.trim_end_matches('/');
        if trimmed.ends_with("/v1") {
            format!("{trimmed}/chat/completions")
        } else {
            format!("{trimmed}/v1/chat/completions")
        }
    }
}

#[async_trait]
impl ThreadMessageTurnPort for AppServerLlmTurnPort {
    async fn run_turn(
        &self,
        req: ThreadMessageTurnRequest,
    ) -> anyhow::Result<ThreadMessageTurnResult> {
        let config = self.config.read().await;
        let resolved = config.resolve_runtime_options(&CliRuntimeOverrides::default());
        let api_key = resolved
            .api_key
            .filter(|key| !key.trim().is_empty())
            .context("api_key is not configured")?;
        let base_url = if resolved.base_url.trim().is_empty() {
            "https://api.deepseek.com".to_string()
        } else {
            resolved.base_url.clone()
        };
        let url = Self::chat_completions_url(&base_url);
        let body = json!({
            "model": req.model,
            "messages": [{"role": "user", "content": req.input}],
            "max_tokens": 2048,
            "stream": false
        });
        let response = reqwest::Client::new()
            .post(url)
            .header(AUTHORIZATION, format!("Bearer {api_key}"))
            .header(CONTENT_TYPE, "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        let payload: Value = response.json().await?;
        let assistant_text = payload
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if assistant_text.is_empty() {
            anyhow::bail!("empty assistant response from upstream");
        }
        Ok(ThreadMessageTurnResult {
            status: "completed".to_string(),
            assistant_text,
        })
    }
}
