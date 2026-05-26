//! Config load/merge/credentials (split from legacy `load.rs`).

mod credentials;
mod env_overrides;
mod impl_config;
mod merge;
mod model;
mod paths;

pub use credentials::*;
pub use paths::*;

#[cfg(test)]
pub(crate) use env_overrides::apply_env_overrides;
#[cfg(test)]
pub(crate) use merge::apply_profile;
#[cfg(test)]
pub(crate) use model::normalize_model_for_provider;

#[cfg(test)]
include!("tests.inc.rs");
