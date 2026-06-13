use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiProvider {
    Deepseek,
    DeepseekCN,
    NvidiaNim,
    Openai,
    Openrouter,
    Novita,
    Fireworks,
    Sglang,
    Vllm,
    Ollama,
}

impl ApiProvider {
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "deepseek" | "deep-seek" => Some(Self::Deepseek),
            "deepseek-cn" | "deepseek_china" | "deepseekcn" | "deepseek-china" => {
                Some(Self::DeepseekCN)
            }
            "nvidia" | "nvidia-nim" | "nvidia_nim" | "nim" => Some(Self::NvidiaNim),
            "openai" | "open-ai" | "openai-compatible" => Some(Self::Openai),
            "openrouter" | "open_router" => Some(Self::Openrouter),
            "novita" => Some(Self::Novita),
            "fireworks" | "fireworks-ai" => Some(Self::Fireworks),
            "sglang" | "sg-lang" => Some(Self::Sglang),
            "vllm" | "v-llm" => Some(Self::Vllm),
            "ollama" | "ollama-local" => Some(Self::Ollama),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deepseek => "deepseek",
            Self::DeepseekCN => "deepseek-cn",
            Self::NvidiaNim => "nvidia-nim",
            Self::Openai => "openai",
            Self::Openrouter => "openrouter",
            Self::Novita => "novita",
            Self::Fireworks => "fireworks",
            Self::Sglang => "sglang",
            Self::Vllm => "vllm",
            Self::Ollama => "ollama",
        }
    }

    /// Human-friendly label for picker UIs / status chips.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Deepseek => "DeepSeek",
            Self::DeepseekCN => "DeepSeek (中国)",
            Self::NvidiaNim => "NVIDIA NIM",
            Self::Openai => "OpenAI",
            Self::Openrouter => "OpenRouter",
            Self::Novita => "Novita AI",
            Self::Fireworks => "Fireworks AI",
            Self::Sglang => "SGLang",
            Self::Vllm => "vLLM",
            Self::Ollama => "Ollama",
        }
    }

    /// All providers, in the order shown in the picker.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Deepseek,
            Self::DeepseekCN,
            Self::NvidiaNim,
            Self::Openai,
            Self::Openrouter,
            Self::Novita,
            Self::Fireworks,
            Self::Sglang,
            Self::Vllm,
            Self::Ollama,
        ]
    }
}

// ============================================================================
// Provider Capability Matrix
// ============================================================================

/// Known capabilities for a provider + resolved-model combination.
///
/// Returned by [`provider_capability`] to describe what a given provider
/// supports for the resolved model string.  All fields are derived from
/// static knowledge (release docs, API guides) rather than live API probes.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ProviderCapability {
    /// Canonical provider identifier.
    pub provider: ApiProvider,
    /// Resolved model identifier that will be sent in the API payload.
    pub resolved_model: String,
    /// Context window in tokens (the maximum input the model can accept).
    pub context_window: u32,
    /// Official maximum output tokens for this combo.
    ///
    /// This is model metadata for diagnostics and CI policy. Normal turns use
    /// a separate, more conservative request cap in the engine.
    pub max_output: u32,
    /// Whether the provider+model supports thinking/reasoning mode.
    pub thinking_supported: bool,
    /// Whether the provider returns prompt-cache telemetry fields.
    pub cache_telemetry_supported: bool,
    /// Which request-payload dialect the provider uses.
    pub request_payload_mode: RequestPayloadMode,
}

/// Which request-payload dialect the provider speaks.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum RequestPayloadMode {
    /// Standard OpenAI-compatible `/v1/chat/completions` payload.
    ChatCompletions,
}

/// Resolve the provider capability for a given [`ApiProvider`] and resolved
/// model string.
///
/// The `resolved_model` should be the final model identifier that will appear
/// in the API payload (after normalization / provider-specific mapping).
#[must_use]
pub fn provider_capability(provider: ApiProvider, resolved_model: &str) -> ProviderCapability {
    if matches!(provider, ApiProvider::Ollama) {
        return ProviderCapability {
            provider,
            resolved_model: resolved_model.to_string(),
            context_window: 8192,
            max_output: 4096,
            thinking_supported: false,
            cache_telemetry_supported: false,
            request_payload_mode: RequestPayloadMode::ChatCompletions,
        };
    }

    let model_lower = resolved_model.to_ascii_lowercase();
    let is_v4_pro = model_lower.contains("v4-pro") || model_lower == "deepseek-v4pro";
    let is_v4_flash = model_lower.contains("v4-flash")
        || model_lower == "deepseek-v4flash"
        || model_lower == "deepseek-v4";

    // Context window: V4-class models get 1M, everything else falls through
    // to the model's own lookup or a default.
    let context_window = if is_v4_pro || is_v4_flash {
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    } else {
        crate::models::context_window_for_model(resolved_model)
            .unwrap_or(crate::models::LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS)
    };

    // Max output tokens: official DeepSeek V4 API metadata lists 384K;
    // runtime request caps remain separate and more conservative.
    let max_output = if is_v4_pro || is_v4_flash {
        384_000
    } else {
        4096
    };

    // Thinking support: V4 models support thinking on all providers, but
    // only when the model name matches the V4 family.
    let thinking_supported = is_v4_pro || is_v4_flash;

    // Cache telemetry: returned only by DeepSeek-native and NVIDIA NIM endpoints.
    let cache_telemetry_supported = matches!(
        provider,
        ApiProvider::Deepseek | ApiProvider::DeepseekCN | ApiProvider::NvidiaNim
    );

    // Request payload mode: all current providers use chat completions.
    let request_payload_mode = RequestPayloadMode::ChatCompletions;

    ProviderCapability {
        provider,
        resolved_model: resolved_model.to_string(),
        context_window,
        max_output,
        thinking_supported,
        cache_telemetry_supported,
        request_payload_mode,
    }
}

/// Canonicalize compact DeepSeek model aliases to stable IDs.
///
/// Already-valid model IDs pass through unchanged. Only the compact
/// `v4pro`/`v4flash` spellings are rewritten to their hyphenated forms.
#[must_use]
pub fn canonical_model_name(model: &str) -> Option<&'static str> {
    match model.trim().to_ascii_lowercase().as_str() {
        "deepseek-v4pro" => Some("deepseek-v4-pro"),
        "deepseek-v4flash" => Some("deepseek-v4-flash"),
        _ => None,
    }
}

/// Normalize a configured/runtime model name.
///
/// Trims whitespace, preserves caller-provided case for already-valid model
/// IDs, and only canonicalizes compact aliases like `deepseek-v4pro`.
/// Non-DeepSeek or malformed names return `None`; DeepSeek's `/v1/models`
/// endpoint is the authority on valid model IDs.
#[must_use]
pub fn normalize_model_name(model: &str) -> Option<String> {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(canonical) = canonical_model_name(trimmed) {
        return Some(canonical.to_string());
    }

    let normalized = trimmed.to_ascii_lowercase();
    if !normalized.starts_with("deepseek") && !normalized.contains("/deepseek") {
        return None;
    }

    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '/'))
    {
        return Some(trimmed.to_string());
    }

    None
}

#[cfg(test)]
mod provider_drift_tests {
    //! Kernel-v2 M0.5: keep the facade `ProviderKind` and the runtime
    //! `ApiProvider` enums from drifting apart. A new variant added on one
    //! side without the other will fail here instead of silently falling
    //! back to DeepSeek defaults at runtime.

    use super::ApiProvider;
    use zagens_config::ProviderKind;

    /// Every facade provider kind must parse into a runtime `ApiProvider`
    /// with the same canonical string. This is the bug class M0.5 fixes:
    /// `provider = "openai"` previously parsed to `None` and silently fell
    /// back to DeepSeek base URL / credentials.
    #[test]
    fn every_facade_provider_kind_parses_into_runtime_api_provider() {
        for kind in ProviderKind::ALL {
            let name = kind.as_str();
            let api = ApiProvider::parse(name).unwrap_or_else(|| {
                panic!("facade ProviderKind '{name}' has no runtime ApiProvider mapping")
            });
            assert_eq!(
                api.as_str(),
                name,
                "canonical string mismatch for facade provider '{name}'"
            );
        }
    }

    /// Every runtime provider must map back to a facade kind, except
    /// `deepseek-cn` which is a runtime-only regional endpoint alias of
    /// `deepseek` (the facade exposes a single DeepSeek entry).
    #[test]
    fn every_runtime_api_provider_maps_back_to_facade_kind() {
        for api in ApiProvider::all() {
            if *api == ApiProvider::DeepseekCN {
                continue;
            }
            let name = api.as_str();
            assert!(
                ProviderKind::ALL.iter().any(|kind| kind.as_str() == name),
                "runtime ApiProvider '{name}' has no facade ProviderKind counterpart"
            );
        }
    }

    /// `parse` must round-trip the canonical string of every variant.
    #[test]
    fn api_provider_parse_round_trips_canonical_strings() {
        for api in ApiProvider::all() {
            assert_eq!(ApiProvider::parse(api.as_str()), Some(*api));
        }
    }
}
