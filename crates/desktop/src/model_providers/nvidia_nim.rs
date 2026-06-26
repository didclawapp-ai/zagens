//! NVIDIA NIM catalog quirks (B-tier: long exclude list + fixed completion cap).

use super::spec::CatalogEntry;

/// Hosted NIM chat endpoints cap completion tokens (see integrate.api.nvidia.com errors).
pub const NVIDIA_NIM_MAX_COMPLETION_TOKENS: u32 = 262_144;

const CHAT_EXCLUDE_ID_MARKERS: &[&str] = &[
    "embed",
    "rerank",
    "whisper",
    "riva-translate",
    "vision",
    "vlm",
    "-vl",
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

pub fn nvidia_nim_keep(entry: &CatalogEntry) -> bool {
    is_chat_capable_model_id(&entry.id)
}

pub fn is_chat_capable_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return false;
    }
    !CHAT_EXCLUDE_ID_MARKERS
        .iter()
        .any(|marker| lower.contains(marker))
}

pub fn nvidia_nim_output_limit(entry: &CatalogEntry) -> Option<u32> {
    let explicit = entry
        .max_output_length
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v > 0);
    explicit
        .map(|v| v.min(NVIDIA_NIM_MAX_COMPLETION_TOKENS))
        .or(Some(NVIDIA_NIM_MAX_COMPLETION_TOKENS))
}

use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Notify;

use super::catalog;
use super::spec::CatalogModelEntryJson;

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

fn to_nvidia_entry(m: CatalogModelEntryJson) -> NvidiaNimModelEntry {
    NvidiaNimModelEntry {
        id: m.id,
        name: m.name,
        owned_by: None,
        context_length: m.context_length,
        max_output_length: m.max_output_length,
    }
}

pub async fn list_nvidia_nim_models() -> Result<NvidiaNimModelList, String> {
    let catalog = catalog::list_catalog_models("nvidia-nim".to_string()).await?;
    Ok(NvidiaNimModelList {
        models: catalog.models.into_iter().map(to_nvidia_entry).collect(),
        current_model: catalog.current_model,
    })
}

pub async fn set_nvidia_nim_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    catalog::set_catalog_model_async("nvidia-nim".to_string(), model_id, sidecar_restart).await
}

#[cfg(test)]
mod api_tests {
    use super::is_chat_capable_model_id;

    #[test]
    fn chat_filter_excludes_embedding_and_vision_models() {
        assert!(!is_chat_capable_model_id(
            "nvidia/nemotron-nano-12b-v2-vl:1b"
        ));
        assert!(!is_chat_capable_model_id("nvidia/nv-embedqa-e5-v5"));
        assert!(is_chat_capable_model_id("deepseek-ai/deepseek-v4-flash"));
    }
}
