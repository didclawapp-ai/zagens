//! Compile-time catalog provider registry (MP-4).

use zagens_config::ProviderKind;

use super::ModelProviderSection;
use super::agnes::{agnes_keep, agnes_output_limit};
use super::nvidia_nim::{nvidia_nim_keep, nvidia_nim_output_limit};
use super::openrouter::{openrouter_keep, openrouter_output_limit};
use super::spec::{CatalogListVariant, CatalogProvider, CatalogSpec, KeepRule, OutputLimitRule};

pub static CATALOG_PROVIDERS: &[CatalogProvider] = &[
    CatalogProvider::Spec(CatalogSpec {
        id: "openrouter",
        kind: ProviderKind::Openrouter,
        keyring_slot: "openrouter",
        models_path: "/models",
        variant: CatalogListVariant::FreePaid,
        keep: KeepRule::Custom(openrouter_keep),
        output_limit: OutputLimitRule::Custom(openrouter_output_limit),
        sync_catalog_before_set: false,
        sync_catalog_on_activate: false,
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "deepseek/deepseek-v4-flash",
        section: ModelProviderSection::Free,
        docs_url: Some("https://openrouter.ai/docs"),
    }),
    CatalogProvider::Spec(CatalogSpec {
        id: "sensenova",
        kind: ProviderKind::SenseNova,
        keyring_slot: "sensenova",
        models_path: "/models",
        variant: CatalogListVariant::Flat,
        keep: KeepRule::All,
        output_limit: OutputLimitRule::FromCatalog,
        sync_catalog_before_set: true,
        sync_catalog_on_activate: true,
        default_base_url: "https://token.sensenova.cn/v1",
        default_model: "sensenova-6.7-flash-lite",
        section: ModelProviderSection::Free,
        docs_url: Some("https://platform.sensenova.cn/docs"),
    }),
    CatalogProvider::Spec(CatalogSpec {
        id: "agnes",
        kind: ProviderKind::Agnes,
        keyring_slot: "agnes",
        models_path: "/models",
        variant: CatalogListVariant::Flat,
        keep: KeepRule::Custom(agnes_keep),
        output_limit: OutputLimitRule::Custom(agnes_output_limit),
        sync_catalog_before_set: false,
        sync_catalog_on_activate: false,
        default_base_url: "https://apihub.agnes-ai.com/v1",
        default_model: "agnes-2.0-flash",
        section: ModelProviderSection::Free,
        docs_url: Some("https://agnes-ai.com/doc"),
    }),
    CatalogProvider::Spec(CatalogSpec {
        id: "nvidia-nim",
        kind: ProviderKind::NvidiaNim,
        keyring_slot: "nvidia-nim",
        models_path: "/models",
        variant: CatalogListVariant::Flat,
        keep: KeepRule::Custom(nvidia_nim_keep),
        output_limit: OutputLimitRule::Custom(nvidia_nim_output_limit),
        sync_catalog_before_set: true,
        sync_catalog_on_activate: true,
        default_base_url: "https://integrate.api.nvidia.com/v1",
        default_model: "deepseek-ai/deepseek-v4-flash",
        section: ModelProviderSection::Free,
        docs_url: Some("https://build.nvidia.com/settings/api-key"),
    }),
    CatalogProvider::Spec(CatalogSpec {
        id: "novita",
        kind: ProviderKind::Novita,
        keyring_slot: "novita",
        models_path: "/models",
        variant: CatalogListVariant::Flat,
        keep: KeepRule::All,
        output_limit: OutputLimitRule::FromCatalog,
        sync_catalog_before_set: false,
        sync_catalog_on_activate: false,
        default_base_url: "https://api.novita.ai/v1",
        default_model: "deepseek/deepseek-v4-pro",
        section: ModelProviderSection::Free,
        docs_url: Some("https://novita.ai/docs"),
    }),
    CatalogProvider::Spec(CatalogSpec {
        id: "moonshot",
        kind: ProviderKind::Moonshot,
        keyring_slot: "moonshot",
        models_path: "/models",
        variant: CatalogListVariant::Flat,
        keep: KeepRule::All,
        output_limit: OutputLimitRule::FromCatalog,
        sync_catalog_before_set: true,
        sync_catalog_on_activate: true,
        default_base_url: "https://api.moonshot.cn/v1",
        default_model: "kimi-k3",
        section: ModelProviderSection::Free,
        docs_url: Some("https://platform.kimi.com/docs/guide/kimi-k3-quickstart"),
    }),
];

#[must_use]
pub fn catalog_by_id(id: &str) -> Option<&'static CatalogProvider> {
    CATALOG_PROVIDERS
        .iter()
        .find(|provider| provider.spec().id == id.trim())
}

#[must_use]
pub fn catalog_provider_has_picker(id: &str) -> bool {
    catalog_by_id(id).is_some()
}

#[must_use]
pub fn catalog_sync_before_set(id: &str) -> bool {
    catalog_by_id(id)
        .map(|provider| provider.spec().sync_catalog_before_set)
        .unwrap_or(false)
}
