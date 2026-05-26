use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex as AsyncMutex;

use anyhow::Result;

use crate::config::Config;
use crate::llm_client::{
    LlmError, RetryConfig as LlmRetryConfig, extract_retry_after, with_retry,
};
use crate::logging;

use super::api_parse::parse_models_response;
use super::http::{
    ERROR_BODY_MAX_BYTES, add_extra_root_certs, api_url, bounded_error_text, build_default_headers,
    force_http1_from_env, validate_base_url_security,
};
use super::types::{
    AvailableModel, ConnectionHealth, DeepSeekClient, TokenBucket,
    apply_request_failure, apply_request_success, mark_recovery_probe_if_due,
};

impl DeepSeekClient {
    /// Create a DeepSeek client from CLI configuration.
    pub fn new(config: &Config) -> Result<Self> {
        let api_key = config.deepseek_api_key()?;
        let base_url = config.deepseek_base_url();
        let api_provider = config.api_provider();
        validate_base_url_security(&base_url)?;
        let retry = config.retry_policy();
        let default_model = config.default_model();
        let http_headers = config.http_headers();

        logging::info(format!("API provider: {}", api_provider.as_str()));
        logging::info(format!("API base URL: {base_url}"));
        if !http_headers.is_empty() {
            logging::info(format!(
                "{} custom HTTP header(s) configured",
                http_headers.len()
            ));
        }
        logging::info(format!(
            "Retry policy: enabled={}, max_retries={}, initial_delay={}s, max_delay={}s",
            retry.enabled, retry.max_retries, retry.initial_delay, retry.max_delay
        ));

        let http_client = Self::build_http_client(&api_key, &http_headers)?;

        Ok(Self {
            http_client,
            api_key,
            base_url,
            api_provider,
            retry,
            default_model,
            connection_health: Arc::new(AsyncMutex::new(ConnectionHealth::default())),
            rate_limiter: Arc::new(AsyncMutex::new(TokenBucket::from_env())),
        })
    }

    fn build_http_client(
        api_key: &str,
        extra_headers: &HashMap<String, String>,
    ) -> Result<reqwest::Client> {
        let headers = build_default_headers(api_key, extra_headers)?;
        let mut builder = reqwest::Client::builder()
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(30))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .http2_keep_alive_interval(Some(Duration::from_secs(15)))
            .http2_keep_alive_timeout(Duration::from_secs(20))
            .min_tls_version(reqwest::tls::Version::TLS_1_2);
        if force_http1_from_env() {
            logging::info("DEEPSEEK_FORCE_HTTP1=1 — pinning HTTP client to HTTP/1.1");
            builder = builder.http1_only();
        }
        if let Ok(cert_path) = std::env::var("SSL_CERT_FILE")
            && !cert_path.is_empty()
        {
            builder = add_extra_root_certs(builder, &cert_path);
        }
        builder.build().map_err(Into::into)
    }

    #[cfg(test)]
    pub(super) fn default_headers(
        api_key: &str,
        extra_headers: &HashMap<String, String>,
    ) -> Result<reqwest::header::HeaderMap> {
        build_default_headers(api_key, extra_headers)
    }
}
impl DeepSeekClient {
    /// List available models from the provider.
    pub async fn list_models(&self) -> Result<Vec<AvailableModel>> {
        let url = api_url(&self.base_url, "models");
        let response = self.send_with_retry(|| self.http_client.get(&url)).await?;

        let status = response.status();
        if !status.is_success() {
            let error_text = bounded_error_text(response, ERROR_BODY_MAX_BYTES).await;
            anyhow::bail!("Failed to list models: HTTP {status}: {error_text}");
        }
        let response_text = response.text().await.unwrap_or_default();

        parse_models_response(&response_text)
    }

    pub(super) async fn wait_for_rate_limit(&self) {
        let maybe_delay = {
            let mut limiter = self.rate_limiter.lock().await;
            limiter.delay_until_available(1.0)
        };
        if let Some(delay) = maybe_delay {
            tokio::time::sleep(delay).await;
        }
    }

    pub(super) async fn mark_request_success(&self) {
        let mut health = self.connection_health.lock().await;
        if apply_request_success(&mut health, Instant::now()) {
            logging::info("Connection recovered");
        }
    }

    pub(super) async fn mark_request_failure(&self, reason: &str) {
        let mut health = self.connection_health.lock().await;
        apply_request_failure(&mut health, Instant::now());
        logging::warn(format!(
            "Connection degraded (failures={}): {}",
            health.consecutive_failures, reason
        ));
    }

    pub(super) async fn maybe_probe_recovery(&self) {
        let should_probe = {
            let mut health = self.connection_health.lock().await;
            mark_recovery_probe_if_due(&mut health, Instant::now())
        };
        if !should_probe {
            return;
        }
        let health_url = api_url(&self.base_url, "models");
        let probe = self.http_client.get(health_url).send().await;
        match probe {
            Ok(resp) if resp.status().is_success() => {
                self.mark_request_success().await;
                logging::info("Recovery probe succeeded");
            }
            Ok(resp) => {
                self.mark_request_failure(&format!("probe status={}", resp.status()))
                    .await;
            }
            Err(err) => {
                self.mark_request_failure(&format!("probe error={err}"))
                    .await;
            }
        }
    }

    pub(super) async fn send_with_retry<F>(&self, mut build: F) -> Result<reqwest::Response>
    where
        F: FnMut() -> reqwest::RequestBuilder,
    {
        let retry_cfg: LlmRetryConfig = self.retry.clone().into();
        let request_result = with_retry(
            &retry_cfg,
            || {
                let request = build();
                async move {
                    self.wait_for_rate_limit().await;
                    let response = request
                        .send()
                        .await
                        .map_err(|err| LlmError::from_reqwest(&err))?;
                    let status = response.status();
                    if status.is_success() {
                        return Ok(response);
                    }
                    let retryable = status.as_u16() == 429 || status.is_server_error();
                    if !retryable {
                        return Ok(response);
                    }
                    let retry_after = extract_retry_after(response.headers());
                    let body = bounded_error_text(response, ERROR_BODY_MAX_BYTES).await;
                    Err(LlmError::from_http_response_with_retry_after(
                        status.as_u16(),
                        &body,
                        retry_after,
                    ))
                }
            },
            Some(Box::new(|err, attempt, delay| {
                let (reason_label, human_reason) = retry_reason_label_and_human(err);
                logging::warn(format!(
                    "HTTP retry reason={} attempt={} delay={:.2}s",
                    reason_label,
                    attempt + 1,
                    delay.as_secs_f64(),
                ));
                crate::retry_status::start(attempt + 1, delay, human_reason);
            })),
        )
        .await;

        match request_result {
            Ok(response) => {
                crate::retry_status::succeeded();
                self.mark_request_success().await;
                Ok(response)
            }
            Err(err) => {
                let last = err.last_error.to_string();
                if err.attempts > 1 {
                    crate::retry_status::failed(last.clone());
                } else {
                    crate::retry_status::clear();
                }
                self.mark_request_failure(&last).await;
                self.maybe_probe_recovery().await;
                Err(anyhow::anyhow!(last))
            }
        }
    }
}
fn retry_reason_label_and_human(err: &LlmError) -> (&'static str, String) {
    match err {
        LlmError::RateLimited { retry_after, .. } => {
            let human = if let Some(after) = retry_after {
                format!("rate limited (Retry-After {}s)", after.as_secs())
            } else {
                "rate limited".to_string()
            };
            ("rate_limited", human)
        }
        LlmError::ServerError { status, .. } => ("server_error", format!("upstream {status}")),
        LlmError::NetworkError(_) => ("network_error", "network error".to_string()),
        LlmError::Timeout(_) => ("timeout", "timeout".to_string()),
        _ => ("other", "other".to_string()),
    }
}
