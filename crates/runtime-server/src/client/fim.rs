use anyhow::Context;
use serde_json::json;

use super::http::{ERROR_BODY_MAX_BYTES, api_url, bounded_error_text};
use super::types::DeepSeekClient;

impl DeepSeekClient {
    /// Call the DeepSeek `/beta/completions` FIM endpoint.
    pub async fn fim_completion(
        &self,
        model: &str,
        prompt: &str,
        suffix: &str,
        max_tokens: u32,
    ) -> anyhow::Result<String> {
        let url = api_url(&self.base_url, "beta/completions");
        let body = json!({
            "model": model,
            "prompt": prompt,
            "suffix": suffix,
            "max_tokens": max_tokens,
        });
        let response = self
            .send_with_retry(|| self.http_client.post(&url).json(&body))
            .await?;
        let status = response.status();
        if !status.is_success() {
            let error_text = bounded_error_text(response, ERROR_BODY_MAX_BYTES).await;
            anyhow::bail!("FIM API error: HTTP {status}: {error_text}");
        }
        let response_text = response.text().await.unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&response_text).context("Failed to parse FIM API response")?;
        let text = value
            .pointer("/choices/0/text")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("FIM response missing choices[0].text"))?;
        Ok(text.to_string())
    }
}
