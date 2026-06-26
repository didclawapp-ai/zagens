//! Desktop model-provider panel — credentials, activation, and health probes.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use zagens_config::{
    CUSTOM_PROVIDER_UI_ID_PREFIX, ConfigStore, ProviderConfigToml, ProviderKind, with_config_mut,
};

mod adapters;
mod agnes;
mod catalog;
mod custom_catalog;
mod nvidia_nim;
mod openrouter;
mod preset;
mod registry;
mod spec;

pub use adapters::{
    list_agnes_models, list_openrouter_models, list_sensenova_models, set_agnes_model,
    set_openrouter_model, set_sensenova_model,
};
pub use catalog::{list_catalog_models, set_catalog_model, set_catalog_model_async};
pub use nvidia_nim::{NvidiaNimModelList, list_nvidia_nim_models, set_nvidia_nim_model};
pub use registry::catalog_sync_before_set;
pub use spec::CatalogModelListJson;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderSection {
    Primary,
    Free,
    Custom,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
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
    /// Custom-provider only: user-configured hard cap on max_tokens per request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Catalog-backed model picker (OpenRouter, SenseNova, …).
    pub has_catalog_picker: bool,
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
pub struct SenseNovaModelEntry {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub context_length: Option<u64>,
    pub max_output_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SenseNovaModelList {
    pub models: Vec<SenseNovaModelEntry>,
    pub current_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgnesModelEntry {
    pub id: String,
    pub name: String,
    pub context_length: Option<u64>,
    pub max_output_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgnesModelList {
    pub models: Vec<AgnesModelEntry>,
    pub current_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderProbeResult {
    pub ok: bool,
    pub message: String,
    pub models: Option<Vec<String>>,
}

pub(crate) struct ProviderPreset {
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

pub(crate) fn preset_by_id(id: &str) -> Result<&'static ProviderPreset, String> {
    if id.trim().starts_with(CUSTOM_PROVIDER_UI_ID_PREFIX) {
        return Err(format!("未知模型接入: {id}"));
    }
    PRESETS
        .iter()
        .find(|p| p.id == id)
        .ok_or_else(|| format!("未知模型接入: {id}"))
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
        id: "nvidia-nim",
        display_name: "NVIDIA NIM",
        kind: ProviderKind::NvidiaNim,
        keyring_slot: "nvidia-nim",
        default_base_url: "https://integrate.api.nvidia.com/v1",
        default_model: "deepseek-ai/deepseek-v4-flash",
        key_required: true,
        section: ModelProviderSection::Free,
        docs_url: Some("https://build.nvidia.com/settings/api-key"),
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

fn is_custom_provider_id(id: &str) -> bool {
    id.trim().starts_with(CUSTOM_PROVIDER_UI_ID_PREFIX)
}

fn provider_cfg(store: &ConfigStore, kind: ProviderKind) -> &ProviderConfigToml {
    store.config.providers.for_provider(kind)
}

fn provider_cfg_mut(store: &mut ConfigStore, kind: ProviderKind) -> &mut ProviderConfigToml {
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

#[allow(dead_code)]
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

    let mut out: Vec<ModelProviderStatus> = PRESETS
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
                max_output_tokens: None,
                has_catalog_picker: registry::catalog_provider_has_picker(preset.id),
            }
        })
        .collect();

    out.extend(crate::custom_providers::custom_provider_statuses(
        &store, &secrets,
    ));
    Ok(out)
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
    max_output_tokens: Option<u32>,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    if is_custom_provider_id(provider_id.trim()) {
        return crate::custom_providers::save_custom_provider_credentials(
            provider_id.trim(),
            api_key,
            base_url,
            model,
            max_output_tokens,
            sidecar_restart,
        );
    }
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

    with_config_mut(None, |store| {
        apply_provider_defaults(store, preset);

        let cfg = provider_cfg_mut(store, preset.kind);
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
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn clear_model_provider_credentials(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    if is_custom_provider_id(provider_id.trim()) {
        return crate::custom_providers::remove_custom_model_provider(provider_id, sidecar_restart);
    }
    let preset = preset_by_id(provider_id.trim())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    secrets
        .delete(preset.keyring_slot)
        .map_err(|e| format!("无法从系统密钥链删除: {e}"))?;

    with_config_mut(None, |store| {
        let cfg = provider_cfg_mut(store, preset.kind);
        cfg.api_key = None;
        cfg.available_models.clear();
        cfg.model_output_limits.clear();
        if preset.kind == ProviderKind::Deepseek {
            store.config.api_key = None;
        }
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn activate_model_provider(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    if is_custom_provider_id(provider_id.trim()) {
        return crate::custom_providers::activate_custom_model_provider(
            provider_id.trim(),
            sidecar_restart,
        );
    }
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

    with_config_mut(None, |store| {
        apply_provider_defaults(store, preset);
        let model = provider_cfg(store, preset.kind)
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| preset.default_model.to_string());

        store.config.provider = preset.kind;
        store.config.default_text_model = Some(model);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub async fn activate_model_provider_async(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let id = provider_id.trim();
    if is_custom_provider_id(id) {
        activate_model_provider(provider_id, sidecar_restart)?;
        return Ok(());
    }
    if registry::catalog_by_id(id).is_some() {
        return catalog::activate_catalog_provider(provider_id, sidecar_restart).await;
    }
    activate_model_provider(provider_id, sidecar_restart)
}

fn normalize_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    // DeepSeek's `/models` endpoint lives at the API root
    // (https://api-docs.deepseek.com/zh-cn/api/list-models: GET /models),
    // NOT under `/v1` or `/beta`. The preset base_url is
    // `https://api.deepseek.com/beta` (chosen so chat/completions can use
    // beta features), but probing `/beta/models` 404s — strip the version
    // segment for DeepSeek hosts and hit `<root>/models` instead.
    let is_deepseek_host = trimmed
        .split("://")
        .nth(1)
        .is_some_and(|rest| rest.starts_with("api.deepseek.com"));
    if is_deepseek_host {
        let root = trimmed
            .strip_suffix("/v1")
            .or_else(|| trimmed.strip_suffix("/beta"))
            .unwrap_or(trimmed);
        return format!("{root}/models");
    }
    if trimmed.ends_with("/v1") || trimmed.ends_with("/beta") {
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

pub(crate) async fn probe_models_endpoint(
    base_url: &str,
    api_key: Option<&str>,
) -> ProviderProbeResult {
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
    let mut end = MAX;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &body[..end])
}

pub async fn probe_model_provider(provider_id: String) -> Result<ProviderProbeResult, String> {
    if is_custom_provider_id(provider_id.trim()) {
        return crate::custom_providers::probe_custom_model_provider(provider_id.trim()).await;
    }
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
        && store
            .config
            .providers
            .ollama
            .model
            .as_ref()
            .is_none_or(|m| m.trim().is_empty())
    {
        let first_model = models[0].clone();
        let _ = with_config_mut(None, |store| {
            let cfg = provider_cfg_mut(store, preset.kind);
            cfg.model = Some(first_model);
            Ok(())
        });
    }

    Ok(result)
}

#[cfg(test)]
mod normalize_models_url_tests {
    use super::*;

    #[test]
    fn appends_v1_for_bare_host() {
        assert_eq!(
            normalize_models_url("https://api.example.com"),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn keeps_v1_suffix() {
        assert_eq!(
            normalize_models_url("https://api.example.com/v1"),
            "https://api.example.com/v1/models"
        );
    }

    #[test]
    fn deepseek_beta_base_hits_root_models() {
        // DeepSeek preset base_url is `https://api.deepseek.com/beta`, but the
        // list-models endpoint is `GET /models` at the API root (per
        // https://api-docs.deepseek.com/zh-cn/api/list-models). `/beta/models`
        // and `/beta/v1/models` both 404 — must resolve to `<root>/models`.
        assert_eq!(
            normalize_models_url("https://api.deepseek.com/beta"),
            "https://api.deepseek.com/models"
        );
    }

    #[test]
    fn deepseek_bare_base_hits_root_models() {
        assert_eq!(
            normalize_models_url("https://api.deepseek.com"),
            "https://api.deepseek.com/models"
        );
    }

    #[test]
    fn deepseek_v1_base_hits_root_models() {
        assert_eq!(
            normalize_models_url("https://api.deepseek.com/v1"),
            "https://api.deepseek.com/models"
        );
    }

    #[test]
    fn deepseek_trims_trailing_slash() {
        assert_eq!(
            normalize_models_url("https://api.deepseek.com/beta/"),
            "https://api.deepseek.com/models"
        );
    }
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

    #[test]
    fn every_preset_slot_in_registry() {
        for preset in PRESETS {
            if !preset.key_required {
                continue;
            }
            assert!(
                zagens_secrets::KEYRING_SLOT_REGISTRY
                    .iter()
                    .any(|def| def.slot == preset.keyring_slot),
                "preset {} ({}) keyring slot `{}` missing from KEYRING_SLOT_REGISTRY",
                preset.id,
                preset.display_name,
                preset.keyring_slot
            );
        }
    }

    #[test]
    fn every_catalog_preset_in_registry() {
        const CATALOG_PRESET_IDS: &[&str] = &["openrouter", "sensenova", "agnes", "nvidia-nim"];
        for id in CATALOG_PRESET_IDS {
            assert!(
                registry::catalog_by_id(id).is_some(),
                "catalog preset {id} missing from CATALOG_PROVIDERS"
            );
            assert!(
                PRESETS.iter().any(|p| p.id == *id),
                "catalog provider {id} missing from PRESETS"
            );
        }
    }
}
