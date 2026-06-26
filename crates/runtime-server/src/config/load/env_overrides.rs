use super::super::MAX_SUBAGENTS;
use super::super::providers::ApiProvider;
use super::super::types::{CapacityConfig, Config, MemoryConfig, ProvidersConfig};
use super::model::parse_http_headers;

// === Environment Overrides ===

pub(crate) fn apply_env_overrides(config: &mut Config) {
    if let Ok(value) = std::env::var("DEEPSEEK_PROVIDER") {
        config.provider = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_BASE_URL") {
        if matches!(config.api_provider(), ApiProvider::NvidiaNim) {
            config
                .providers
                .get_or_insert_with(ProvidersConfig::default)
                .nvidia_nim
                .base_url = Some(value);
        } else {
            config.base_url = Some(value);
        }
    }
    if matches!(config.api_provider(), ApiProvider::NvidiaNim)
        && let Ok(value) = std::env::var("NVIDIA_NIM_BASE_URL")
            .or_else(|_| std::env::var("NIM_BASE_URL"))
            .or_else(|_| std::env::var("NVIDIA_BASE_URL"))
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .nvidia_nim
            .base_url = Some(value);
    }
    // OpenRouter / Novita are scoped only on their own provider entry — the
    // legacy root `base_url` keeps DeepSeek-only semantics.
    if matches!(config.api_provider(), ApiProvider::Openrouter)
        && let Ok(value) = std::env::var("OPENROUTER_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .openrouter
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Novita)
        && let Ok(value) = std::env::var("NOVITA_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .novita
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Fireworks)
        && let Ok(value) = std::env::var("FIREWORKS_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .fireworks
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Sglang)
        && let Ok(value) = std::env::var("SGLANG_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .sglang
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Vllm)
        && let Ok(value) = std::env::var("VLLM_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .vllm
            .base_url = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_HTTP_HEADERS")
        && let Ok(headers) = parse_http_headers(&value)
        && !headers.is_empty()
    {
        let mut root_headers = config.http_headers.clone().unwrap_or_default();
        root_headers.extend(headers.clone());
        config.http_headers = Some(root_headers);

        let provider = config.api_provider();
        let providers = config
            .providers
            .get_or_insert_with(ProvidersConfig::default);
        let entry = match provider {
            ApiProvider::Deepseek => &mut providers.deepseek,
            ApiProvider::DeepseekCN => &mut providers.deepseek_cn,
            ApiProvider::NvidiaNim => &mut providers.nvidia_nim,
            ApiProvider::Openai => &mut providers.openai,
            ApiProvider::Openrouter => &mut providers.openrouter,
            ApiProvider::Novita => &mut providers.novita,
            ApiProvider::Fireworks => &mut providers.fireworks,
            ApiProvider::Sglang => &mut providers.sglang,
            ApiProvider::Vllm => &mut providers.vllm,
            ApiProvider::Ollama => &mut providers.ollama,
            ApiProvider::Agnes => &mut providers.agnes,
            ApiProvider::SenseNova => &mut providers.sensenova,
        };
        let mut provider_headers = entry.http_headers.clone().unwrap_or_default();
        provider_headers.extend(headers);
        entry.http_headers = Some(provider_headers);
    }
    if matches!(config.api_provider(), ApiProvider::Ollama)
        && let Ok(value) = std::env::var("OLLAMA_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .ollama
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Openai)
        && let Ok(value) = std::env::var("OPENAI_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .openai
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Sglang)
        && let Ok(value) = std::env::var("SGLANG_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Vllm)
        && let Ok(value) = std::env::var("VLLM_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Ollama)
        && let Ok(value) = std::env::var("OLLAMA_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Openai)
        && let Ok(value) = std::env::var("OPENAI_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if let Ok(value) =
        std::env::var("DEEPSEEK_MODEL").or_else(|_| std::env::var("DEEPSEEK_DEFAULT_TEXT_MODEL"))
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::NvidiaNim)
        && let Ok(value) = std::env::var("NVIDIA_NIM_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SKILLS_DIR") {
        config.skills_dir = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MCP_CONFIG") {
        config.mcp_config_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_NOTES_PATH") {
        config.notes_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MEMORY_PATH") {
        config.memory_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MEMORY") {
        let on = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "true" | "yes" | "y" | "enabled"
        );
        config
            .memory
            .get_or_insert_with(MemoryConfig::default)
            .enabled = Some(on);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_ALLOW_SHELL") {
        config.allow_shell = Some(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    if let Ok(value) = std::env::var("DEEPSEEK_TRUST_MODE") {
        config.trust_mode = Some(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    if let Ok(value) = std::env::var("DEEPSEEK_APPROVAL_POLICY") {
        config.approval_policy = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_MODE") {
        config.sandbox_mode = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_WINDOWS_SANDBOX")
        && let Ok(mode) = crate::config::parse_windows_sandbox_mode(&value)
    {
        config.windows.get_or_insert_with(Default::default).sandbox = Some(mode);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_BACKEND") {
        config.sandbox_backend = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_URL") {
        config.sandbox_url = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_API_KEY") {
        config.sandbox_api_key = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MANAGED_CONFIG_PATH") {
        config.managed_config_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_REQUIREMENTS_PATH") {
        config.requirements_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MAX_SUBAGENTS")
        && let Ok(parsed) = value.parse::<usize>()
    {
        config.max_subagents = Some(parsed.clamp(1, MAX_SUBAGENTS));
    }

    let capacity = config.capacity.get_or_insert(CapacityConfig {
        enabled: None,
        low_risk_max: None,
        medium_risk_max: None,
        severe_min_slack: None,
        severe_violation_ratio: None,
        refresh_cooldown_turns: None,
        replan_cooldown_turns: None,
        max_replay_per_turn: None,
        min_turns_before_guardrail: None,
        profile_window: None,
        deepseek_v3_2_chat_prior: None,
        deepseek_v3_2_reasoner_prior: None,
        deepseek_v4_pro_prior: None,
        deepseek_v4_flash_prior: None,
        fallback_default_prior: None,
    });

    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_ENABLED") {
        let val = value.trim().to_ascii_lowercase();
        capacity.enabled = Some(matches!(val.as_str(), "1" | "true" | "yes" | "on"));
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_LOW_RISK_MAX")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.low_risk_max = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MEDIUM_RISK_MAX")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.medium_risk_max = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_SEVERE_MIN_SLACK")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.severe_min_slack = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_SEVERE_VIOLATION_RATIO")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.severe_violation_ratio = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_REFRESH_COOLDOWN_TURNS")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.refresh_cooldown_turns = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_REPLAN_COOLDOWN_TURNS")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.replan_cooldown_turns = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MAX_REPLAY_PER_TURN")
        && let Ok(parsed) = value.parse::<usize>()
    {
        capacity.max_replay_per_turn = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MIN_TURNS_BEFORE_GUARDRAIL")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.min_turns_before_guardrail = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PROFILE_WINDOW")
        && let Ok(parsed) = value.parse::<usize>()
    {
        capacity.profile_window = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_CHAT")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v3_2_chat_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_REASONER")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v3_2_reasoner_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_V4_PRO")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v4_pro_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_V4_FLASH")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v4_flash_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_FALLBACK")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.fallback_default_prior = Some(parsed);
    }

    if config.capacity.as_ref().is_some_and(|c| {
        c.enabled.is_none()
            && c.low_risk_max.is_none()
            && c.medium_risk_max.is_none()
            && c.severe_min_slack.is_none()
            && c.severe_violation_ratio.is_none()
            && c.refresh_cooldown_turns.is_none()
            && c.replan_cooldown_turns.is_none()
            && c.max_replay_per_turn.is_none()
            && c.min_turns_before_guardrail.is_none()
            && c.profile_window.is_none()
            && c.deepseek_v3_2_chat_prior.is_none()
            && c.deepseek_v3_2_reasoner_prior.is_none()
            && c.deepseek_v4_pro_prior.is_none()
            && c.deepseek_v4_flash_prior.is_none()
            && c.fallback_default_prior.is_none()
    }) {
        config.capacity = None;
    }
}
