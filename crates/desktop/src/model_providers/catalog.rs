//! Generic OpenAI-compatible catalog fetch / parse / persist / list / set (MP-4).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use tokio::sync::Notify;
use zagens_config::{ConfigStore, ProviderKind, with_config_mut};

use super::registry::catalog_by_id;
use super::spec::{
    CatalogEntry, CatalogListVariant, CatalogModelEntryJson, CatalogModelListJson, CatalogProvider,
    CatalogSpec, KeepRule, OutputLimitRule,
};

const CATALOG_FETCH_TIMEOUT: Duration = Duration::from_secs(12);

#[derive(Debug, Deserialize)]
struct OpenAiModelsEnvelope {
    data: Option<Vec<serde_json::Value>>,
}

pub fn default_parse_openai(body: &str) -> Result<Vec<CatalogEntry>, String> {
    let parsed: OpenAiModelsEnvelope =
        serde_json::from_str(body).map_err(|e| format!("无法解析模型列表 JSON: {e}"))?;
    Ok(parsed
        .data
        .unwrap_or_default()
        .into_iter()
        .filter_map(|raw| {
            let id = raw.get("id")?.as_str()?.trim().to_string();
            if id.is_empty() {
                return None;
            }
            let name = raw
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| id.clone());
            let context_length = raw.get("context_length").and_then(|v| v.as_u64());
            let max_output_length = raw
                .get("max_output_length")
                .or_else(|| raw.get("max_completion_tokens"))
                .or_else(|| raw.get("max_output_tokens"))
                .and_then(|v| v.as_u64());
            let description = raw
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Some(CatalogEntry {
                id: id.clone(),
                name,
                context_length,
                max_output_length,
                description,
                raw,
            })
        })
        .collect())
}

fn chat_only_keep(entry: &CatalogEntry) -> bool {
    let id = entry.id.to_ascii_lowercase();
    !id.contains("embed")
        && !id.contains("rerank")
        && !id.contains("whisper")
        && !id.contains("tts")
        && !id.contains("vision-only")
}

fn apply_keep_rule(spec: &CatalogSpec, entries: Vec<CatalogEntry>) -> Vec<CatalogEntry> {
    entries
        .into_iter()
        .filter(|entry| match spec.keep {
            KeepRule::All => true,
            KeepRule::ChatOnly => chat_only_keep(entry),
            KeepRule::Custom(pred) => pred(entry),
        })
        .collect()
}

fn output_limit_for_model(spec: &CatalogSpec, entry: &CatalogEntry) -> Option<u32> {
    match spec.output_limit {
        OutputLimitRule::None => None,
        OutputLimitRule::Fixed(limit) => Some(limit),
        OutputLimitRule::FromCatalog => entry
            .max_output_length
            .and_then(|v| u32::try_from(v).ok())
            .filter(|&v| v > 0 && v <= 1_000_000),
        OutputLimitRule::Table(table) => table
            .iter()
            .filter(|(prefix, _)| entry.id.starts_with(prefix))
            .max_by_key(|(prefix, _)| prefix.len())
            .map(|(_, limit)| *limit),
        OutputLimitRule::Custom(f) => f(entry),
    }
}

fn output_limits_from_entries(
    spec: &CatalogSpec,
    entries: &[CatalogEntry],
) -> BTreeMap<String, u32> {
    let mut limits = BTreeMap::new();
    for entry in entries {
        if let Some(limit) = output_limit_for_model(spec, entry) {
            limits.insert(entry.id.clone(), limit);
        }
    }
    limits
}

fn entries_for(provider: &CatalogProvider, body: &str) -> Result<Vec<CatalogEntry>, String> {
    let spec = provider.spec();
    let parsed = match provider {
        CatalogProvider::Spec(_) => default_parse_openai(body)?,
        CatalogProvider::Custom(catalog) => catalog.parse_entries(body)?,
    };
    Ok(apply_keep_rule(spec, parsed))
}

fn catalog_models_url(base_url: &str, models_path: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let path = models_path.trim();
    if path.is_empty() || path == "/models" {
        if trimmed.ends_with("/v1") {
            format!("{trimmed}/models")
        } else {
            format!("{trimmed}/v1/models")
        }
    } else if path.starts_with('/') {
        format!("{trimmed}{path}")
    } else {
        format!("{trimmed}/{path}")
    }
}

async fn fetch_catalog_body(
    base_url: &str,
    models_path: &str,
    api_key: Option<&str>,
) -> Result<String, String> {
    let url = catalog_models_url(base_url, models_path);
    let client = reqwest::Client::builder()
        .timeout(CATALOG_FETCH_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let mut last_err = String::new();
    for attempt in 0..2 {
        let mut req = client.get(&url).header(CONTENT_TYPE, "application/json");
        if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
            req = req.header(AUTHORIZATION, format!("Bearer {}", key.trim()));
        }
        match req.send().await {
            Ok(response) => {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                if status.is_success() {
                    return Ok(body);
                }
                last_err = format!("HTTP {status}: {body}");
            }
            Err(err) => {
                last_err = format!("连接失败: {err}");
            }
        }
        if attempt == 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    Err(last_err)
}

pub async fn fetch_catalog(provider_id: &str) -> Result<Vec<CatalogEntry>, String> {
    let provider =
        catalog_by_id(provider_id).ok_or_else(|| format!("未知 catalog 商: {provider_id}"))?;
    let spec = provider.spec();
    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let secrets = zagens_secrets::Secrets::auto_detect();
    let cfg = store.config.providers.for_provider(spec.kind);
    let base_url = cfg
        .base_url
        .as_deref()
        .filter(|u| !u.trim().is_empty())
        .unwrap_or(spec.default_base_url);
    let api_key = secrets.resolve(spec.keyring_slot);
    let body = fetch_catalog_body(base_url, spec.models_path, api_key.as_deref()).await?;
    entries_for(provider, &body)
}

pub fn persist_catalog(
    kind: ProviderKind,
    model_ids: &[String],
    output_limits: &BTreeMap<String, u32>,
) -> Result<(), String> {
    with_config_mut(None, |store| {
        let cfg = store.config.providers.for_provider_mut(kind);
        cfg.available_models = model_ids.to_vec();
        cfg.model_output_limits = output_limits.clone();
        Ok(())
    })
}

fn apply_catalog_defaults(store: &mut ConfigStore, spec: &CatalogSpec) {
    let cfg = store.config.providers.for_provider_mut(spec.kind);
    if cfg.base_url.as_ref().is_none_or(|u| u.trim().is_empty()) {
        cfg.base_url = Some(spec.default_base_url.to_string());
    }
    if cfg.model.as_ref().is_none_or(|m| m.trim().is_empty()) {
        cfg.model = Some(spec.default_model.to_string());
    }
    cfg.api_key = None;
}

fn is_catalog_configured(spec: &CatalogSpec, store: &ConfigStore) -> bool {
    let secrets = zagens_secrets::Secrets::auto_detect();
    if secrets.resolve(spec.keyring_slot).is_some() {
        return true;
    }
    store
        .config
        .providers
        .for_provider(spec.kind)
        .api_key
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty())
}

fn entry_to_json(entry: &CatalogEntry, is_free: Option<bool>) -> CatalogModelEntryJson {
    CatalogModelEntryJson {
        id: entry.id.clone(),
        name: entry.name.clone(),
        context_length: entry.context_length,
        max_output_length: entry.max_output_length,
        description: entry.description.clone(),
        is_free,
    }
}

pub async fn list_catalog_models(provider_id: String) -> Result<CatalogModelListJson, String> {
    let provider =
        catalog_by_id(&provider_id).ok_or_else(|| format!("未知 catalog 商: {provider_id}"))?;
    let spec = provider.spec();
    let entries = fetch_catalog(&provider_id).await?;
    let limits = output_limits_from_entries(spec, &entries);
    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let _ = persist_catalog(spec.kind, &ids, &limits);

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let current_model = store
        .config
        .providers
        .for_provider(spec.kind)
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| store.config.default_text_model.clone());

    match spec.variant {
        CatalogListVariant::Flat => Ok(CatalogModelListJson {
            variant: CatalogListVariant::Flat,
            models: entries.iter().map(|e| entry_to_json(e, None)).collect(),
            free: None,
            paid: None,
            current_model,
            output_limits: limits,
        }),
        CatalogListVariant::FreePaid => {
            let mut free = Vec::new();
            let mut paid = Vec::new();
            for entry in &entries {
                let is_free = super::openrouter::openrouter_entry_is_free(entry);
                let json = entry_to_json(entry, Some(is_free));
                if is_free {
                    free.push(json);
                } else {
                    paid.push(json);
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
            let models = free.iter().chain(paid.iter()).cloned().collect();
            Ok(CatalogModelListJson {
                variant: CatalogListVariant::FreePaid,
                models,
                free: Some(free),
                paid: Some(paid),
                current_model,
                output_limits: limits,
            })
        }
    }
}

pub async fn set_catalog_model_async(
    provider_id: String,
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let provider =
        catalog_by_id(&provider_id).ok_or_else(|| format!("未知 catalog 商: {provider_id}"))?;
    let spec = provider.spec();
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    if !is_catalog_configured(spec, &store) {
        return Err(format!("请先配置 {} 的 API Key", spec.id));
    }

    if spec.sync_catalog_before_set {
        let entries = fetch_catalog(&provider_id).await?;
        let limits = output_limits_from_entries(spec, &entries);
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        persist_catalog(spec.kind, &ids, &limits)?;
    }

    with_config_mut(None, |store| {
        apply_catalog_defaults(store, spec);
        let cfg = store.config.providers.for_provider_mut(spec.kind);
        cfg.model = Some(model_id.clone());
        store.config.provider = spec.kind;
        store.config.default_text_model = Some(model_id);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub fn set_catalog_model(
    provider_id: String,
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let provider =
        catalog_by_id(&provider_id).ok_or_else(|| format!("未知 catalog 商: {provider_id}"))?;
    let spec = provider.spec();
    if spec.sync_catalog_before_set {
        return Err(format!("{} 切换模型需走 async catalog 路径", spec.id));
    }

    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    if !is_catalog_configured(spec, &store) {
        return Err(format!("请先配置 {} 的 API Key", spec.id));
    }

    with_config_mut(None, |store| {
        apply_catalog_defaults(store, spec);
        let cfg = store.config.providers.for_provider_mut(spec.kind);
        cfg.model = Some(model_id.clone());
        store.config.provider = spec.kind;
        store.config.default_text_model = Some(model_id);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

pub async fn activate_catalog_provider(
    provider_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let provider =
        catalog_by_id(&provider_id).ok_or_else(|| format!("未知 catalog 商: {provider_id}"))?;
    let spec = provider.spec();

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    if !is_catalog_configured(spec, &store) {
        return Err(format!("请先配置 {} 的 API Key", spec.id));
    }

    if spec.sync_catalog_on_activate {
        let entries = fetch_catalog(&provider_id).await?;
        let limits = output_limits_from_entries(spec, &entries);
        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        persist_catalog(spec.kind, &ids, &limits)?;
    }

    let model = {
        let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
        store
            .config
            .providers
            .for_provider(spec.kind)
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| spec.default_model.to_string())
    };

    with_config_mut(None, |store| {
        apply_catalog_defaults(store, spec);
        store.config.provider = spec.kind;
        store.config.default_text_model = Some(model);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};

    use super::super::registry::CATALOG_PROVIDERS;
    use super::super::spec::{CatalogListVariant, CatalogSpec, KeepRule, OutputLimitRule};

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|p| p.into_inner())
    }

    const MOCK_MODELS_JSON: &str = r#"{"data":[{"id":"deepseek/deepseek-v4-flash","name":"DeepSeek V4 Flash","max_output_length":8192},{"id":"text-embedding-3-small","name":"Embed"}]}"#;

    fn spawn_mock_models_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
        listener.set_nonblocking(true).expect("nonblocking");
        let addr = listener.local_addr().expect("addr");
        let body = MOCK_MODELS_JSON;
        std::thread::spawn(move || {
            for _ in 0..500 {
                if let Ok((mut stream, _)) = listener.accept() {
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                    let _ = stream.read(&mut [0u8; 1024]);
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        format!("http://{addr}/v1")
    }

    fn temp_config_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "zagens-catalog-p4a-{}-{}.toml",
            std::process::id(),
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn default_parser_reads_openai_shape() {
        let entries = default_parse_openai(MOCK_MODELS_JSON).expect("parse");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "deepseek/deepseek-v4-flash");
    }

    #[test]
    fn table_output_limit_uses_longest_prefix() {
        let spec = CatalogSpec {
            id: "test",
            kind: ProviderKind::Novita,
            keyring_slot: "novita",
            models_path: "/models",
            variant: CatalogListVariant::Flat,
            keep: KeepRule::All,
            output_limit: OutputLimitRule::Table(&[
                ("deepseek/", 4096),
                ("deepseek/deepseek-v4", 8192),
            ]),
            sync_catalog_before_set: false,
            sync_catalog_on_activate: false,
            default_base_url: "https://example.com/v1",
            default_model: "m",
            section: super::super::ModelProviderSection::Free,
            docs_url: None,
        };
        let entry = CatalogEntry {
            id: "deepseek/deepseek-v4-flash".into(),
            name: "flash".into(),
            context_length: None,
            max_output_length: None,
            description: None,
            raw: serde_json::json!({}),
        };
        assert_eq!(output_limit_for_model(&spec, &entry), Some(8192));
    }

    #[test]
    fn every_catalog_provider_has_keyring_registry_slot() {
        for provider in CATALOG_PROVIDERS {
            let spec = provider.spec();
            assert!(
                zagens_secrets::KEYRING_SLOT_REGISTRY
                    .iter()
                    .any(|def| def.slot == spec.keyring_slot),
                "catalog provider {} missing keyring registry slot {}",
                spec.id,
                spec.keyring_slot
            );
        }
    }

    #[test]
    fn deepseek_and_ollama_not_in_catalog_registry() {
        assert!(catalog_by_id("deepseek").is_none());
        assert!(catalog_by_id("ollama").is_none());
    }

    #[tokio::test]
    #[allow(clippy::await_holding_lock)] // env_lock intentionally serializes env mutation across awaits
    async fn novita_stub_routes_list_set_activate() {
        let _lock = env_lock();
        let mock_base = spawn_mock_models_server();
        let config_path = temp_config_path();
        let prev_config = std::env::var("ZAGENS_CONFIG_PATH").ok();
        let prev_key = std::env::var("NOVITA_API_KEY").ok();

        unsafe {
            std::env::set_var("ZAGENS_CONFIG_PATH", &config_path);
            std::env::set_var("NOVITA_API_KEY", "test-novita-key");
        }

        with_config_mut(Some(config_path.clone()), |store| {
            store.config.providers.novita.base_url = Some(mock_base);
            Ok(())
        })
        .expect("seed config");

        let list = list_catalog_models("novita".to_string())
            .await
            .expect("list via registry");
        assert_eq!(list.variant, CatalogListVariant::Flat);
        assert!(
            list.models
                .iter()
                .any(|m| m.id == "deepseek/deepseek-v4-flash")
        );
        assert!(
            list.output_limits
                .contains_key("deepseek/deepseek-v4-flash")
        );

        let restart = Arc::new(Notify::new());
        set_catalog_model(
            "novita".to_string(),
            "deepseek/deepseek-v4-flash".to_string(),
            &restart,
        )
        .expect("set via registry");

        activate_catalog_provider("novita".to_string(), &restart)
            .await
            .expect("activate via registry");

        let final_store = ConfigStore::load(Some(config_path.clone())).expect("reload");
        assert_eq!(final_store.config.provider, ProviderKind::Novita);
        assert_eq!(
            final_store.config.default_text_model.as_deref(),
            Some("deepseek/deepseek-v4-flash")
        );

        unsafe {
            match prev_config {
                Some(v) => std::env::set_var("ZAGENS_CONFIG_PATH", v),
                None => std::env::remove_var("ZAGENS_CONFIG_PATH"),
            }
            match prev_key {
                Some(v) => std::env::set_var("NOVITA_API_KEY", v),
                None => std::env::remove_var("NOVITA_API_KEY"),
            }
        }
        let _ = std::fs::remove_file(config_path);
    }
}
