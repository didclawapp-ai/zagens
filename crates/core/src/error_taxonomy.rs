//! Shared error taxonomy across client, tools, runtime, and UI.
use std::fmt;

/// Broad category for typed error handling and policy decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Network,
    Authentication,
    Authorization,
    RateLimit,
    Timeout,
    InvalidInput,
    Parse,
    Tool,
    State,
    Internal,
}

/// Severity hint for UI and logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Stream/turn retry policy derived from [`ErrorCategory`] (A3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorRetryPolicy {
    /// Transient network, timeout, rate limit, or generic internal hiccup.
    NetworkRetryable,
    /// Business, auth, validation, or tool errors — do not burn retry budget.
    NotRetryable,
}

/// Unified envelope used when crossing subsystem boundaries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorEnvelope {
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub recoverable: bool,
    pub code: String,
    pub message: String,
    /// Actionable user hint (HTTP `error.hint`, TUI status line).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Network => "network",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::RateLimit => "rate_limit",
            Self::Timeout => "timeout",
            Self::InvalidInput => "invalid_input",
            Self::Parse => "parse",
            Self::Tool => "tool",
            Self::State => "state",
            Self::Internal => "internal",
        };
        f.write_str(label)
    }
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Critical => "critical",
        };
        f.write_str(label)
    }
}

impl fmt::Display for ErrorEnvelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.severity, self.code, self.message)
    }
}

impl std::error::Error for ErrorEnvelope {}

impl ErrorCategory {
    /// A3.2 — whether transparent/outer stream retries may retry this category.
    #[must_use]
    pub fn retry_policy(self) -> ErrorRetryPolicy {
        if is_category_network_retryable(self) {
            ErrorRetryPolicy::NetworkRetryable
        } else {
            ErrorRetryPolicy::NotRetryable
        }
    }
}

/// User-facing hint for an error category (A3.4 — distinct from raw `message`).
#[must_use]
pub fn user_hint_for_category(category: ErrorCategory) -> &'static str {
    match category {
        ErrorCategory::Network => {
            "Check your network or proxy, then retry the message."
        }
        ErrorCategory::Timeout => {
            "The request timed out; retry or reduce context with /compact."
        }
        ErrorCategory::RateLimit => {
            "Wait briefly and retry, or switch to a lighter model."
        }
        ErrorCategory::InvalidInput => {
            "Fix model/thinking settings or compact context — this request cannot be retried automatically."
        }
        ErrorCategory::Authentication => {
            "Set a valid API key in DEEPSEEK_API_KEY or ~/.deepseek/config.toml."
        }
        ErrorCategory::Authorization => {
            "This action is not allowed in the current trust or approval mode."
        }
        ErrorCategory::Parse => "The response could not be parsed; retry once or report if it persists.",
        ErrorCategory::Tool => "Review the tool output in the transcript and adjust the request.",
        ErrorCategory::State => "The thread or resource may have ended; refresh or start a new turn.",
        ErrorCategory::Internal => "Retry the message; if it persists, check logs or restart the runtime.",
    }
}

#[must_use]
pub fn is_category_network_retryable(category: ErrorCategory) -> bool {
    matches!(
        category,
        ErrorCategory::Network | ErrorCategory::Timeout | ErrorCategory::RateLimit
    ) || category == ErrorCategory::Internal
}

impl ErrorEnvelope {
    #[must_use]
    pub fn new(
        category: ErrorCategory,
        severity: ErrorSeverity,
        recoverable: bool,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            severity,
            recoverable,
            code: code.into(),
            message: message.into(),
            hint: Some(user_hint_for_category(category).to_string()),
        }
    }

    /// Whether stream/turn outer retries should consume budget for this envelope (A3.2).
    #[must_use]
    pub fn is_network_retryable(&self) -> bool {
        is_category_network_retryable(self.category)
    }

    /// JSON body for HTTP `ApiError` and desktop clients (`error` object).
    #[must_use]
    pub fn to_wire_error_body(&self, http_status: u16) -> serde_json::Value {
        let category = self.category.to_string();
        serde_json::json!({
            "error": {
                "message": self.message,
                "status": http_status,
                "category": category,
                "class": category,
                "code": self.code,
                "recoverable": self.recoverable,
                "retryable": self.is_network_retryable(),
                "retry_policy": self.category.retry_policy().as_str(),
                "severity": self.severity.to_string(),
                "hint": self.hint,
            }
        })
    }

    /// Recoverable internal error — stream stalls, transient retries, generic
    /// engine errors that the user can resolve by retrying. Severity is
    /// `Warning` so the UI surfaces it in amber rather than red.
    #[must_use]
    pub fn transient(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::Internal,
            ErrorSeverity::Warning,
            true,
            "transient",
            message,
        )
    }

    /// Non-recoverable internal error — missing client, spawn failure, etc.
    /// Flips the session into offline mode.
    #[must_use]
    pub fn fatal(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::Internal,
            ErrorSeverity::Error,
            false,
            "fatal",
            message,
        )
    }

    /// Authentication failure — fatal and blocks the session.
    #[must_use]
    pub fn fatal_auth(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::Authentication,
            ErrorSeverity::Critical,
            false,
            "auth_fatal",
            message,
        )
    }

    /// Context length / overflow — invalid input, recoverable via /compact.
    #[must_use]
    pub fn context_overflow(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::InvalidInput,
            ErrorSeverity::Error,
            true,
            "context_overflow",
            message,
        )
    }

    /// Recoverable network / transport hiccup.
    #[must_use]
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::Network,
            ErrorSeverity::Warning,
            true,
            "network_transient",
            message,
        )
    }

    /// Tool execution failure.
    #[must_use]
    pub fn tool(message: impl Into<String>) -> Self {
        Self::new(
            ErrorCategory::Tool,
            ErrorSeverity::Error,
            true,
            "tool_failed",
            message,
        )
    }

}

/// Stream-specific errors for the turn loop (chunk timeout, overflow, duration).
#[derive(Debug, Clone)]
pub enum StreamError {
    Stall { timeout_secs: u64 },
    Overflow { limit_bytes: usize },
    DurationLimit { limit_secs: u64 },
}

impl StreamError {
    #[must_use]
    pub fn into_envelope(self) -> ErrorEnvelope {
        match self {
            Self::Stall { timeout_secs } => ErrorEnvelope::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Warning,
                true,
                "stream_stall",
                format!("Stream stalled: no data received for {timeout_secs}s, closing stream"),
            ),
            Self::Overflow { limit_bytes } => ErrorEnvelope::new(
                ErrorCategory::Internal,
                ErrorSeverity::Error,
                true,
                "stream_overflow",
                format!("Stream exceeded maximum content size of {limit_bytes} bytes, closing"),
            ),
            Self::DurationLimit { limit_secs } => ErrorEnvelope::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Error,
                true,
                "stream_duration_limit",
                format!("Stream exceeded maximum duration of {limit_secs}s, closing"),
            ),
        }
    }
}

impl fmt::Display for StreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stall { timeout_secs } => write!(f, "Stream stalled after {timeout_secs}s idle"),
            Self::Overflow { limit_bytes } => {
                write!(f, "Stream exceeded {limit_bytes} bytes limit")
            }
            Self::DurationLimit { limit_secs } => {
                write!(f, "Stream exceeded {limit_secs}s duration limit")
            }
        }
    }
}

impl std::error::Error for StreamError {}

impl ErrorEnvelope {
    /// Build an envelope by classifying a raw error message string. Used at
    /// boundaries where the underlying error type was already stringified.
    #[must_use]
    pub fn classify(message: impl Into<String>, recoverable: bool) -> Self {
        let message = message.into();
        let category = classify_error_message(&message);
        let severity = match category {
            ErrorCategory::Authentication => ErrorSeverity::Critical,
            ErrorCategory::RateLimit | ErrorCategory::Timeout | ErrorCategory::Network => {
                ErrorSeverity::Warning
            }
            ErrorCategory::InvalidInput | ErrorCategory::Authorization | ErrorCategory::Parse => {
                ErrorSeverity::Error
            }
            ErrorCategory::Tool | ErrorCategory::State | ErrorCategory::Internal => {
                if recoverable {
                    ErrorSeverity::Warning
                } else {
                    ErrorSeverity::Error
                }
            }
        };
        Self::new(
            category,
            severity,
            recoverable,
            category.to_string(),
            message,
        )
    }
}

impl ErrorRetryPolicy {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NetworkRetryable => "network_retryable",
            Self::NotRetryable => "not_retryable",
        }
    }
}

/// Classify an error message string into an ErrorCategory.
///
/// Uses heuristic keyword matching on the lowercased message.
/// This is a replacement for ad-hoc string matching in callers.
#[must_use]
pub fn classify_error_message(message: &str) -> ErrorCategory {
    let lower = message.to_lowercase();

    if lower.contains("maximum context length")
        || lower.contains("context length")
        || lower.contains("context_length")
        || lower.contains("prompt is too long")
        || (lower.contains("requested") && lower.contains("tokens") && lower.contains("maximum"))
        || lower.contains("context window")
        || lower.contains("reasoning_content")
        || lower.contains("reasoning_effort")
        || lower.contains("thinking mode")
        || lower.contains("thinking.type")
    {
        return ErrorCategory::InvalidInput;
    }
    if lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("429")
        || lower.contains("quota")
    {
        return ErrorCategory::RateLimit;
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return ErrorCategory::Timeout;
    }
    if lower.contains("auth") || lower.contains("unauthorized") || lower.contains("api key") {
        return ErrorCategory::Authentication;
    }
    if lower.contains("permission") || lower.contains("forbidden") || lower.contains("denied") {
        return ErrorCategory::Authorization;
    }
    if lower.contains("network")
        || lower.contains("connection")
        || lower.contains("dns")
        || lower.contains("temporarily unavailable")
        || lower.contains(" 502 ")
        || lower.contains(" 503 ")
        || lower.contains(" 504 ")
        || lower.starts_with("502 ")
        || lower.starts_with("503 ")
        || lower.starts_with("504 ")
        || lower.ends_with(" 502")
        || lower.ends_with(" 503")
        || lower.ends_with(" 504")
        || lower == "502"
        || lower == "503"
        || lower == "504"
    {
        return ErrorCategory::Network;
    }
    if lower.contains("decision must")
        || lower.contains("expected rfc 3339")
        || lower.starts_with("invalid ")
        || lower.contains("invalid request")
    {
        return ErrorCategory::InvalidInput;
    }
    if lower.contains("parse") || lower.contains("syntax") || lower.contains("malformed") {
        return ErrorCategory::Parse;
    }
    if lower.contains("not found")
        || lower.contains("unavailable")
        || lower.contains("not available")
    {
        return ErrorCategory::State;
    }
    if lower.contains("tool") {
        return ErrorCategory::Tool;
    }

    ErrorCategory::Internal
}

/// Whether a stream failure should consume transparent / outer retry budget (A3.3).
///
/// Business-invalid requests (thinking constraints, auth) must not be silently
/// re-issued; transient network/proxy issues may be.
#[must_use]
pub fn is_stream_failure_retryable(message: &str) -> bool {
    is_category_network_retryable(classify_error_message(message))
}

impl From<deepseek_tools::ToolError> for ErrorEnvelope {
    fn from(value: deepseek_tools::ToolError) -> Self {
        match value {
            deepseek_tools::ToolError::InvalidInput { message } => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "tool_invalid_input",
                message,
            ),
            deepseek_tools::ToolError::MissingField { field } => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "tool_missing_field",
                format!("Missing required field: {field}"),
            ),
            deepseek_tools::ToolError::PathEscape { path } => Self::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "tool_path_escape",
                format!("Path escapes workspace: {}", path.display()),
            ),
            deepseek_tools::ToolError::ExecutionFailed { message } => Self::new(
                ErrorCategory::Tool,
                ErrorSeverity::Error,
                true,
                "tool_execution_failed",
                message,
            ),
            deepseek_tools::ToolError::Timeout { seconds } => Self::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Warning,
                true,
                "tool_timeout",
                format!("Tool timed out after {seconds}s"),
            ),
            deepseek_tools::ToolError::NotAvailable { message } => Self::new(
                ErrorCategory::State,
                ErrorSeverity::Error,
                false,
                "tool_not_available",
                message,
            ),
            deepseek_tools::ToolError::PermissionDenied { message } => Self::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "tool_permission_denied",
                message,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deepseek_tools::ToolError;

    // ── classify_error_message golden tests (A3 / R-007) ─────────────────

    #[test]
    fn context_length_exact() {
        assert_eq!(
            classify_error_message("maximum context length exceeded"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn context_length_underscore() {
        assert_eq!(
            classify_error_message("context_length_error: too many tokens"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn context_length_variants() {
        assert_eq!(
            classify_error_message("context length is 128000 but messages used 250000"),
            ErrorCategory::InvalidInput
        );
        assert_eq!(
            classify_error_message("prompt is too long for this model"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn rate_limit_variants() {
        assert_eq!(
            classify_error_message("too many requests, please try again later"),
            ErrorCategory::RateLimit
        );
        assert_eq!(
            classify_error_message("HTTP 429: you have been rate limited"),
            ErrorCategory::RateLimit
        );
        assert_eq!(
            classify_error_message("quota exceeded"),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn timeout_wins_over_auth_substring() {
        assert_eq!(
            classify_error_message("auth error: connection timed out"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn network_gateway_codes() {
        assert_eq!(
            classify_error_message("server returned 502 Bad Gateway"),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error_message("503 Service Unavailable"),
            ErrorCategory::Network
        );
        assert_eq!(
            classify_error_message("service temporarily unavailable"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn status_502_embedded_in_token_not_network() {
        assert_eq!(
            classify_error_message("error code ERR5021: bad input"),
            ErrorCategory::Internal
        );
    }

    #[test]
    fn tool_not_found_is_state_not_tool() {
        assert_eq!(
            classify_error_message("tool execution failed: /bin/bash not found"),
            ErrorCategory::State
        );
    }

    #[test]
    fn envelope_helpers() {
        let t = ErrorEnvelope::transient("oops");
        assert_eq!(t.category, ErrorCategory::Internal);
        assert!(t.recoverable);
        let f = ErrorEnvelope::fatal_auth("bad key");
        assert_eq!(f.severity, ErrorSeverity::Critical);
        assert!(!f.recoverable);
    }

    #[test]
    fn display_labels() {
        assert_eq!(ErrorCategory::RateLimit.to_string(), "rate_limit");
        assert_eq!(ErrorSeverity::Critical.to_string(), "critical");
        assert!(ErrorEnvelope::network("lost").to_string().contains("lost"));
    }

    #[test]
    fn stream_overflow_envelope() {
        let e = StreamError::Overflow {
            limit_bytes: 1_000_000,
        }
        .into_envelope();
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Error);
    }

    #[test]
    fn reasoning_content_constraint_is_invalid_input_not_network() {
        assert_eq!(
            classify_error_message(
                "400 Bad Request: reasoning_content is required for tool calls in thinking mode"
            ),
            ErrorCategory::InvalidInput
        );
        assert_eq!(
            classify_error_message("connection reset by peer"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn reasoning_effort_invalid() {
        assert_eq!(
            classify_error_message("invalid reasoning_effort: maxx"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn thinking_mode_constraint() {
        assert_eq!(
            classify_error_message("thinking mode does not support this parameter"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn rate_limit_exact() {
        assert_eq!(
            classify_error_message("rate limit exceeded"),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn timeout_before_network_status_codes() {
        assert_eq!(
            classify_error_message("504 Gateway Timeout"),
            ErrorCategory::Timeout
        );
        assert_eq!(classify_error_message("502"), ErrorCategory::Network);
    }

    #[test]
    fn network_disconnect() {
        assert_eq!(
            classify_error_message("connection reset by peer"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn auth_api_key() {
        assert_eq!(
            classify_error_message("invalid api key provided"),
            ErrorCategory::Authentication
        );
    }

    #[test]
    fn authorization_denied() {
        assert_eq!(
            classify_error_message("access denied by policy"),
            ErrorCategory::Authorization
        );
    }

    #[test]
    fn parse_malformed() {
        assert_eq!(
            classify_error_message("malformed response from server"),
            ErrorCategory::Parse
        );
    }

    #[test]
    fn state_not_found() {
        assert_eq!(
            classify_error_message("thread not found"),
            ErrorCategory::State
        );
    }

    #[test]
    fn tool_without_not_found_substring() {
        assert_eq!(
            classify_error_message("a tool returned an error code 1"),
            ErrorCategory::Tool
        );
    }

    #[test]
    fn empty_and_whitespace_fallback_internal() {
        assert_eq!(classify_error_message(""), ErrorCategory::Internal);
        assert_eq!(classify_error_message("   "), ErrorCategory::Internal);
    }

    #[test]
    fn internal_fallback() {
        assert_eq!(
            classify_error_message("something completely unexpected happened"),
            ErrorCategory::Internal
        );
    }

    #[test]
    fn capitalization_irrelevant() {
        assert_eq!(
            classify_error_message("NETWORK ERROR: Connection REFUSED"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn classify_recoverable_internal_is_warning() {
        let e = ErrorEnvelope::classify("unknown hiccup", true);
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn classify_auth_is_critical() {
        let e = ErrorEnvelope::classify("401 unauthorized", false);
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(e.severity, ErrorSeverity::Critical);
    }

    #[test]
    fn stream_stall_is_recoverable_warning() {
        let e = StreamError::Stall { timeout_secs: 60 }.into_envelope();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn tool_timeout_is_recoverable_warning() {
        let e: ErrorEnvelope = ToolError::Timeout { seconds: 30 }.into();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn tool_path_escape_is_authorization() {
        let e: ErrorEnvelope = ToolError::PathEscape {
            path: std::path::PathBuf::from("/etc/passwd"),
        }
        .into();
        assert_eq!(e.category, ErrorCategory::Authorization);
    }

    #[test]
    fn stream_retry_policy_network_vs_invalid_input() {
        assert!(is_stream_failure_retryable("connection reset by peer"));
        assert!(is_stream_failure_retryable("502 Bad Gateway"));
        assert!(!is_stream_failure_retryable(
            "Missing reasoning_content on assistant tool message"
        ));
        assert!(!is_stream_failure_retryable("401 unauthorized"));
    }

    #[test]
    fn user_hints_differ_for_network_vs_invalid_input() {
        let net = user_hint_for_category(ErrorCategory::Network);
        let invalid = user_hint_for_category(ErrorCategory::InvalidInput);
        assert_ne!(net, invalid);
        assert!(net.contains("network") || net.contains("proxy"));
        assert!(invalid.contains("compact") || invalid.contains("thinking"));
    }

    #[test]
    fn wire_error_body_includes_hint_class_and_retry_policy() {
        let e = ErrorEnvelope::classify("connection reset by peer", true);
        let body = e.to_wire_error_body(503);
        let err = body.get("error").expect("error object");
        assert_eq!(err["category"], "network");
        assert_eq!(err["class"], "network");
        assert_eq!(err["retry_policy"], "network_retryable");
        assert_eq!(err["retryable"], true);
        assert!(err.get("hint").and_then(|h| h.as_str()).is_some());
    }

    #[test]
    fn invalid_input_wire_body_not_retryable() {
        let e = ErrorEnvelope::classify(
            "reasoning_content is required for tool calls in thinking mode",
            false,
        );
        let err = e.to_wire_error_body(400).get("error").cloned().unwrap();
        assert_eq!(err["category"], "invalid_input");
        assert_eq!(err["retry_policy"], "not_retryable");
        assert_eq!(err["retryable"], false);
    }

    #[test]
    fn api_validation_messages_are_invalid_input() {
        assert_eq!(
            classify_error_message("decision must be 'approve' or 'deny'"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn category_retry_policy_labels() {
        assert_eq!(
            ErrorCategory::Network.retry_policy(),
            ErrorRetryPolicy::NetworkRetryable
        );
        assert_eq!(
            ErrorCategory::InvalidInput.retry_policy(),
            ErrorRetryPolicy::NotRetryable
        );
    }
}
