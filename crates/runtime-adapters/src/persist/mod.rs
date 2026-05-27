//! Session persistence (JSON + SQLite backends).

pub mod context_reference;
pub mod session_manager;
pub mod session_store_sqlite;

pub use context_reference::{
    ContextReference, ContextReferenceKind, ContextReferenceSource,
};
pub use session_manager::{
    SavedSession, SessionContextReference, SessionManager, SessionMetadata,
    prune_workspace_snapshots,
};
pub use session_store_sqlite::{
    delete_session_sqlite, list_sessions_sqlite, load_session_sqlite, open_sqlite_session_db,
    save_session_sqlite,
};
