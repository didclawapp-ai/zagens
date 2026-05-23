//! Shared error taxonomy — core types in `deepseek-core`; TUI `From` + stream guards.

use std::fmt;

use crate::llm_client::LlmError;
use deepseek_tools::ToolError;

pub use deepseek_core::error_taxonomy::{
    classify_error_message, ErrorCategory, ErrorEnvelope, ErrorSeverity, StreamError,
};

#[must_use]
pub fn envelope_from_llm_error(value: LlmError) -> ErrorEnvelope {
    match value {
        LlmError::RateLimited { message, .. } => ErrorEnvelope::new(
                ErrorCategory::RateLimit,
                ErrorSeverity::Warning,
                true,
                "llm_rate_limited",
                message,
            ),
        LlmError::ServerError { status, message } => ErrorEnvelope::new(
                ErrorCategory::Internal,
                ErrorSeverity::Error,
                true,
                format!("llm_server_{status}"),
                message,
            ),
        LlmError::NetworkError(message) => ErrorEnvelope::new(
                ErrorCategory::Network,
                ErrorSeverity::Error,
                true,
                "llm_network_error",
                message,
            ),
        LlmError::Timeout(duration) => ErrorEnvelope::new(
                ErrorCategory::Timeout,
                ErrorSeverity::Warning,
                true,
                "llm_timeout",
                format!("Request timed out after {duration:?}"),
            ),
        LlmError::AuthenticationError(message) => ErrorEnvelope::new(
                ErrorCategory::Authentication,
                ErrorSeverity::Critical,
                false,
                "llm_auth_error",
                message,
            ),
        LlmError::InvalidRequest { message, .. } => ErrorEnvelope::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_invalid_request",
                message,
            ),
        LlmError::ModelError(message) => ErrorEnvelope::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_model_error",
                message,
            ),
        LlmError::ContentPolicyError(message) => ErrorEnvelope::new(
                ErrorCategory::Authorization,
                ErrorSeverity::Error,
                false,
                "llm_content_policy",
                message,
            ),
        LlmError::ParseError(message) => ErrorEnvelope::new(
                ErrorCategory::Parse,
                ErrorSeverity::Error,
                false,
                "llm_parse_error",
                message,
            ),
        LlmError::ContextLengthError(message) => ErrorEnvelope::new(
                ErrorCategory::InvalidInput,
                ErrorSeverity::Error,
                false,
                "llm_context_length",
                message,
            ),
            LlmError::Other(message) => ErrorEnvelope::new(
                ErrorCategory::Internal,
                ErrorSeverity::Error,
                true,
                "llm_other",
                message,
            ),
        }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: tui re-exports core classifier (full golden suite lives in `deepseek-core`).
    #[test]
    fn reexport_classify_matches_core() {
        assert_eq!(
            classify_error_message("rate limit exceeded"),
            ErrorCategory::RateLimit
        );
    }

    // ── LlmError → ErrorEnvelope (tui-only; orphan rule) ─────────────────

    #[test]
    fn llm_rate_limited_is_recoverable_warning() {
        let e: ErrorEnvelope = envelope_from_llm_error(crate::llm_client::LlmError::RateLimited {
            message: "slow down".into(),
            retry_after: None,
        });
        assert_eq!(e.category, ErrorCategory::RateLimit);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_network_error() {
        let e: ErrorEnvelope =
            envelope_from_llm_error(crate::llm_client::LlmError::NetworkError(
                "connection lost".into(),
            ));
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.severity, ErrorSeverity::Error);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_timeout() {
        let e: ErrorEnvelope =
            envelope_from_llm_error(crate::llm_client::LlmError::Timeout(
                std::time::Duration::from_secs(30),
            ));
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert_eq!(e.severity, ErrorSeverity::Warning);
        assert!(e.recoverable);
    }

    #[test]
    fn llm_auth_is_critical() {
        let e: ErrorEnvelope =
            envelope_from_llm_error(crate::llm_client::LlmError::AuthenticationError(
                "bad api key".into(),
            ));
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(e.severity, ErrorSeverity::Critical);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_invalid_request_is_non_recoverable() {
        let e: ErrorEnvelope = envelope_from_llm_error(crate::llm_client::LlmError::InvalidRequest {
            message: "model not found".into(),
            status: 400,
        });
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_content_policy() {
        let e: ErrorEnvelope =
            envelope_from_llm_error(crate::llm_client::LlmError::ContentPolicyError(
                "blocked".into(),
            ));
        assert_eq!(e.category, ErrorCategory::Authorization);
        assert!(!e.recoverable);
    }

    #[test]
    fn llm_context_length() {
        let e: ErrorEnvelope =
            envelope_from_llm_error(crate::llm_client::LlmError::ContextLengthError(
                "too long".into(),
            ));
        assert_eq!(e.category, ErrorCategory::InvalidInput);
        assert!(!e.recoverable);
    }
}
