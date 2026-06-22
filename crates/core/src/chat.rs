//! Chat / message types shared between the runtime core and the TUI shell.
//!
//! These are the wire-format types used to communicate with LLM APIs.
//! They live in `zagens-core` so both the TUI and future shells can depend
//! on them without pulling in TUI-specific code.

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────

pub const LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS: u32 = 128_000;
pub const DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS: u32 = 1_000_000;
/// Official DeepSeek V4 max output (384 × 1024). Desktop may request up to this cap.
pub const DEEPSEEK_V4_MAX_OUTPUT_TOKENS: u32 = 384 * 1024;
/// Unified API `max_tokens` ceiling for non-V4 providers (OpenRouter, Agnes, etc.).
pub const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 65_536;
/// NVIDIA NIM hosted chat endpoints cap **completion** tokens (not context length).
pub const NVIDIA_NIM_MAX_COMPLETION_TOKENS: u32 = 262_144;
pub const DEFAULT_COMPACTION_TOKEN_THRESHOLD: usize = 102_400;
const COMPACTION_THRESHOLD_PERCENT: u32 = 80;

// ── Message types ─────────────────────────────────────────────────────

/// Request payload for sending a message to the API.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Tool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
}

/// System prompt representation (plain text or structured blocks).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub enum SystemPrompt {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

/// A structured system prompt block.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// A chat message with role and content blocks.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

/// A single content block inside a message.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_blocks: Option<Vec<serde_json::Value>>,
    },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_search_tool_result")]
    ToolSearchToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
    #[serde(rename = "code_execution_tool_result")]
    CodeExecutionToolResult {
        tool_use_id: String,
        content: serde_json::Value,
    },
}

/// Cache control metadata for tool definitions and blocks.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CacheControl {
    #[serde(rename = "type")]
    pub cache_type: String,
}

/// Metadata describing who invoked a tool call.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ToolCaller {
    #[serde(rename = "type")]
    pub caller_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_id: Option<String>,
}

/// Tool definition exposed to the model.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Tool {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_callers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub defer_loading: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_examples: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Container metadata for code-execution style server tools.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Response payload for a message request.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MessageResponse {
    pub id: String,
    pub r#type: String,
    pub role: String,
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerInfo>,
    pub usage: crate::models::Usage,
}

// ── Streaming types ───────────────────────────────────────────────────

/// Streaming event types for SSE responses.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart { message: MessageResponse },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u32,
        content_block: ContentBlockStart,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta { index: u32, delta: Delta },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop { index: u32 },
    #[serde(rename = "message_delta")]
    MessageDelta {
        delta: MessageDelta,
        usage: Option<crate::models::Usage>,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
    #[serde(rename = "ping")]
    Ping,
}

/// Content block types used in streaming starts.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentBlockStart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        caller: Option<ToolCaller>,
    },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
}

/// A content delta inside a `content_block_delta` event.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

/// Delta payload for message-level updates.
#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone)]
pub struct MessageDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
}

// ── LLM Client trait (P2 PR3b) ───────────────────────────────────────

/// Type alias for boxed stream of SSE events.
pub type StreamEventBox = std::pin::Pin<
    Box<dyn futures_util::Stream<Item = anyhow::Result<StreamEvent>> + Send + 'static>,
>;

/// Unified interface for LLM providers — dyn-compatible via `#[async_trait]`.
///
/// Implementations live in the TUI shell (`DeepSeekClient`); the core only
/// depends on this trait so Engine/turn_loop can be provider-agnostic.
#[async_trait::async_trait]
pub trait LlmClient: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn model(&self) -> &str;
    async fn create_message(&self, request: MessageRequest) -> anyhow::Result<MessageResponse>;
    async fn create_message_stream(
        &self,
        request: MessageRequest,
    ) -> anyhow::Result<StreamEventBox>;
    async fn health_check(&self) -> anyhow::Result<bool> {
        Ok(true)
    }
    /// DeepSeek FIM (Fill-in-the-Middle) completion. Returns an error by default;
    /// `DeepSeekClient` overrides this.
    async fn fim_completion(
        &self,
        _model: &str,
        _prefix: &str,
        _suffix: &str,
        _max_tokens: u32,
    ) -> anyhow::Result<String> {
        Err(anyhow::anyhow!("FIM not supported by this provider"))
    }
}

// ── Context window helpers ────────────────────────────────────────────

/// Whether the model id refers to a DeepSeek V4-class model (1M context / high output cap).
#[must_use]
pub fn is_deepseek_v4_model(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    if !lower.contains("deepseek") {
        return false;
    }
    lower.contains("v4-pro")
        || lower.contains("v4-flash")
        || lower.contains("v4pro")
        || lower.contains("v4flash")
        || (lower.contains("v4") && !lower.contains("v3"))
}

/// Provider-aware `max_tokens` ceiling for outbound API requests.
#[must_use]
pub fn max_output_token_cap_for_model(model: &str) -> u32 {
    if is_deepseek_v4_model(model) {
        DEEPSEEK_V4_MAX_OUTPUT_TOKENS
    } else {
        DEFAULT_MAX_OUTPUT_TOKENS
    }
}

/// Clamp a client-requested `max_tokens` to the model's provider cap.
#[must_use]
pub fn clamp_max_output_tokens_for_model(model: &str, requested: u32) -> u32 {
    requested.min(max_output_token_cap_for_model(model))
}

/// Provider catalog cap (e.g. SenseNova `max_output_length`) layered on top of
/// {@link max_output_token_cap_for_model}. DeepSeek V4 models keep the main-model
/// 384K budget even on third-party gateways unless the catalog limit is higher.
#[must_use]
pub fn clamp_max_output_tokens_with_catalog_limit(
    model: &str,
    requested: u32,
    catalog_limit: Option<u32>,
) -> u32 {
    let base_cap = max_output_token_cap_for_model(model);
    let Some(limit) = catalog_limit.filter(|&v| v > 0) else {
        return requested.min(base_cap);
    };
    if is_deepseek_v4_model(model) {
        if limit > DEFAULT_MAX_OUTPUT_TOKENS {
            return requested.min(base_cap.min(limit));
        }
        // Catalog still on the generic 64K gate — V4 uses the main-model cap.
        return requested.min(base_cap);
    }
    requested.min(base_cap.min(limit))
}

/// Apply the hosted NVIDIA NIM completion ceiling after catalog / model caps.
#[must_use]
pub fn clamp_max_output_tokens_for_nvidia_nim(wired: u32) -> u32 {
    wired.min(NVIDIA_NIM_MAX_COMPLETION_TOKENS)
}

/// Map known models to their approximate context window sizes.
#[must_use]
pub fn context_window_for_model(model: &str) -> Option<u32> {
    let lower = model.to_lowercase();
    if lower.contains("deepseek") {
        if let Some(explicit_window) = deepseek_context_window_hint(&lower) {
            return Some(explicit_window);
        }
        if lower.contains("v4") {
            return Some(DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS);
        }
        return Some(LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS);
    }
    if lower.contains("claude") {
        return Some(200_000);
    }
    // Qwen / QwQ family — Qwen2.5+ generally support 128K context
    if lower.contains("qwen") || lower.contains("qwq") {
        return Some(128_000);
    }
    // Llama 3.x and later: 128K context
    if lower.contains("llama-3") || lower.contains("llama3") || lower.contains("llama_3") {
        return Some(128_000);
    }
    // Older Llama / Llama-2: 4K–8K, use conservative 4K
    if lower.contains("llama") {
        return Some(4_096);
    }
    // Mistral / Mixtral: 32K for older variants, many newer models support 128K
    if lower.contains("mixtral") {
        return Some(32_000);
    }
    if lower.contains("mistral") {
        return Some(32_000);
    }
    // Gemma 2 / Gemma 3: up to 128K context depending on variant; use 8K as safe lower bound
    if lower.contains("gemma") {
        return Some(8_192);
    }
    // Phi-3 / Phi-4: Microsoft Phi models generally have 128K context
    if lower.contains("phi-3")
        || lower.contains("phi3")
        || lower.contains("phi-4")
        || lower.contains("phi4")
    {
        return Some(128_000);
    }
    // GPT-4 series via OpenAI-compatible endpoints (e.g. OpenRouter)
    if lower.contains("gpt-4") || lower.contains("gpt4") {
        return Some(128_000);
    }
    // Yi family models from 01-ai
    if lower.contains("/yi-") || lower.starts_with("yi-") {
        return Some(200_000);
    }
    None
}

fn deepseek_context_window_hint(model_lower: &str) -> Option<u32> {
    let bytes = model_lower.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // Must be followed by 'k'/'K' with word-boundary guards.
            if i >= bytes.len() || bytes[i] != b'k' {
                continue;
            }
            let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
            let after_ok = i + 1 >= bytes.len() || !bytes[i + 1].is_ascii_alphanumeric();
            if !before_ok || !after_ok {
                continue;
            }
            if let Ok(kilo_tokens) = model_lower[start..i].parse::<u32>()
                && (8..=1024).contains(&kilo_tokens)
            {
                return Some(kilo_tokens.saturating_mul(1000));
            }
        } else {
            i += 1;
        }
    }
    None
}

/// Suggested compaction token threshold for a given model.
#[must_use]
pub fn compaction_threshold_for_model(model: &str) -> usize {
    let Some(window) = context_window_for_model(model) else {
        return DEFAULT_COMPACTION_TOKEN_THRESHOLD;
    };
    let threshold = (u64::from(window) * u64::from(COMPACTION_THRESHOLD_PERCENT)) / 100;
    usize::try_from(threshold).unwrap_or(DEFAULT_COMPACTION_TOKEN_THRESHOLD)
}

/// Get the default compaction threshold for the model override, or the
/// configured default.
#[must_use]
pub fn compaction_threshold_for_override(override_model: Option<&str>) -> usize {
    match override_model {
        Some(model) => compaction_threshold_for_model(model),
        None => DEFAULT_COMPACTION_TOKEN_THRESHOLD,
    }
}

#[cfg(test)]
mod max_output_tests {
    use super::*;

    #[test]
    fn v4_models_get_high_output_cap() {
        assert!(is_deepseek_v4_model("deepseek-v4-pro"));
        assert_eq!(
            max_output_token_cap_for_model("deepseek-v4-pro"),
            DEEPSEEK_V4_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn third_party_models_clamp_to_unified_cap() {
        assert!(!is_deepseek_v4_model("agnes-2.0-flash"));
        assert_eq!(
            clamp_max_output_tokens_for_model("agnes-2.0-flash", 393_216),
            DEFAULT_MAX_OUTPUT_TOKENS
        );
        assert_eq!(
            clamp_max_output_tokens_for_model("openrouter/owl-alpha", 200_000),
            DEFAULT_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn v4_on_sensenova_keeps_main_model_cap_when_catalog_is_64k() {
        assert_eq!(
            clamp_max_output_tokens_with_catalog_limit("deepseek-v4-flash", 393_216, Some(65_536)),
            DEEPSEEK_V4_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn v4_on_sensenova_respects_higher_catalog_limit() {
        assert_eq!(
            clamp_max_output_tokens_with_catalog_limit("deepseek-v4-flash", 500_000, Some(400_000)),
            DEEPSEEK_V4_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn native_sensenova_clamps_to_catalog() {
        assert_eq!(
            clamp_max_output_tokens_with_catalog_limit(
                "sensenova-6.7-flash-lite",
                393_216,
                Some(65_536)
            ),
            DEFAULT_MAX_OUTPUT_TOKENS
        );
    }

    #[test]
    fn nvidia_nim_completion_ceiling_clamps_inflated_catalog() {
        let wired = clamp_max_output_tokens_with_catalog_limit(
            "deepseek-ai/deepseek-v4-pro",
            393_216,
            Some(384_256),
        );
        assert_eq!(
            clamp_max_output_tokens_for_nvidia_nim(wired),
            NVIDIA_NIM_MAX_COMPLETION_TOKENS
        );
    }
}
