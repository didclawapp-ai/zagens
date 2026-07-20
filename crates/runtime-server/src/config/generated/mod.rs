//! @generated modules — see `scripts/generate-providers.py`.

#[path = "api_provider.rs"]
mod api_provider;
#[path = "provider_defaults.rs"]
mod provider_defaults;
#[path = "provider_env.rs"]
mod provider_env;

pub use api_provider::ApiProvider;
pub use provider_defaults::*;
pub use provider_env::{PROVIDER_ENV_REGISTRY, first_nonempty_env};
