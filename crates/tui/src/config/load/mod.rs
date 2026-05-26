//! Config load/merge/credentials (split from legacy `load.rs`).

mod credentials;
mod env_overrides;
mod impl_config;
mod merge;
mod model;
mod paths;

pub use credentials::*;
pub use env_overrides::*;
pub use merge::*;
pub use model::*;
pub use paths::*;

#[cfg(test)]
include!("tests.inc.rs");
