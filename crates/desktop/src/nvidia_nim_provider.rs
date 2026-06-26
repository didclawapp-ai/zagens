//! NVIDIA NIM hosted LLM catalog (`integrate.api.nvidia.com`).

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use zagens_config::{ConfigStore, ProviderKind};

const NVIDIA_NIM_MODELS_URL: &str = "https://integrate.api.nvidia.com/v1/models";
const NVIDIA_NIM_DEFAULT_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
/// Hosted NIM chat endpoints cap completion tokens (see integrate.api.nvidia.com errors).
const NVIDIA_NIM_MAX_COMPLETION_TOKENS: u32 = 262_144;

/// Substrings that indicate non-chat / non-completion endpoints in the NIM catalog.
const CHAT_EXCLUDE_ID_MARKERS: &[&str] = &[
    "embed",
    "rerank",
    "whisper",
    "riva-translate",
    "vision",
    "vlm",
    "ocr",
    "grounding",
    "segmentation",
    "classification",
    "guardrail",
    "nemoguard",
    "gliner",
    "jailbreak",
    "fourcastnet",
    "proteina",
    "neva",
    "vila",
    "deplot",
    "fuyu",
    "kosmos",
    "nvclip",
    "parse",
    "detector",
    "chatqa",
    "starcoder",
    "recurrentgemma",
    "ising",
    "content-safety",
    "topic-control",
    "moderation",
    "diffusion",
    "flux",
    "sdxl",
    "healthcare",
    "boltz",
    "alphafold",
    "esmfold",
    "cuopt",
    "corrdiff",
];

#[derive(Debug, Clone, Serialize)]
pub struct NvidiaNimModelEntry {
    pub id: String,
    pub name: String,
    pub owned_by: Option<String>,
    pub context_length: Option<u64>,
    pub max_output_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NvidiaNimModelList {
    pub models: Vec<NvidiaNimModelEntry>,
    pub current_model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NvidiaNimModelsResponse {
    data: Option<Vec<NvidiaNimModelRaw>>,
}

#[derive(Debug, Deserialize)]
struct NvidiaNimModelRaw {
    id: String,
    name: Option<String>,
    owned_by: Option<String>,
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

fn nvidia_model_display_name(raw: &NvidiaNimModelRaw) -> String {
    raw.name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| raw.id.replace('/', " · "))
}

fn is_chat_capable_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return false;
    }
    !CHAT_EXCLUDE_ID_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

fn first_positive_limit(candidates: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    candidates.into_iter().flatten().find(|&v| v > 0)
}

fn nvidia_context_length(raw: &NvidiaNimModelRaw) -> Option<u64> {
    first_positive_limit([raw.context_length, raw.max_model_len])
}

fn nvidia_output_limit(raw: &NvidiaNimModelRaw) -> Option<u32> {
    let explicit =
        first_positive_limit([raw.max_output_length, raw.max_output_tokens, raw.max_tokens])
            .and_then(|v| u32::try_from(v).ok())
            .filter(|&v| v > 0);

    let fallback = first_positive_limit([raw.max_model_len, raw.context_length])
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v > 0);

    let limit = explicit.or(fallback)?;
    Some(limit.min(NVIDIA_NIM_MAX_COMPLETION_TOKENS))
}

async fn fetch_nvidia_nim_models_from_api() -> Result<
    (
        Vec<NvidiaNimModelEntry>,
        std::collections::BTreeMap<String, u32>,
    ),
    String,
> {
    let secrets = zagens_secrets::Secrets::auto_detect();
    let api_key = secrets
        .resolve("nvidia-nim")
        .filter(|k| !k.trim().is_empty())
        .ok_or_else(|| "请先保存 NVIDIA NIM API Key".to_string())?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .build()
        .map_err(|e| format!("HTTP 客户端初始化失败: {e}"))?;

    let response = client
        .get(NVIDIA_NIM_MODELS_URL)
        .header(AUTHORIZATION, format!("Bearer {}", api_key.trim()))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .await
        .map_err(|e| format!("NVIDIA NIM 请求失败: {e}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "NVIDIA NIM 模型列表 HTTP {status}: {}",
            truncate_probe_body(&body)
        ));
    }

    let parsed: NvidiaNimModelsResponse =
        serde_json::from_str(&body).map_err(|e| format!("NVIDIA NIM 响应解析失败: {e}"))?;

    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    let mut output_limits = std::collections::BTreeMap::new();

    for raw in parsed.data.unwrap_or_default() {
        if raw.id.trim().is_empty() || !is_chat_capable_model_id(&raw.id) {
            continue;
        }
        if !seen.insert(raw.id.clone()) {
            continue;
        }
        if let Some(limit) = nvidia_output_limit(&raw) {
            output_limits.insert(raw.id.clone(), limit);
        }
        let output = nvidia_output_limit(&raw);
        models.push(NvidiaNimModelEntry {
            id: raw.id.clone(),
            name: nvidia_model_display_name(&raw),
            owned_by: raw
                .owned_by
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            context_length: nvidia_context_length(&raw),
            max_output_length: output.map(u64::from),
        });
    }

    models.sort_by(|a, b| {
        a.name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase())
    });

    if models.is_empty() {
        return Err("NVIDIA NIM 未返回可用对话模型".to_string());
    }

    Ok((models, output_limits))
}

fn persist_nvidia_nim_catalog(
    model_ids: &[String],
    output_limits: &std::collections::BTreeMap<String, u32>,
) -> Result<(), String> {
    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    store.config.providers.nvidia_nim.available_models = model_ids.to_vec();
    store.config.providers.nvidia_nim.model_output_limits = output_limits.clone();
    store.save().map_err(|e| e.to_string())
}

/// Refresh `[providers.nvidia_nim].available_models` and output limits from the official API.
pub async fn sync_nvidia_nim_models_catalog(
    model_ids: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let (models, output_limits) = fetch_nvidia_nim_models_from_api().await?;
    let ids = match model_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => models.into_iter().map(|m| m.id).collect(),
    };
    persist_nvidia_nim_catalog(&ids, &output_limits)?;
    Ok(ids)
}

pub async fn list_nvidia_nim_models() -> Result<NvidiaNimModelList, String> {
    let (models, output_limits) = fetch_nvidia_nim_models_from_api().await?;
    if !output_limits.is_empty() {
        let ids: Vec<String> = models.iter().map(|m| m.id.clone()).collect();
        let _ = persist_nvidia_nim_catalog(&ids, &output_limits);
    }

    let store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let current_model = store
        .config
        .providers
        .nvidia_nim
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .or_else(|| store.config.default_text_model.clone());

    Ok(NvidiaNimModelList {
        models,
        current_model,
    })
}

pub async fn set_nvidia_nim_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    let model_id = model_id.trim().to_string();
    if model_id.is_empty() {
        return Err("请选择模型".to_string());
    }

    let secrets = zagens_secrets::Secrets::auto_detect();
    if secrets.resolve("nvidia-nim").is_none() {
        return Err("请先配置 NVIDIA NIM API Key".to_string());
    }

    sync_nvidia_nim_models_catalog(None).await?;

    let mut store = ConfigStore::load(None).map_err(|e| e.to_string())?;
    let cfg = &mut store.config.providers.nvidia_nim;
    if cfg.base_url.as_ref().is_none_or(|u| u.trim().is_empty()) {
        cfg.base_url = Some(NVIDIA_NIM_DEFAULT_BASE_URL.to_string());
    }
    cfg.model = Some(model_id.clone());
    store.config.provider = ProviderKind::NvidiaNim;
    store.config.default_text_model = Some(model_id);
    store.save().map_err(|e| e.to_string())?;
    sidecar_restart.notify_one();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_embedding_and_guard_models() {
        assert!(!is_chat_capable_model_id("nvidia/nv-embed-v1"));
        assert!(!is_chat_capable_model_id(
            "nvidia/llama-3.1-nemoguard-8b-content-safety"
        ));
        assert!(is_chat_capable_model_id("z-ai/glm5.1"));
        assert!(is_chat_capable_model_id("deepseek-ai/deepseek-v4-flash"));
    }

    #[test]
    fn prefers_explicit_output_limit_fields() {
        let raw = NvidiaNimModelRaw {
            id: "z-ai/glm5.1".into(),
            name: None,
            owned_by: Some("z-ai".into()),
            max_model_len: Some(200_000),
            context_length: None,
            max_output_length: Some(128_000),
            max_output_tokens: None,
            max_tokens: None,
        };
        assert_eq!(nvidia_output_limit(&raw), Some(128_000));
    }

    #[test]
    fn caps_context_fallback_to_nim_completion_ceiling() {
        let raw = NvidiaNimModelRaw {
            id: "deepseek-ai/deepseek-v4-pro".into(),
            name: None,
            owned_by: Some("deepseek-ai".into()),
            max_model_len: Some(384_256),
            context_length: None,
            max_output_length: None,
            max_output_tokens: None,
            max_tokens: None,
        };
        assert_eq!(
            nvidia_output_limit(&raw),
            Some(NVIDIA_NIM_MAX_COMPLETION_TOKENS)
        );
    }
}
