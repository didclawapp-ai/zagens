use std::collections::HashMap;

use anyhow::Result;

use super::super::providers::{ApiProvider, normalize_model_name};
use super::super::types::Config;
use super::super::{
    DEFAULT_FIREWORKS_MODEL, DEFAULT_NOVITA_FLASH_MODEL, DEFAULT_NOVITA_MODEL,
    DEFAULT_NVIDIA_NIM_FLASH_MODEL, DEFAULT_NVIDIA_NIM_MODEL, DEFAULT_OPENROUTER_FLASH_MODEL,
    DEFAULT_OPENROUTER_MODEL, DEFAULT_SGLANG_FLASH_MODEL, DEFAULT_SGLANG_MODEL,
    DEFAULT_VLLM_FLASH_MODEL, DEFAULT_VLLM_MODEL,
};

pub(crate) fn normalize_model_config(config: &mut Config) {
    if let Some(model) = config.default_text_model.as_deref()
        && !matches!(
            config.api_provider(),
            ApiProvider::Ollama
                | ApiProvider::Openai
                | ApiProvider::Agnes
                | ApiProvider::SenseNova
                | ApiProvider::Openrouter
        )
        && let Some(normalized) = normalize_model_for_provider(config.api_provider(), model)
    {
        config.default_text_model = Some(normalized);
    }

    if let Some(providers) = config.providers.as_mut() {
        if let Some(model) = providers.deepseek.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Deepseek, model)
        {
            providers.deepseek.model = Some(normalized);
        }
        if let Some(model) = providers.deepseek_cn.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::DeepseekCN, model)
        {
            providers.deepseek_cn.model = Some(normalized);
        }
        if let Some(model) = providers.nvidia_nim.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::NvidiaNim, model)
        {
            providers.nvidia_nim.model = Some(normalized);
        }
        if let Some(model) = providers.openrouter.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Openrouter, model)
        {
            providers.openrouter.model = Some(normalized);
        }
        if let Some(model) = providers.novita.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Novita, model)
        {
            providers.novita.model = Some(normalized);
        }
        if let Some(model) = providers.fireworks.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Fireworks, model)
        {
            providers.fireworks.model = Some(normalized);
        }
        if let Some(model) = providers.sglang.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Sglang, model)
        {
            providers.sglang.model = Some(normalized);
        }
        if let Some(model) = providers.vllm.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Vllm, model)
        {
            providers.vllm.model = Some(normalized);
        }
    }
}

pub(crate) fn normalize_model_for_provider(provider: ApiProvider, model: &str) -> Option<String> {
    // Ollama and OpenAI model identifiers are free-form; DeepSeek-style
    // normalization does not apply.
    if matches!(
        provider,
        ApiProvider::Ollama
            | ApiProvider::Openai
            | ApiProvider::Agnes
            | ApiProvider::SenseNova
            | ApiProvider::Openrouter
    ) {
        return None;
    }
    normalize_model_name(model).map(|normalized| model_for_provider(provider, normalized))
}

pub(crate) fn model_for_provider(provider: ApiProvider, normalized: String) -> String {
    let lowered = normalized.to_ascii_lowercase();
    match (provider, lowered.as_str()) {
        (ApiProvider::NvidiaNim, "deepseek-v4-pro") => DEFAULT_NVIDIA_NIM_MODEL.to_string(),
        (ApiProvider::NvidiaNim, "deepseek-v4-flash") => DEFAULT_NVIDIA_NIM_FLASH_MODEL.to_string(),
        (ApiProvider::Openrouter, "deepseek-v4-pro") => DEFAULT_OPENROUTER_MODEL.to_string(),
        (ApiProvider::Openrouter, "deepseek-v4-flash") => {
            DEFAULT_OPENROUTER_FLASH_MODEL.to_string()
        }
        (ApiProvider::Novita, "deepseek-v4-pro") => DEFAULT_NOVITA_MODEL.to_string(),
        (ApiProvider::Novita, "deepseek-v4-flash") => DEFAULT_NOVITA_FLASH_MODEL.to_string(),
        (ApiProvider::Fireworks, "deepseek-v4-pro") => DEFAULT_FIREWORKS_MODEL.to_string(),
        (ApiProvider::Fireworks, "deepseek-v4-flash") => {
            // Flash not yet available on Fireworks; fall through to normalized name
            "accounts/fireworks/models/deepseek-v4-flash".to_string()
        }
        (ApiProvider::Sglang, "deepseek-v4-pro") => DEFAULT_SGLANG_MODEL.to_string(),
        (ApiProvider::Sglang, "deepseek-v4-flash") => DEFAULT_SGLANG_FLASH_MODEL.to_string(),
        (ApiProvider::Vllm, "deepseek-v4-pro") => DEFAULT_VLLM_MODEL.to_string(),
        (ApiProvider::Vllm, "deepseek-v4-flash") => DEFAULT_VLLM_FLASH_MODEL.to_string(),
        _ => normalized,
    }
}

pub(crate) fn normalize_base_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let deepseek_domains = ["api.deepseek.com", "api.deepseeki.com"];
    if deepseek_domains
        .iter()
        .any(|domain| trimmed.contains(domain))
    {
        return trimmed.trim_end_matches("/v1").to_string();
    }
    trimmed.to_string()
}

pub(crate) fn parse_http_headers(raw: &str) -> Result<HashMap<String, String>> {
    let mut headers = HashMap::new();
    for pair in raw.trim().split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((name, value)) = pair.split_once('=') else {
            anyhow::bail!("invalid header pair '{pair}', expected name=value");
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            anyhow::bail!("header name cannot be empty");
        }
        if value.is_empty() {
            continue;
        }
        headers.insert(name.to_string(), value.to_string());
    }
    Ok(headers)
}
