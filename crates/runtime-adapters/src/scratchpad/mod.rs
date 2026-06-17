//! Audit scratchpad types and path-based reads (D16 E1-a2).

pub mod config;
pub mod coverage;
pub mod note_quality;
pub mod path_store;
pub mod schema;
pub mod summary;

pub use config::{ScratchpadConfig, ScratchpadConfigToml};
pub use coverage::{
    CoverageGateOutcome, CoverageStats, area_meets_deferred_quality, area_meets_done_quality,
    build_l0_status_line, compute_coverage_stats, coverage_gate, resume_area_id_from_inventory,
};
pub use path_store::{display_run_path, read_inventory, read_notes, try_open_run_dir};
pub use schema::{
    AreaStatus, Inventory, InventoryArea, NoteLine, is_high_severity, is_open_finding,
    is_verified_finding, parse_note_line,
};
pub use summary::compute_superseded_ids;
