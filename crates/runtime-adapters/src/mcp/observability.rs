//! MCP call observability: in-memory recent-call ring buffer + helpers for tracing.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_RECENT_CALLS: usize = 64;

/// One completed MCP RPC or tool invocation (sanitized for UI).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct McpCallRecord {
    pub timestamp_ms: u128,
    pub server: String,
    pub method: String,
    pub duration_ms: u64,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub result_bytes: usize,
}

struct CallLog {
    entries: VecDeque<McpCallRecord>,
}

impl CallLog {
    fn push(&mut self, record: McpCallRecord) {
        if self.entries.len() >= MAX_RECENT_CALLS {
            self.entries.pop_front();
        }
        self.entries.push_back(record);
    }
}

static CALL_LOG: OnceLock<Mutex<CallLog>> = OnceLock::new();

fn call_log() -> &'static Mutex<CallLog> {
    CALL_LOG.get_or_init(|| {
        Mutex::new(CallLog {
            entries: VecDeque::new(),
        })
    })
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// Record a completed MCP operation for the desktop diagnostics panel.
pub fn record_mcp_call(
    server: impl Into<String>,
    method: impl Into<String>,
    duration_ms: u64,
    success: bool,
    error: Option<String>,
    result_bytes: usize,
) {
    let record = McpCallRecord {
        timestamp_ms: now_ms(),
        server: server.into(),
        method: method.into(),
        duration_ms,
        success,
        error: error.map(|e| super::diagnostics::redact_body_preview(&e)),
        result_bytes,
    };
    if let Ok(mut log) = call_log().lock() {
        log.push(record);
    }
}

/// Recent MCP calls, newest last (up to [`MAX_RECENT_CALLS`]).
#[must_use]
pub fn recent_mcp_calls() -> Vec<McpCallRecord> {
    call_log()
        .lock()
        .ok()
        .map(|log| log.entries.iter().cloned().collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_calls_ring_buffer() {
        for i in 0..70 {
            record_mcp_call("srv", format!("tool_{i}"), 1, true, None, 10);
        }
        let calls = recent_mcp_calls();
        assert!(calls.len() <= MAX_RECENT_CALLS);
        assert_eq!(calls.last().map(|c| c.method.as_str()), Some("tool_69"));
    }
}
