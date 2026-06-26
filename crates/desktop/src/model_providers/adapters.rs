//! Thin wrappers: legacy per-provider IPC types → catalog registry (P4b).

use std::sync::Arc;

use tokio::sync::Notify;

use super::catalog;
use super::spec::CatalogModelEntryJson;
use super::{
    AgnesModelEntry, AgnesModelList, OpenRouterModelEntry, OpenRouterModelList,
    SenseNovaModelEntry, SenseNovaModelList,
};

fn to_openrouter_entry(m: CatalogModelEntryJson, is_free: bool) -> OpenRouterModelEntry {
    OpenRouterModelEntry {
        id: m.id,
        name: m.name,
        is_free,
    }
}

pub async fn list_openrouter_models() -> Result<OpenRouterModelList, String> {
    let catalog = catalog::list_catalog_models("openrouter".to_string()).await?;
    Ok(OpenRouterModelList {
        free: catalog
            .free
            .unwrap_or_default()
            .into_iter()
            .map(|m| to_openrouter_entry(m, true))
            .collect(),
        paid: catalog
            .paid
            .unwrap_or_default()
            .into_iter()
            .map(|m| to_openrouter_entry(m, false))
            .collect(),
        current_model: catalog.current_model,
    })
}

pub fn set_openrouter_model(model_id: String, sidecar_restart: &Arc<Notify>) -> Result<(), String> {
    catalog::set_catalog_model("openrouter".to_string(), model_id, sidecar_restart)
}

fn to_sensenova_entry(m: CatalogModelEntryJson) -> SenseNovaModelEntry {
    SenseNovaModelEntry {
        id: m.id,
        name: m.name,
        description: m.description,
        context_length: m.context_length,
        max_output_length: m.max_output_length,
    }
}

pub async fn list_sensenova_models() -> Result<SenseNovaModelList, String> {
    let catalog = catalog::list_catalog_models("sensenova".to_string()).await?;
    Ok(SenseNovaModelList {
        models: catalog.models.into_iter().map(to_sensenova_entry).collect(),
        current_model: catalog.current_model,
    })
}

pub async fn set_sensenova_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    catalog::set_catalog_model_async("sensenova".to_string(), model_id, sidecar_restart).await
}

fn to_agnes_entry(m: CatalogModelEntryJson) -> AgnesModelEntry {
    AgnesModelEntry {
        id: m.id,
        name: m.name,
        context_length: m.context_length,
        max_output_length: m.max_output_length,
    }
}

pub async fn list_agnes_models() -> Result<AgnesModelList, String> {
    let catalog = catalog::list_catalog_models("agnes".to_string()).await?;
    Ok(AgnesModelList {
        models: catalog.models.into_iter().map(to_agnes_entry).collect(),
        current_model: catalog.current_model,
    })
}

pub async fn set_agnes_model(
    model_id: String,
    sidecar_restart: &Arc<Notify>,
) -> Result<(), String> {
    catalog::set_catalog_model("agnes".to_string(), model_id, sidecar_restart)
}
