//! Desktop model-provider panel — credentials, activation, and health probes.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use zagens_config::{CUSTOM_PROVIDER_UI_ID_PREFIX, ConfigStore, ProviderConfigToml, ProviderKind};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProviderSection {
    Primary,
    Free,
    Custom,
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
    /// Custom-provider only: user-configured hard cap on max_tokens per request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
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
    if is_custom_provider_id(provider_id.trim()) {
        return crate::custom_providers::remove_custom_model_provider(provider_id, sidecar_restart);
    }
    let preset = preset_by_id(provider_id.trim())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    secrets
        .delete(preset.keyring_slot)
        .map_err(|e| format!("无法从系统密钥链删除: {e}"))?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = provider_cfg_mut(&mut store, preset.kind);
    cfg.api_key = None;
    cfg.available_models.clear();
    cfg.model_output_limits.clear();
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

pub async fn activate_model_provider_async(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    activate_model_provider(provider_id.clone(), sidecar_restart)?;
    if provider_id.trim() == "sensenova" {
        sync_sensenova_models_catalog(None).await?;
    }
    if provider_id.trim() == "nvidia-nim" {
        crate::nvidia_nim_provider::sync_nvidia_nim_models_catalog(None).await?;
    }
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
    /// Per-model max completion tokens reported by OpenRouter.
    #[serde(default)]
    top_provider: Option<OpenRouterTopProvider>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterPricing {
    prompt: Option<String>,
    completion: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterTopProvider {
    max_completion_tokens: Option<u64>,
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
    let mut output_limits: std::collections::BTreeMap<String, u32> =
        std::collections::BTreeMap::new();

    for raw in parsed.data.unwrap_or_default() {
        if raw.id.trim().is_empty() {
            continue;
        }
        let id_lower = raw.id.to_ascii_lowercase();
        if id_lower.contains("embed") || id_lower.contains("moderation") {
            continue;
        }
        // Collect per-model max_completion_tokens from the top_provider field
        if let Some(limit) = raw
            .top_provider
            .as_ref()
            .and_then(|tp| tp.max_completion_tokens)
            .and_then(|v| u32::try_from(v).ok())
            .filter(|&v| v > 0 && v <= 1_000_000)
        {
            output_limits.insert(raw.id.clone(), limit);
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

    // Persist the per-model output limits so the runtime can use them for all providers
    if !output_limits.is_empty() {
        if let Ok(mut store_mut) = ConfigStore::load(None) {
            store_mut.config.providers.openrouter.model_output_limits = output_limits;
            let _ = store_mut.save();
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

const SENSENOVA_MODELS_URL: &str = "https://token.sensenova.cn/v1/models";

#[derive(Debug, Deserialize)]
struct SenseNovaModelsResponse {
    data: Option<Vec<SenseNovaModelRaw>>,
}

#[derive(Debug, Deserialize)]
struct SenseNovaModelRaw {
    id: String,
    name: Option<String>,
    description: Option<String>,
    context_length: Option<u64>,
    max_output_length: Option<u64>,
}

fn sensenova_display_name(raw: &SenseNovaModelRaw) -> String {
    raw.name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(raw.id.as_str())
        .to_string()
}

async fn fetch_sensenova_models_from_api() -> Result<Vec<SenseNovaModelEntry>, String> {
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets
        .resolve("sensenova")
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "请先保存 SenseNova API Key".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let response = client
        .get(SENSENOVA_MODELS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", api_key.trim()))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| format!("SenseNova 请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "SenseNova 模型列表 HTTP {status}: {}",
            truncate_probe_body(&body)
        ));
    }

    let parsed: SenseNovaModelsResponse =
        serde_json::from_str(&body).map_err(|e| format!("SenseNova 响应解析失败: {e}"))?;

    let mut models = Vec::new();
    for raw in parsed.data.unwrap_or_default() {
        if raw.id.trim().is_empty() {
            continue;
        }
        models.push(SenseNovaModelEntry {
            id: raw.id.clone(),
            name: sensenova_display_name(&raw),
            description: raw
                .description
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            context_length: raw.context_length,
            max_output_length: raw.max_output_length,
        });
    }

    models.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    if models.is_empty() {
        return Err("SenseNova 未返回可用模型".to_string());
    }

    Ok(models)
}

fn persist_sensenova_catalog(
    model_ids: &[String],
    output_limits: &std::collections::BTreeMap<String, u32>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.providers.sensenova.available_models = model_ids.to_vec();
    store.config.providers.sensenova.model_output_limits = output_limits.clone();
    store.save().map_err(|e| e.to_string())
}

fn sensenova_output_limits_from_models(
    models: &[SenseNovaModelEntry],
) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for model in models {
        if let Some(limit) = model.max_output_length.filter(|v| *v > 0) {
            let Ok(limit_u32) = u32::try_from(limit) else {
                continue;
            };
            out.insert(model.id.clone(), limit_u32);
        }
    }
    out
}

pub async fn list_sensenova_models() -> Result<SenseNovaModelList, String> {
    let models = fetch_sensenova_models_from_api().await?;
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let current_model = store
        .config
        .providers
        .sensenova
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| store.config.default_text_model.clone());
    Ok(SenseNovaModelList {
        models,
        current_model,
    })
}

/// Refresh `[providers.sensenova].available_models` from the official API (no sidecar restart).
pub async fn sync_sensenova_models_catalog(
    model_ids: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let models = fetch_sensenova_models_from_api().await?;
    let limits = sensenova_output_limits_from_models(&models);
    let ids = match model_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => models.into_iter().map(|m| m.id).collect(),
    };
    persist_sensenova_catalog(&ids, &limits)?;
    Ok(ids)
}

pub async fn set_sensenova_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let preset = preset_by_id("sensenova")?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    if !is_configured(
        preset,
        &ConfigStore::load(None).map_err(|e| e.to_string())?,
        &secrets,
    ) {
        return Err("请先配置 SenseNova API Key".to_string());
    }

    sync_sensenova_models_catalog(None).await?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    apply_provider_defaults(&mut store, preset);
    let cfg = provider_cfg_mut(&mut store, ProviderKind::SenseNova);
    cfg.model = Some(model_id.clone());
    store.config.provider = ProviderKind::SenseNova;
    store.config.default_text_model = Some(model_id);
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
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

/// Agnes 2.0 chat models: 256K context, 64K max output (official docs).
const AGNES_CHAT_CONTEXT_TOKENS: u32 = 256_000;
const AGNES_CHAT_MAX_OUTPUT_TOKENS: u32 = 65_536;

#[derive(Debug, Deserialize)]
struct AgnesModelsResponse {
    data: Option<Vec<AgnesModelRaw>>,
}

#[derive(Debug, Deserialize)]
struct AgnesModelRaw {
    id: String,
    name: Option<String>,
    #[serde(default)]
    max_model_len: Option<u64>,
    #[serde(default)]
    context_length: Option<u64>,
    #[serde(default)]
    max_output_length: Option<u64>,
    #[serde(default)]
    max_output_tokens: Option<u64>,
    #[serde(default)]
    max_tokens: Option<u64>,
}

fn agnes_display_name(raw: &AgnesModelRaw) -> String {
    raw.name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(raw.id.as_str())
        .to_string()
}

/// Chat/completions models only — image/video endpoints use different APIs.
fn is_agnes_chat_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return false;
    }
    if lower.contains("embed") || lower.contains("moderation") {
        return false;
    }
    if lower.contains("image") || lower.contains("video") {
        return false;
    }
    true
}

fn agnes_first_positive_limit(candidates: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    candidates.into_iter().flatten().find(|&v| v > 0)
}

fn agnes_known_output_limit(model_id: &str) -> Option<u32> {
    if is_agnes_chat_model_id(model_id) {
        Some(AGNES_CHAT_MAX_OUTPUT_TOKENS)
    } else {
        None
    }
}

fn agnes_output_limit(raw: &AgnesModelRaw) -> Option<u32> {
    agnes_first_positive_limit([raw.max_output_length, raw.max_output_tokens, raw.max_tokens])
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v > 0)
        .or_else(|| agnes_known_output_limit(&raw.id))
}

fn agnes_context_length(raw: &AgnesModelRaw) -> Option<u64> {
    agnes_first_positive_limit([raw.context_length, raw.max_model_len]).or_else(|| {
        if is_agnes_chat_model_id(&raw.id) {
            Some(u64::from(AGNES_CHAT_CONTEXT_TOKENS))
        } else {
            None
        }
    })
}

fn agnes_output_limits_from_models(
    models: &[AgnesModelEntry],
) -> std::collections::BTreeMap<String, u32> {
    let mut out = std::collections::BTreeMap::new();
    for model in models {
        if let Some(limit) = model.max_output_length.filter(|v| *v > 0) {
            let Ok(limit_u32) = u32::try_from(limit) else {
                continue;
            };
            out.insert(model.id.clone(), limit_u32);
        }
    }
    out
}

async fn fetch_agnes_models_from_api() -> Result<Vec<AgnesModelEntry>, String> {
    let preset = preset_by_id("agnes")?;
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets
        .resolve(preset.keyring_slot)
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "请先保存 Agnes AI API Key".to_string())?;
    let cfg = provider_cfg(&store, preset.kind);
    let base_url = cfg
        .base_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(preset.default_base_url);
    let url = normalize_models_url(base_url);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let response = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", api_key.trim()))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| format!("Agnes AI 请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "Agnes AI 模型列表 HTTP {status}: {}",
            truncate_probe_body(&body)
        ));
    }

    let parsed: AgnesModelsResponse =
        serde_json::from_str(&body).map_err(|e| format!("Agnes AI 响应解析失败: {e}"))?;

    let mut models = Vec::new();
    for raw in parsed.data.unwrap_or_default() {
        if raw.id.trim().is_empty() || !is_agnes_chat_model_id(&raw.id) {
            continue;
        }
        let max_output = agnes_output_limit(&raw);
        models.push(AgnesModelEntry {
            id: raw.id.clone(),
            name: agnes_display_name(&raw),
            context_length: agnes_context_length(&raw),
            max_output_length: max_output.map(u64::from),
        });
    }

    models.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    if models.is_empty() {
        return Err("Agnes AI 未返回可用模型".to_string());
    }

    Ok(models)
}

fn persist_agnes_catalog(
    model_ids: &[String],
    output_limits: &std::collections::BTreeMap<String, u32>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.providers.agnes.available_models = model_ids.to_vec();
    store.config.providers.agnes.model_output_limits = output_limits.clone();
    store.save().map_err(|e| e.to_string())
}

pub async fn list_agnes_models() -> Result<AgnesModelList, String> {
    let models = fetch_agnes_models_from_api().await?;
    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let limits = agnes_output_limits_from_models(&models);
    let _ = persist_agnes_catalog(&ids, &limits);

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let current_model = store
        .config
        .providers
        .agnes
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| store.config.default_text_model.clone());

    Ok(AgnesModelList {
        models,
        current_model,
    })
}

pub async fn set_agnes_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let preset = preset_by_id("agnes")?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    if !is_configured(
        preset,
        &ConfigStore::load(None).map_err(|e| e.to_string())?,
        &secrets,
    ) {
        return Err("请先配置 Agnes AI API Key".to_string());
    }

    let models = fetch_agnes_models_from_api().await?;
    let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
    let limits = agnes_output_limits_from_models(&models);
    persist_agnes_catalog(&ids, &limits)?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    apply_provider_defaults(&mut store, preset);
    let cfg = provider_cfg_mut(&mut store, ProviderKind::Agnes);
    cfg.model = Some(model_id.clone());
    store.config.provider = ProviderKind::Agnes;
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

#[cfg(test)]
mod agnes_tests {
    use super::*;

    #[test]
    fn chat_model_filter_excludes_image_and_video() {
        assert!(is_agnes_chat_model_id("agnes-2.0-flash"));
        assert!(is_agnes_chat_model_id("agnes-1.5-flash"));
        assert!(!is_agnes_chat_model_id("agnes-image-2.0-flash"));
        assert!(!is_agnes_chat_model_id("agnes-image-2.1-flash"));
        assert!(!is_agnes_chat_model_id("agnes-video-v2.0"));
    }

    #[test]
    fn known_output_limit_for_chat_models() {
        assert_eq!(
            agnes_known_output_limit("agnes-2.0-flash"),
            Some(AGNES_CHAT_MAX_OUTPUT_TOKENS)
        );
        assert_eq!(agnes_known_output_limit("agnes-image-2.0-flash"), None);
    }

    #[test]
    fn output_limit_prefers_api_field() {
        let raw = AgnesModelRaw {
            id: "agnes-2.0-flash".into(),
            name: None,
            max_model_len: None,
            context_length: None,
            max_output_length: Some(32_768),
            max_output_tokens: None,
            max_tokens: None,
        };
        assert_eq!(agnes_output_limit(&raw), Some(32_768));
    }
}
