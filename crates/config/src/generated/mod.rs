//! @generated modules — see `scripts/generate-providers.py`.

#[path = "provider_defaults.rs"]
mod provider_defaults;
#[path = "provider_env.rs"]
mod provider_env;
#[path = "provider_kind.rs"]
mod provider_kind;
#[path = "providers_toml.rs"]
mod providers_toml;

pub use provider_defaults::*;
pub use provider_env::{PROVIDER_ENV_REGISTRY, first_nonempty_env};
pub use provider_kind::ProviderKind;
pub use providers_toml::ProvidersToml;
