//! H3 harness engine boundary (Phase 3.1).
//!
//! Stable surface for registry filtering, verify-loop, state paths, and T1 telemetry.

pub mod affected_tests;
pub mod hints;
pub mod registry_surface;
pub mod state;
pub mod symbol_search;
pub mod telemetry;
pub mod verify_loop;

pub use affected_tests::{
    AffectedTestSuggestion, hint_suffix_for_paths, hint_suffix_for_tool, is_edit_tool,
    suggest_for_edited_paths,
};
pub use symbol_search::{SymbolSearchHit, SymbolSearchResult, search_workspace_symbols};

pub use zagens_core::engine::{
    REPLAY_PACK_SCHEMA_VERSION, ReplayPack, ReplayPackMetadata, ReplayPackValidation,
    build_replay_pack_from_fixture, parse_replay_pack_json, validate_replay_pack,
};

pub use hints::{ToolHintAudit, audit_tool, audit_tools};
pub use registry_surface::RegistrySurface;
pub use state::{HarnessStateAdapter, WorkspaceHarnessState};
pub use telemetry::{
    ToolHintAuditEntry, ToolStat, ToolTelemetryReport, build_tool_telemetry_report,
    default_sessions_db_path,
};
pub use verify_loop::{
    HarnessVerifyLoop, HarnessVerifyLoopConfig, HarnessVerifyOutcome, HarnessVerifyRecord,
    VerifyStageSpec,
};
