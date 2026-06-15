//! Session persistence (JSON + SQLite backends).

pub mod compaction_artifact_store;
pub mod context_reference;
pub mod kernel_event_log;
pub mod kernel_event_writer;
pub mod session_manager;
pub mod session_store_sqlite;

pub use compaction_artifact_store::{
    delete_compaction_artifacts_for_session, ensure_compaction_artifacts_table,
    load_compaction_artifacts, save_compaction_artifact,
};
pub use context_reference::{ContextReference, ContextReferenceKind, ContextReferenceSource};
pub use kernel_event_log::{KernelEventLog, ensure_kernel_events_table};
pub use kernel_event_writer::KernelEventWriter;
pub use session_manager::{
    SavedSession, SessionContextReference, SessionManager, SessionMetadata,
    prune_workspace_snapshots,
};
pub use session_store_sqlite::{
    delete_session_sqlite, list_sessions_sqlite, load_session_sqlite, open_sqlite_session_db,
    save_session_sqlite,
};
