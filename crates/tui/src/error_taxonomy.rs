//! Shared error taxonomy across client, tools, runtime, and UI.
use std::fmt;

use crate::llm_client::LlmError;
use crate::tools::spec::ToolError;

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

/// Unified envelope used when crossing subsystem boundaries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorEnvelope {
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub recoverable: bool,
    pub code: String,
    pub message: String,
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
        }
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

impl From<LlmError> for ErrorEnvelope {
    fn from(value: LlmError) -> Self {
        match value {
            LlmError::RateLimited { message, .. } => Self::new(
                ErrorCategory::RateLimit,
                ErrorSeverity::Warning,
                true,
                "llm_rate_limited",
                message,
            ),
            LlmError::ServerError { status, message } => Self::new(
                ErrorCategory::Internal,
                ErrorSeverity::Error,
                true,
                format!("llm_server_{status}"),
                message,
            ),
            LlmError::NetworkError(message) => Self::new(
                ErrorCategory::Network,
                ErrorSeverity::Error,
                true,
                "llm_network_error",
                message,
            ),
            LlmError::Timeout(duration) => Self::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Warning,
                true,
                "llm_timeout",
                format!("Request timed out after {duration:?}"),
            ),
            LlmError::AuthenticationError(message) => Self::new(
                ErrorCategory::Authentication,
                ErrorSeverity::Critical,
                false,
                "llm_auth_error",
                message,
            ),
            LlmError::InvalidRequest { message, .. } => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_invalid_request",
                message,
            ),
            LlmError::ModelError(message) => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_model_error",
                message,
            ),
            LlmError::ContentPolicyError(message) => Self::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "llm_content_policy",
                message,
            ),
            LlmError::ParseError(message) => Self::new(
                ErrorCategory::Parse,
                ErrorSeverity::Error,
                false,
                "llm_parse_error",
                message,
            ),
            LlmError::ContextLengthError(message) => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_context_length",
                message,
            ),
            LlmError::Other(message) => Self::new(
                ErrorCategory::Internal,
                ErrorSeverity::Error,
                true,
                "llm_other",
                message,
            ),
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

impl From<ToolError> for ErrorEnvelope {
    fn from(value: ToolError) -> Self {
        match value {
            ToolError::InvalidInput { message } => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "tool_invalid_input",
                message,
            ),
            ToolError::MissingField { field } => Self::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "tool_missing_field",
                format!("Missing required field: {field}"),
            ),
            ToolError::PathEscape { path } => Self::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "tool_path_escape",
                format!("Path escapes workspace: {}", path.display()),
            ),
            ToolError::ExecutionFailed { message } => Self::new(
                ErrorCategory::Tool,
                ErrorSeverity::Error,
                true,
                "tool_execution_failed",
                message,
            ),
            ToolError::Timeout { seconds } => Self::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Warning,
                true,
                "tool_timeout",
                format!("Tool timed out after {seconds}s"),
            ),
            ToolError::NotAvailable { message } => Self::new(
                ErrorCategory::State,
                ErrorSeverity::Error,
                false,
                "tool_not_available",
                message,
            ),
            ToolError::PermissionDenied { message } => Self::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "tool_permission_denied",
                message,
            ),
        }
    }
}

/// Stream‑level error discriminated by origin.
///
/// Each variant maps to an `ErrorCategory` so the UI can render
/// stream‑specific icons or formatting. Wired into engine.rs at the three
/// stream guard sites (chunk timeout, max-bytes overflow, max-duration).
#[derive(Debug, Clone)]
pub enum StreamError {
    /// Stream stalled — no chunk received within the idle timeout.
    Stall { timeout_secs: u64 },
    /// Stream exceeded content size limit.
    Overflow { limit_bytes: usize },
    /// Stream exceeded wall‑clock duration limit.
    DurationLimit { limit_secs: u64 },
}

impl StreamError {
    /// Convert directly into an `ErrorEnvelope` for emission on the engine
    /// event channel. Stalls are warning-severity and recoverable; size and
    /// duration limits are errors (the user must restart the turn).
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
            Self::Stall { timeout_secs } => {
                write!(f, "Stream stalled after {timeout_secs}s idle")
            }
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

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── classify_error_message golden tests ────────────────────────────

    #[test]
    fn context_length_exact() {
        assert_eq!(
            classify_error_message("maximum context length exceeded"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn context_length_short() {
        assert_eq!(
            classify_error_message("context length is 128000 but messages used 250000"),
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
    fn prompt_too_long() {
        assert_eq!(
            classify_error_message("prompt is too long for this model"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn tokens_maximum_pattern() {
        assert_eq!(
            classify_error_message("requested 999999 tokens exceeds maximum of 65536"),
            ErrorCategory::InvalidInput
        );
    }

    #[test]
    fn context_window() {
        assert_eq!(
            classify_error_message("input exceeds the context window size"),
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
    fn too_many_requests() {
        assert_eq!(
            classify_error_message("too many requests, please try again later"),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn http_429() {
        assert_eq!(
            classify_error_message("HTTP 429: you have been rate limited"),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn quota_exceeded() {
        assert_eq!(
            classify_error_message("quota exceeded for this billing period"),
            ErrorCategory::RateLimit
        );
    }

    #[test]
    fn timeout_exact() {
        assert_eq!(
            classify_error_message("request timeout"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn timed_out() {
        assert_eq!(
            classify_error_message("connection timed out after 30s"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn authentication_unauthorized() {
        assert_eq!(
            classify_error_message("401 unauthorized"),
            ErrorCategory::Authentication
        );
    }

    #[test]
    fn authentication_api_key() {
        assert_eq!(
            classify_error_message("invalid api key provided"),
            ErrorCategory::Authentication
        );
    }

    #[test]
    fn authentication_auth_failed() {
        assert_eq!(
            classify_error_message("authentication failed for provider"),
            ErrorCategory::Authentication
        );
    }

    #[test]
    fn authorization_permission() {
        assert_eq!(
            classify_error_message("permission denied for this resource"),
            ErrorCategory::Authorization
        );
    }

    #[test]
    fn authorization_forbidden() {
        assert_eq!(
            classify_error_message("403 forbidden"),
            ErrorCategory::Authorization
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
    fn network_connection_refused() {
        assert_eq!(
            classify_error_message("connection refused"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn network_dns_failure() {
        assert_eq!(
            classify_error_message("dns resolution failed for api.example.com"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn network_502() {
        assert_eq!(
            classify_error_message("server returned 502 Bad Gateway"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn network_503() {
        assert_eq!(
            classify_error_message("503 Service Unavailable"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn network_504() {
        // "timeout" is checked before "504" — timeout wins
        assert_eq!(
            classify_error_message("504 Gateway Timeout"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn network_temporarily_unavailable() {
        assert_eq!(
            classify_error_message("service temporarily unavailable"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn parse_error() {
        assert_eq!(
            classify_error_message("failed to parse JSON response"),
            ErrorCategory::Parse
        );
    }

    #[test]
    fn parse_syntax() {
        assert_eq!(
            classify_error_message("syntax error in request body"),
            ErrorCategory::Parse
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
    fn state_unavailable() {
        // "temporarily unavailable" is checked in Network before "unavailable" in State
        assert_eq!(
            classify_error_message("resource temporarily unavailable"),
            ErrorCategory::Network
        );
    }

    #[test]
    fn tool_error() {
        // "not found" is checked before "tool" — State wins
        assert_eq!(
            classify_error_message("tool execution failed: /bin/bash not found"),
            ErrorCategory::State
        );
    }

    #[test]
    fn tool_in_message() {
        assert_eq!(
            classify_error_message("a tool returned an error code 1"),
            ErrorCategory::Tool
        );
    }

    #[test]
    fn internal_fallback() {
        assert_eq!(
            classify_error_message("something completely unexpected happened"),
            ErrorCategory::Internal
        );
    }

    // ── Boundary / tricky cases ────────────────────────────────────────

    #[test]
    fn empty_message() {
        assert_eq!(classify_error_message(""), ErrorCategory::Internal);
    }

    #[test]
    fn whitespace_only() {
        assert_eq!(classify_error_message("   "), ErrorCategory::Internal);
    }

    #[test]
    fn auth_before_timeout() {
        // "timeout" is checked before "auth" — timeout wins
        assert_eq!(
            classify_error_message("auth error: connection timed out"),
            ErrorCategory::Timeout
        );
    }

    #[test]
    fn network_exact_502_only() {
        assert_eq!(classify_error_message("502"), ErrorCategory::Network);
    }

    #[test]
    fn network_exact_503_only() {
        assert_eq!(classify_error_message("503"), ErrorCategory::Network);
    }

    #[test]
    fn status_502_in_path_not_network() {
        // "502" inside a larger word should NOT match network
        assert_eq!(
            classify_error_message("error code ERR5021: bad input"),
            ErrorCategory::Internal
        );
    }

    #[test]
    fn quota_not_rate_limit_when_in_context() {
        // "quota" only matches rate limit; not all uses of "quota"
        assert_eq!(
            classify_error_message("quota exceeded"),
            ErrorCategory::RateLimit
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
    fn unicode_ellipsis_still_classified() {
        assert_eq!(
            classify_error_message("请求超时…请重试"), // "timeout" not present in CJK
            ErrorCategory::Internal // falls through — no CJK keywords
        );
    }

    // ── ErrorEnvelope construction helpers ─────────────────────────────

    #[test]
    fn envelope_transient_is_recoverable_warning() {
        let e = ErrorEnvelope::transient("oops");
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn envelope_fatal_is_non_recoverable_error() {
        let e = ErrorEnvelope::fatal("boom");
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(!e.recoverable);
    }

    #[test]
    fn envelope_fatal_auth_is_critical() {
        let e = ErrorEnvelope::fatal_auth("bad key");
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(e.severity, ErrorSeverity::Critical);
        assert!(!e.recoverable);
    }

    #[test]
    fn envelope_context_overflow_is_recoverable() {
        let e = ErrorEnvelope::context_overflow("too big");
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(e.recoverable);
    }

    #[test]
    fn envelope_network_is_recoverable_warning() {
        let e = ErrorEnvelope::network("dns fail");
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    // ── classify() integration ─────────────────────────────────────────

    #[test]
    fn classify_recoverable_internal_is_warning() {
        let e = ErrorEnvelope::classify("unknown hiccup", true);
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn classify_non_recoverable_internal_is_error() {
        let e = ErrorEnvelope::classify("unknown hiccup", false);
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(!e.recoverable);
    }

    #[test]
    fn classify_network_is_warning() {
        let e = ErrorEnvelope::classify("connection reset by peer", true);
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.severity, ErrorSeverity::Warning);
    }

    #[test]
    fn classify_rate_limit_is_warning() {
        let e = ErrorEnvelope::classify("rate limit hit", true);
        assert_eq!(e.category, ErrorCategory::RateLimit);
        assert_eq!(e.severity, ErrorSeverity::Warning);
    }

    #[test]
    fn classify_auth_is_critical() {
        let e = ErrorEnvelope::classify("401 unauthorized", false);
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(e.severity, ErrorSeverity::Critical);
    }

    // ── LlmError → ErrorEnvelope ───────────────────────────────────────

    #[test]
    fn llm_rate_limited_is_recoverable_warning() {
        let e: ErrorEnvelope = crate::llm_client::LlmError::RateLimited {
            message: "slow down".into(),
            retry_after: None,
        }
        .into();
        assert_eq!(e.category, ErrorCategory::RateLimit);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_network_error() {
        let e: ErrorEnvelope =
            crate::llm_client::LlmError::NetworkError("connection lost".into()).into();
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_timeout() {
        let e: ErrorEnvelope =
            crate::llm_client::LlmError::Timeout(std::time::Duration::from_secs(30)).into();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_auth_is_critical() {
        let e: ErrorEnvelope =
            crate::llm_client::LlmError::AuthenticationError("bad api key".into()).into();
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(e.severity, ErrorSeverity::Critical);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_invalid_request_is_non_recoverable() {
        let e: ErrorEnvelope = crate::llm_client::LlmError::InvalidRequest {
            message: "model not found".into(),
            status: 400,
        }
        .into();
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_content_policy() {
        let e: ErrorEnvelope =
            crate::llm_client::LlmError::ContentPolicyError("blocked".into()).into();
        assert_eq!(e.category, ErrorCategory::Authorization);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_context_length() {
        let e: ErrorEnvelope =
            crate::llm_client::LlmError::ContextLengthError("too long".into()).into();
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(!e.recoverable);
    }

    // ── ToolError → ErrorEnvelope ──────────────────────────────────────

    #[test]
    fn tool_invalid_input() {
        let e: ErrorEnvelope = crate::tools::spec::ToolError::InvalidInput {
            message: "bad args".into(),
        }
        .into();
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(!e.recoverable);
    }

    #[test]
    fn tool_path_escape_is_authorization() {
        let e: ErrorEnvelope = crate::tools::spec::ToolError::PathEscape {
            path: std::path::PathBuf::from("/etc/passwd"),
        }
        .into();
        assert_eq!(e.category, ErrorCategory::Authorization);
    }

    #[test]
    fn tool_timeout_is_recoverable_warning() {
        let e: ErrorEnvelope = crate::tools::spec::ToolError::Timeout { seconds: 30 }.into();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn tool_execution_failed_is_recoverable() {
        let e: ErrorEnvelope = crate::tools::spec::ToolError::ExecutionFailed {
            message: "exit code 1".into(),
        }
        .into();
        assert_eq!(e.category, ErrorCategory::Tool);
        assert!(e.recoverable);
    }

    // ── StreamError ────────────────────────────────────────────────────

    #[test]
    fn stream_stall_is_recoverable_warning() {
        let e = StreamError::Stall { timeout_secs: 60 }.into_envelope();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn stream_overflow_is_recoverable_error() {
        let e = StreamError::Overflow {
            limit_bytes: 1_000_000,
        }
        .into_envelope();
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(e.recoverable);
    }

    #[test]
    fn stream_duration_limit() {
        let e = StreamError::DurationLimit { limit_secs: 300 }.into_envelope();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(e.recoverable);
    }

    // ── Display impls ──────────────────────────────────────────────────

    #[test]
    fn error_category_display() {
        assert_eq!(ErrorCategory::Network.to_string(), "network");
        assert_eq!(ErrorCategory::Authentication.to_string(), "authentication");
        assert_eq!(ErrorCategory::RateLimit.to_string(), "rate_limit");
        assert_eq!(ErrorCategory::Internal.to_string(), "internal");
    }

    #[test]
    fn error_severity_display() {
        assert_eq!(ErrorSeverity::Info.to_string(), "info");
        assert_eq!(ErrorSeverity::Warning.to_string(), "warning");
        assert_eq!(ErrorSeverity::Error.to_string(), "error");
        assert_eq!(ErrorSeverity::Critical.to_string(), "critical");
    }

    #[test]
    fn error_envelope_display() {
        let e = ErrorEnvelope::network("connection lost");
        let s = e.to_string();
        assert!(s.contains("network"));
        assert!(s.contains("connection lost"));
    }

    #[test]
    fn stream_error_display() {
        let s = StreamError::Stall { timeout_secs: 30 }.to_string();
        assert!(s.contains("stalled"));
        assert!(s.contains("30"));
    }
}
