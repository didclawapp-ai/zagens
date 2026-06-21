//! User-defined OpenAI-compatible model providers (Phase 4).

use std::sync::Arc;

use tokio::sync::Notify;
use zagens_config::{
    ConfigStore, CustomProviderToml, ProviderKind, custom_provider_keyring_slot,
    custom_provider_ui_id, parse_custom_provider_ui_id, slugify_custom_provider_id,
};

use crate::model_providers::{
    ModelProviderSection, ModelProviderStatus, ProviderProbeResult, probe_models_endpoint,
};

fn unique_custom_id(store: &ConfigStore, base: &str) -> String {
    let base = if base.is_empty() { "provider" } else { base };
    if !store.config.custom_providers.contains_key(base) {
        return base.to_string();
    }
    for n in 2..100 {
        let candidate = format!("{base}-{n}");
        if !store.config.custom_providers.contains_key(&candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", chrono::Utc::now().timestamp())
}

fn normalize_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("API 路径不能为空".to_string());
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("API 路径需以 http:// 或 https:// 开头".to_string());
    }
    Ok(trimmed.to_string())
}

pub fn custom_provider_statuses(
    store: &ConfigStore,
    secrets: &zagens_secrets::Secrets,
) -> Vec<ModelProviderStatus> {
    let active_custom = store.config.provider == ProviderKind::Custom;
    let active_id = store
        .config
        .custom_provider_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    store
        .config
        .custom_providers
        .iter()
        .map(|(slug, entry)| {
            let slot = custom_provider_keyring_slot(slug);
            let configured = secrets.resolve(&slot).is_some();
            ModelProviderStatus {
                id: custom_provider_ui_id(slug),
                display_name: entry.display_name.clone(),
                section: ModelProviderSection::Custom,
                configured,
                active: active_custom && active_id == Some(slug.as_str()),
                key_required: true,
                model: Some(entry.model.clone()),
                base_url: Some(entry.base_url.clone()),
                service_ok: None,
                service_detail: None,
            }
        })
        .collect()
}

pub fn add_custom_model_provider(
    display_name: String,
    base_url: String,
    api_key: String,
    model: String,
    set_active: bool,
    sidecar_restart: &Arc<Notify>,
) -> Result<String, String> {
    let name = display_name.trim();
    if name.is_empty() {
        return Err("模型商名称不能为空".to_string());
    }
    let model = model.trim();
    if model.is_empty() {
        return Err("模型 ID 不能为空".to_string());
    }
    let key = api_key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".to_string());
    }
    let base_url = normalize_base_url(&base_url)?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let mut slug = slugify_custom_provider_id(name);
    if slug.is_empty() {
        slug = "provider".to_string();
    }
    slug = unique_custom_id(&store, &slug);

    store.config.custom_providers.insert(
        slug.clone(),
        CustomProviderToml {
            display_name: name.to_string(),
            base_url,
            model: model.to_string(),
        },
    );

    let slot = custom_provider_keyring_slot(&slug);
    zagens_secrets::Secrets::auto_detect()
        .set(&slot, key)
        .map_err(|e| format!("无法保存到系统密钥链: {e}"))?;

    if set_active {
        store.config.provider = ProviderKind::Custom;
        store.config.custom_provider_id = Some(slug.clone());
        store.config.default_text_model = Some(model.to_string());
    }

    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(custom_provider_ui_id(&slug))
}

pub fn remove_custom_model_provider(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let slug = parse_custom_provider_ui_id(&provider_id)
        .ok_or_else(|| "无效的自定义接入 ID".to_string())?;

    let secrets = zagens_secrets::Secrets::auto_detect();
    let slot = custom_provider_keyring_slot(&slug);
    secrets
        .delete(&slot)
        .map_err(|e| format!("无法从系统密钥链删除: {e}"))?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    if !store.config.custom_providers.remove(&slug).is_some() {
        return Err(format!("未找到自定义接入: {slug}"));
    }

    let was_active = store.config.provider == ProviderKind::Custom
        && store
            .config
            .custom_provider_id
            .as_deref()
            .is_some_and(|id| id == slug);
    if was_active {
        store.config.provider = ProviderKind::Deepseek;
        store.config.custom_provider_id = None;
        store.config.default_text_model = Some("deepseek-v4-pro".to_string());
    }

    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn save_custom_provider_credentials(
    provider_id: &str,
    api_key: Option<String>,
    base_url: Option<String>,
    model: Option<String>,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let slug = parse_custom_provider_ui_id(provider_id)
        .ok_or_else(|| "无效的自定义接入 ID".to_string())?;

    let key_trim = api_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());
    let secrets = zagens_secrets::Secrets::auto_detect();
    if key_trim.is_none()
        && secrets
            .resolve(&custom_provider_keyring_slot(&slug))
            .is_none()
    {
        return Err("API Key 不能为空".to_string());
    }
    if let Some(key) = key_trim.as_deref() {
        secrets
            .set(&custom_provider_keyring_slot(&slug), key)
            .map_err(|e| format!("无法保存到系统密钥链: {e}"))?;
    }

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let entry = store
        .config
        .custom_providers
        .get_mut(&slug)
        .ok_or_else(|| format!("未找到自定义接入: {slug}"))?;

    if let Some(url) = base_url {
        entry.base_url = normalize_base_url(&url)?;
    }
    if let Some(m) = model
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
    {
        entry.model = m;
    }

    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn activate_custom_model_provider(
    provider_id: &str,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let slug = parse_custom_provider_ui_id(provider_id)
        .ok_or_else(|| "无效的自定义接入 ID".to_string())?;

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let entry = store
        .config
        .custom_providers
        .get(&slug)
        .ok_or_else(|| format!("未找到自定义接入: {slug}"))?;
    let display_name = entry.display_name.clone();
    let model = entry.model.clone();

    let secrets = zagens_secrets::Secrets::auto_detect();
    if secrets
        .resolve(&custom_provider_keyring_slot(&slug))
        .is_none()
    {
        return Err(format!("请先配置 {display_name} 的 API Key"));
    }

    let mut store = store;
    store.config.provider = ProviderKind::Custom;
    store.config.custom_provider_id = Some(slug);
    store.config.default_text_model = Some(model);
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub async fn probe_custom_model_provider(provider_id: &str) -> Result<ProviderProbeResult, String> {
    let slug = parse_custom_provider_ui_id(provider_id)
        .ok_or_else(|| "无效的自定义接入 ID".to_string())?;

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let entry = store
        .config
        .custom_providers
        .get(&slug)
        .ok_or_else(|| format!("未找到自定义接入: {slug}"))?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets.resolve(&custom_provider_keyring_slot(&slug));

    Ok(probe_models_endpoint(&entry.base_url, api_key.as_deref()).await)
}
