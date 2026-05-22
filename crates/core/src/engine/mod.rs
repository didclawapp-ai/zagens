//! Engine boundary (P2 PR2).
//!
//! `Session` and related state live here. The live `Engine` / `turn_loop`
//! implementation remains in `deepseek-tui::core::engine` until tool-registry
//! injection and PR3 delegation land (see `docs/tech/adr/P2_MIGRATION_SPIKE.md`).

pub use crate::session::{Session, SessionUsage};
