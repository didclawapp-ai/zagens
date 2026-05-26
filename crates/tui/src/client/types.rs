use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{ApiProvider, RetryPolicy};
use tokio::sync::Mutex as AsyncMutex;

// === Types ===

/// Model descriptor returned by the provider's `/v1/models` endpoint.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AvailableModel {
    pub id: String,
    pub owned_by: Option<String>,
    pub created: Option<u64>,
}

/// Client for DeepSeek's OpenAI-compatible APIs.
#[must_use]
pub struct DeepSeekClient {
    pub(super) http_client: reqwest::Client,
    pub(super) api_key: String,
    pub(super) base_url: String,
    pub(super) api_provider: ApiProvider,
    pub(super) retry: RetryPolicy,
    pub(super) default_model: String,
    pub(super) connection_health: Arc<AsyncMutex<ConnectionHealth>>,
    pub(super) rate_limiter: Arc<AsyncMutex<TokenBucket>>,
}

const CONNECTION_FAILURE_THRESHOLD: u32 = 2;
pub(super) const RECOVERY_PROBE_COOLDOWN: Duration = Duration::from_secs(15);

const DEFAULT_CLIENT_RATE_LIMIT_RPS: f64 = 8.0;
const DEFAULT_CLIENT_RATE_LIMIT_BURST: f64 = 16.0;
const ALLOW_INSECURE_HTTP_ENV: &str = "DEEPSEEK_ALLOW_INSECURE_HTTP";

pub(super) const SSE_BACKPRESSURE_HIGH_WATERMARK: usize = 8 * 1024 * 1024; // 8 MB
pub(super) const SSE_BACKPRESSURE_SLEEP_MS: u64 = 10;
pub(super) const SSE_MAX_LINES_PER_CHUNK: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConnectionState {
    Healthy,
    Degraded,
    Recovering,
}

#[derive(Debug)]
pub(super) struct ConnectionHealth {
    pub(super) state: ConnectionState,
    pub(super) consecutive_failures: u32,
    pub(super) last_failure: Option<Instant>,
    pub(super) last_success: Option<Instant>,
    pub(super) last_probe: Option<Instant>,
}

impl Default for ConnectionHealth {
    fn default() -> Self {
        Self {
            state: ConnectionState::Healthy,
            consecutive_failures: 0,
            last_failure: None,
            last_success: None,
            last_probe: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct TokenBucket {
    pub(super) enabled: bool,
    pub(super) capacity: f64,
    pub(super) tokens: f64,
    pub(super) refill_per_sec: f64,
    pub(super) last_refill: Instant,
}

impl TokenBucket {
    pub(super) fn from_env() -> Self {
        let rps = std::env::var("DEEPSEEK_RATE_LIMIT_RPS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(DEFAULT_CLIENT_RATE_LIMIT_RPS)
            .max(0.0);
        let burst = std::env::var("DEEPSEEK_RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(DEFAULT_CLIENT_RATE_LIMIT_BURST)
            .max(1.0);
        let enabled = rps > 0.0;
        Self {
            enabled,
            capacity: burst,
            tokens: burst,
            refill_per_sec: rps,
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self, now: Instant) {
        if !self.enabled {
            return;
        }
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
    }

    pub(super) fn delay_until_available(&mut self, tokens: f64) -> Option<Duration> {
        if !self.enabled {
            return None;
        }
        let now = Instant::now();
        self.refill(now);
        if self.tokens >= tokens {
            self.tokens -= tokens;
            return None;
        }
        let needed = tokens - self.tokens;
        self.tokens = 0.0;
        if self.refill_per_sec <= 0.0 {
            return Some(Duration::from_secs(1));
        }
        Some(Duration::from_secs_f64(needed / self.refill_per_sec))
    }
}

pub(super) fn apply_request_success(health: &mut ConnectionHealth, now: Instant) -> bool {
    let recovered = health.state != ConnectionState::Healthy;
    health.state = ConnectionState::Healthy;
    health.consecutive_failures = 0;
    health.last_success = Some(now);
    recovered
}

pub(super) fn apply_request_failure(health: &mut ConnectionHealth, now: Instant) {
    health.consecutive_failures = health.consecutive_failures.saturating_add(1);
    health.last_failure = Some(now);
    if health.consecutive_failures >= CONNECTION_FAILURE_THRESHOLD {
        health.state = ConnectionState::Degraded;
    }
}

pub(super) fn mark_recovery_probe_if_due(health: &mut ConnectionHealth, now: Instant) -> bool {
    if health.state == ConnectionState::Healthy {
        return false;
    }
    if health
        .last_probe
        .is_some_and(|last| now.duration_since(last) < RECOVERY_PROBE_COOLDOWN)
    {
        return false;
    }
    health.last_probe = Some(now);
    health.state = ConnectionState::Recovering;
    true
}

fn buffer_pool() -> &'static StdMutex<Vec<Vec<u8>>> {
    static POOL: OnceLock<StdMutex<Vec<Vec<u8>>>> = OnceLock::new();
    POOL.get_or_init(|| StdMutex::new(Vec::new()))
}

pub(super) fn acquire_stream_buffer() -> Vec<u8> {
    if let Ok(mut pool) = buffer_pool().lock() {
        pool.pop().unwrap_or_else(|| Vec::with_capacity(8192))
    } else {
        Vec::with_capacity(8192)
    }
}

pub(super) fn release_stream_buffer(mut buf: Vec<u8>) {
    buf.clear();
    if buf.capacity() > 256 * 1024 {
        buf.shrink_to(256 * 1024);
    }
    if let Ok(mut pool) = buffer_pool().lock()
        && pool.len() < 8
    {
        pool.push(buf);
    }
}

impl Clone for DeepSeekClient {
    fn clone(&self) -> Self {
        Self {
            http_client: self.http_client.clone(),
            api_key: self.api_key.clone(),
            base_url: self.base_url.clone(),
            api_provider: self.api_provider,
            retry: self.retry.clone(),
            default_model: self.default_model.clone(),
            connection_health: self.connection_health.clone(),
            rate_limiter: self.rate_limiter.clone(),
        }
    }
}

