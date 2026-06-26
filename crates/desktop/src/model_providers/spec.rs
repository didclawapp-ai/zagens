//! Data-driven catalog provider specifications (MP-4).
#![allow(dead_code)] // registry variants/fields wired incrementally (MP-4 → P5)

use serde::Serialize;
use serde_json::Value;
use zagens_config::ProviderKind;

use super::ModelProviderSection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogListVariant {
    Flat,
    FreePaid,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogModelEntryJson {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_length: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogModelListJson {
    pub variant: CatalogListVariant,
    pub models: Vec<CatalogModelEntryJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free: Option<Vec<CatalogModelEntryJson>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paid: Option<Vec<CatalogModelEntryJson>>,
    pub current_model: Option<String>,
    pub output_limits: std::collections::BTreeMap<String, u32>,
}

#[derive(Debug, Clone)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub context_length: Option<u64>,
    pub max_output_length: Option<u64>,
    pub description: Option<String>,
    pub raw: Value,
}

/// Filter rule applied after parsing `/v1/models` JSON.
pub enum KeepRule {
    All,
    ChatOnly,
    Custom(fn(&CatalogEntry) -> bool),
}

/// Per-model completion cap applied when persisting catalog metadata.
pub enum OutputLimitRule {
    FromCatalog,
    Fixed(u32),
    Table(&'static [(&'static str, u32)]),
    Custom(fn(&CatalogEntry) -> Option<u32>),
    None,
}

/// A/B-tier catalog providers are fully described by this struct.
pub struct CatalogSpec {
    pub id: &'static str,
    pub kind: ProviderKind,
    pub keyring_slot: &'static str,
    pub models_path: &'static str,
    pub variant: CatalogListVariant,
    pub keep: KeepRule,
    pub output_limit: OutputLimitRule,
    pub sync_catalog_before_set: bool,
    pub sync_catalog_on_activate: bool,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub section: ModelProviderSection,
    pub docs_url: Option<&'static str>,
}

/// C-tier escape hatch for non-standard catalog APIs.
pub trait ChatModelCatalog: Send + Sync {
    fn spec(&self) -> &CatalogSpec;
    fn parse_entries(&self, body: &str) -> Result<Vec<CatalogEntry>, String>;
}

pub enum CatalogProvider {
    Spec(CatalogSpec),
    Custom(&'static dyn ChatModelCatalog),
}

impl CatalogProvider {
    pub fn spec(&self) -> &CatalogSpec {
        match self {
            Self::Spec(spec) => spec,
            Self::Custom(catalog) => catalog.spec(),
        }
    }
}
