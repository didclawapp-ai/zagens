//! Desktop model-provider panel — credentials, activation, and health probes.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use zagens_config::{ConfigStore, ProviderConfigToml, ProviderKind};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderSection {
    Primary,
    Free,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelProviderPresetView {
    pub id: &'static str,
    pub display_name: &'static str,
    pub section: ModelProviderSection,
    pub key_required: bool,
    pub docs_url: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelProviderStatus {
    pub id: String,
    pub display_name: String,
    pub section: ModelProviderSection,
    pub configured: bool,
    pub active: bool,
    pub key_required: bool,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub service_ok: Option<bool>,
    pub service_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterModelEntry {
    pub id: String,
    pub name: String,
    pub is_free: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenRouterModelList {
    pub free: Vec<OpenRouterModelEntry>,
    pub paid: Vec<OpenRouterModelEntry>,
    pub current_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderProbeResult {
    pub ok: bool,
    pub message: String,
    pub models: Option<Vec<String>>,
}

struct ProviderPreset {
    id: &'static str,
    display_name: &'static str,
    kind: ProviderKind,
    keyring_slot: &'static str,
    default_base_url: &'static str,
    default_model: &'static str,
    key_required: bool,
    section: ModelProviderSection,
    docs_url: Option<&'static str>,
}

const PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "deepseek",
        display_name: "DeepSeek",
        kind: ProviderKind::Deepseek,
        keyring_slot: "deepseek",
        default_base_url: "https://api.deepseek.com/beta",
        default_model: "deepseek-v4-pro",
        key_required: true,
        section: ModelProviderSection::Primary,
        docs_url: Some("https://platform.deepseek.com/api_keys"),
    },
    ProviderPreset {
        id: "openrouter",
        display_name: "OpenRouter",
        kind: ProviderKind::Openrouter,
        keyring_slot: "openrouter",
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "deepseek/deepseek-v4-flash",
        key_required: true,
        section: ModelProviderSection::Free,
        docs_url: Some("https://openrouter.ai/docs"),
    },
    ProviderPreset {
        id: "ollama",
        display_name: "Ollama",
        kind: ProviderKind::Ollama,
        keyring_slot: "ollama",
        default_base_url: "http://localhost:11434/v1",
        default_model: "qwen2.5-coder:7b",
        key_required: false,
        section: ModelProviderSection::Free,
        docs_url: Some("https://ollama.com"),
    },
    ProviderPreset {
        id: "agnes",
        display_name: "Agnes AI",
        kind: ProviderKind::Agnes,
        keyring_slot: "agnes",
        default_base_url: "https://apihub.agnes-ai.com/v1",
        default_model: "agnes-2.0-flash",
        key_required: true,
        section: ModelProviderSection::Free,
        docs_url: Some("https://agnes-ai.com/doc"),
    },
    ProviderPreset {
        id: "sensenova",
        display_name: "SenseNova",
        kind: ProviderKind::SenseNova,
        keyring_slot: "sensenova",
        default_base_url: "https://token.sensenova.cn/v1",
        default_model: "sensenova-6.7-flash-lite",
        key_required: true,
        section: ModelProviderSection::Free,
        docs_url: Some("https://platform.sensenova.cn/docs"),
    },
];

fn preset_by_id(id: &str) -> Result<&'static ProviderPreset, String> {
    PRESETS
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("未知模型接入: {id}"))
}

fn provider_cfg<'a>(store: &'a ConfigStore, kind: ProviderKind) -> &'a ProviderConfigToml {
    store.config.providers.for_provider(kind)
}

fn provider_cfg_mut<'a>(
    store: &'a mut ConfigStore,
    kind: ProviderKind,
) -> &'a mut ProviderConfigToml {
    store.config.providers.for_provider_mut(kind)
}

fn is_configured(
    preset: &ProviderPreset,
    store: &ConfigStore,
    secrets: &zagens_secrets::Secrets,
) -> bool {
    if secrets.resolve(preset.keyring_slot).is_some() {
        return true;
    }
    let cfg = provider_cfg(store, preset.kind);
    if preset.kind == ProviderKind::Deepseek {
        return store
            .config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.trim().is_empty());
    }
    if !preset.key_required {
        return cfg.base_url.as_ref().is_some_and(|u| !u.trim().is_empty());
    }
    cfg.api_key.as_ref().is_some_and(|k| !k.trim().is_empty())
}

pub fn list_model_provider_presets() -> Vec<ModelProviderPresetView> {
    PRESETS
        .iter()
        .map(|p| ModelProviderPresetView {
            id: p.id,
            display_name: p.display_name,
            section: p.section,
            key_required: p.key_required,
            docs_url: p.docs_url,
        })
        .collect()
}

pub fn get_model_providers_status() -> Result<Vec<ModelProviderStatus>, String> {
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    let active = store.config.provider;

    Ok(PRESETS
        .iter()
        .map(|preset| {
            let cfg = provider_cfg(&store, preset.kind);
            ModelProviderStatus {
                id: preset.id.to_string(),
                display_name: preset.display_name.to_string(),
                section: preset.section,
                configured: is_configured(preset, &store, &secrets),
                active: active == preset.kind,
                key_required: preset.key_required,
                model: cfg
                    .model
                    .clone()
                    .filter(|m| !m.trim().is_empty())
                    .or_else(|| Some(preset.default_model.to_string())),
                base_url: cfg
                    .base_url
                    .clone()
                    .filter(|u| !u.trim().is_empty())
                    .or_else(|| Some(preset.default_base_url.to_string())),
                service_ok: None,
                service_detail: None,
            }
        })
        .collect())
}

fn apply_provider_defaults(store: &mut ConfigStore, preset: &ProviderPreset) {
    let cfg = provider_cfg_mut(store, preset.kind);
    if cfg.base_url.as_ref().is_none_or(|u| u.trim().is_empty()) {
        cfg.base_url = Some(preset.default_base_url.to_string());
    }
    if cfg.model.as_ref().is_none_or(|m| m.trim().is_empty()) {
        cfg.model = Some(preset.default_model.to_string());
    }
    cfg.api_key = None;
    if preset.kind == ProviderKind::Deepseek {
        store.config.api_key = None;
    }
}

pub fn save_model_provider_credentials(
    provider_id: String,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let preset = preset_by_id(provider_id.trim())?;
    let key_trim = api_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    if preset.key_required && key_trim.is_none() {
        let secrets = zagens_secrets::Secrets::auto_detect();
        if secrets.resolve(preset.keyring_slot).is_none() {
            return Err("API Key 不能为空".to_string());
        }
    }

    if let Some(key) = key_trim.as_deref() {
        let secrets = zagens_secrets::Secrets::auto_detect();
        secrets
            .set(preset.keyring_slot, key)
            .map_err(|e| format!("无法保存到系统密钥链: {e}"))?;
    }

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    apply_provider_defaults(&mut store, preset);

    let cfg = provider_cfg_mut(&mut store, preset.kind);
    if let Some(url) = base_url
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
    {
        cfg.base_url = Some(url);
    }
    if let Some(m) = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
    {
        cfg.model = Some(m);
    }
    cfg.api_key = None;
    if preset.kind == ProviderKind::Deepseek {
        store.config.api_key = None;
    }

    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn clear_model_provider_credentials(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let preset = preset_by_id(provider_id.trim())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    secrets
        .delete(preset.keyring_slot)
        .map_err(|e| format!("无法从系统密钥链删除: {e}"))?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = provider_cfg_mut(&mut store, preset.kind);
    cfg.api_key = None;
    if preset.kind == ProviderKind::Deepseek {
        store.config.api_key = None;
    }
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn activate_model_provider(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let preset = preset_by_id(provider_id.trim())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    if preset.key_required
        && !is_configured(
            preset,
            &ConfigStore::load(None).map_err(|e| e.to_string())?,
            &secrets,
        )
    {
        return Err(format!("请先配置 {} 的 API Key", preset.display_name));
    }

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    apply_provider_defaults(&mut store, preset);
    let model = provider_cfg(&store, preset.kind)
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| preset.default_model.to_string());

    store.config.provider = preset.kind;
    store.config.default_text_model = Some(model);
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

fn normalize_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

#[derive(Debug, Deserialize)]
struct OpenAiModelsResponse {
    data: Option<Vec<OpenAiModelEntry>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiModelEntry {
    id: String,
}

async fn probe_models_endpoint(base_url: &str, api_key: Option<&str>) -> ProviderProbeResult {
    let url = normalize_models_url(base_url);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            return ProviderProbeResult {
                ok: false,
                message: format!("HTTP 客户端初始化失败: {err}"),
                models: None,
            };
        }
    };

    let mut req = client.get(&url).header(CONTENT_TYPE, "application/json");
    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        req = req.header(AUTHORIZATION, format!("Bearer {}", key.trim()));
    }

    let response = match req.send().await {
        Ok(r) => r,
        Err(err) => {
            return ProviderProbeResult {
                ok: false,
                message: format!("连接失败: {err}"),
                models: None,
            };
        }
    };

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return ProviderProbeResult {
            ok: false,
            message: format!("HTTP {status}: {}", truncate_probe_body(&body)),
            models: None,
        };
    }

    if let Ok(parsed) = serde_json::from_str::<OpenAiModelsResponse>(&body) {
        let models: Vec<String> = parsed
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.id)
            .filter(|id| !id.trim().is_empty())
            .collect();
        if !models.is_empty() {
            return ProviderProbeResult {
                ok: true,
                message: format!("服务可用（{} 个模型）", models.len()),
                models: Some(models),
            };
        }
    }

    ProviderProbeResult {
        ok: true,
        message: "服务可用".to_string(),
        models: None,
    }
}

fn truncate_probe_body(body: &str) -> String {
    const MAX: usize = 160;
    if body.len() <= MAX {
        return body.to_string();
    }
    format!("{}…", &body[..MAX])
}

pub async fn probe_model_provider(provider_id: String) -> Result<ProviderProbeResult, String> {
    let preset = preset_by_id(provider_id.trim())?;
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    let cfg = provider_cfg(&store, preset.kind);
    let base_url = cfg
        .base_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(preset.default_base_url);
    let api_key = secrets.resolve(preset.keyring_slot);

    let result = probe_models_endpoint(base_url, api_key.as_deref()).await;

    if result.ok
        && preset.kind == ProviderKind::Ollama
        && let Some(models) = result.models.as_ref()
        && !models.is_empty()
    {
        let mut store = store;
        let cfg = provider_cfg_mut(&mut store, preset.kind);
        if cfg.model.as_ref().is_none_or(|m| m.trim().is_empty()) {
            cfg.model = Some(models[0].clone());
            let _ = store.save();
        }
    }

    Ok(result)
}

const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";

#[derive(Debug, Deserialize)]
struct OpenRouterModelsResponse {
    data: Option<Vec<OpenRouterModelRaw>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterModelRaw {
    id: String,
    name: Option<String>,
    pricing: Option<OpenRouterPricing>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    prompt: Option<String>,
    completion: Option<String>,
}

#[must_use]
fn openrouter_model_is_free(id: &str, pricing: Option<&OpenRouterPricing>) -> bool {
    let id_lower = id.to_ascii_lowercase();
    if id_lower.contains(":free") || id_lower.ends_with("-free") {
        return true;
    }
    let Some(pricing) = pricing else {
        return false;
    };
    let prompt_free = pricing.prompt.as_deref().is_some_and(pricing_is_zero);
    let completion_free = pricing.completion.as_deref().is_some_and(pricing_is_zero);
    prompt_free && completion_free
}

fn pricing_is_zero(raw: &str) -> bool {
    match raw.trim().parse::<f64>() {
        Ok(v) => v == 0.0,
        Err(_) => raw.trim() == "0",
    }
}

fn openrouter_display_name(raw: &OpenRouterModelRaw) -> String {
    raw.name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(raw.id.as_str())
        .to_string()
}

pub async fn list_openrouter_models() -> Result<OpenRouterModelList, String> {
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets
        .resolve("openrouter")
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "请先保存 OpenRouter API Key".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let response = client
        .get(OPENROUTER_MODELS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", api_key.trim()))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| format!("OpenRouter 请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "OpenRouter 模型列表 HTTP {status}: {}",
            truncate_probe_body(&body)
        ));
    }

    let parsed: OpenRouterModelsResponse =
        serde_json::from_str(&body).map_err(|e| format!("OpenRouter 响应解析失败: {e}"))?;

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let current_model = store
        .config
        .providers
        .openrouter
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| store.config.default_text_model.clone());

    let mut free = Vec::new();
    let mut paid = Vec::new();

    for raw in parsed.data.unwrap_or_default() {
        if raw.id.trim().is_empty() {
            continue;
        }
        let id_lower = raw.id.to_ascii_lowercase();
        if id_lower.contains("embed") || id_lower.contains("moderation") {
            continue;
        }
        let entry = OpenRouterModelEntry {
            id: raw.id.clone(),
            name: openrouter_display_name(&raw),
            is_free: openrouter_model_is_free(&raw.id, raw.pricing.as_ref()),
        };
        if entry.is_free {
            free.push(entry);
        } else {
            paid.push(entry);
        }
    }

    free.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });
    paid.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    Ok(OpenRouterModelList {
        free,
        paid,
        current_model,
    })
}

pub fn set_openrouter_model(model_id: String, sidecar_restart: &Arc<Notify>) -> Result<(), String> {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let preset = preset_by_id("openrouter")?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    if !is_configured(
        preset,
        &ConfigStore::load(None).map_err(|e| e.to_string())?,
        &secrets,
    ) {
        return Err("请先配置 OpenRouter API Key".to_string());
    }

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    apply_provider_defaults(&mut store, preset);
    let cfg = provider_cfg_mut(&mut store, ProviderKind::Openrouter);
    cfg.model = Some(model_id.clone());
    store.config.provider = ProviderKind::Openrouter;
    store.config.default_text_model = Some(model_id);
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

#[cfg(test)]
mod keyring_injection_tests {
    use super::*;

    /// Model panel presets that require a key must have sidecar env injection.
    #[test]
    fn preset_keyring_slots_inject_into_sidecar_env() {
        for preset in PRESETS {
            if !preset.key_required {
                continue;
            }
            assert!(
                zagens_secrets::keyring_slot_has_sidecar_env_injection(preset.keyring_slot),
                "preset {} ({}) missing sidecar env injection for keyring slot `{}`",
                preset.id,
                preset.display_name,
                preset.keyring_slot
            );
        }
    }
}

#[cfg(test)]
mod openrouter_tests {
    use super::*;

    #[test]
    fn free_when_pricing_zero() {
        let pricing = OpenRouterPricing {
            prompt: Some("0".into()),
            completion: Some("0".into()),
        };
        assert!(openrouter_model_is_free(
            "meta-llama/llama-3.3-70b",
            Some(&pricing)
        ));
    }

    #[test]
    fn free_when_id_suffix() {
        assert!(openrouter_model_is_free("deepseek/deepseek-r1:free", None));
    }

    #[test]
    fn paid_when_prompt_nonzero() {
        let pricing = OpenRouterPricing {
            prompt: Some("0.000003".into()),
            completion: Some("0".into()),
        };
        assert!(!openrouter_model_is_free("openai/gpt-4.1", Some(&pricing)));
    }
}
