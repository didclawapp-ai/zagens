use super::super::MAX_SUBAGENTS;
use super::super::generated::{PROVIDER_ENV_REGISTRY, first_nonempty_env};
use super::super::providers::ApiProvider;
use super::super::types::{CapacityConfig, Config, MemoryConfig, ProviderConfig, ProvidersConfig};
use super::model::parse_http_headers;

fn provider_entry_mut(config: &mut Config, provider: ApiProvider) -> &mut ProviderConfig {
    let providers = config
        .providers
        .get_or_insert_with(ProvidersConfig::default);
    match provider {
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
        ApiProvider::Moonshot => &mut providers.moonshot,
    }
}

// === Environment Overrides ===

pub(crate) fn apply_env_overrides(config: &mut Config) {
    if let Ok(value) = std::env::var("DEEPSEEK_PROVIDER") {
        config.provider = Some(value);
    }

    let active = config.api_provider();

    // Legacy: when Nvidia is active, DEEPSEEK_BASE_URL writes providers.nvidia_nim
    // (not root). Table-driven deepseek root write is skipped in that case below.
    if matches!(active, ApiProvider::NvidiaNim)
        && let Some(url) = first_nonempty_env(&["DEEPSEEK_BASE_URL"])
    {
        provider_entry_mut(config, ApiProvider::NvidiaNim).base_url = Some(url);
    }

    // Provider base_url / model from shared-defs/providers.toml.
    let mut global_model: Option<String> = None;
    for def in PROVIDER_ENV_REGISTRY {
        let Some(api) = ApiProvider::parse(def.id) else {
            continue;
        };

        if let Some(url) = first_nonempty_env(def.env_base_url) {
            if def.base_url_writes_root {
                if !(matches!(active, ApiProvider::NvidiaNim) && def.id == "deepseek") {
                    config.base_url = Some(url);
                }
            } else if api == active {
                provider_entry_mut(config, api).base_url = Some(url);
            }
        }

        if let Some(model) = first_nonempty_env(def.env_model) {
            if def.model_env_global {
                global_model = Some(model);
            } else if api == active {
                config.default_text_model = Some(model);
            }
        }
    }
    // Global DEEPSEEK_MODEL* applied last so it wins over provider-scoped *_MODEL.
    if let Some(model) = global_model {
        config.default_text_model = Some(model);
    }

    if let Ok(value) = std::env::var("DEEPSEEK_HTTP_HEADERS")
        && let Ok(headers) = parse_http_headers(&value)
        && !headers.is_empty()
    {
        let mut root_headers = config.http_headers.clone().unwrap_or_default();
        root_headers.extend(headers.clone());
        config.http_headers = Some(root_headers);

        let entry = provider_entry_mut(config, config.api_provider());
        let mut provider_headers = entry.http_headers.clone().unwrap_or_default();
        provider_headers.extend(headers);
        entry.http_headers = Some(provider_headers);
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

    if let Ok(value) = std::env::var("ZAGENS_BROWSER_YOLO") {
        let on = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "true" | "yes" | "y" | "enabled"
        );
        config.browser.get_or_insert_with(Default::default).yolo = Some(on);
    }
    // Policy bridge reads env each approval; mirror config → env so [browser] yolo works.
    if config.browser.as_ref().and_then(|b| b.yolo) == Some(true) {
        // SAFETY: config load runs before request workers; syncing env is intentional.
        unsafe {
            std::env::set_var("ZAGENS_BROWSER_YOLO", "1");
        }
    }
}
