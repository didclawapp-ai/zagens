//! Runtime orchestrator — thread/turn persist (D16 E1-b, D17 frozen).
//!
//! Live engine orchestration (`RuntimeThreadHost` impl, spawn/monitor, turn
//! lifecycle) intentionally remains in `deepseek-runtime-server`.  Further
//! extraction is **deferred by design** (D17 Architecture Freeze) — the
//! engine, tools, and route handlers form an internally co-located unit
//! that is not a candidate for crate-level splitting.  This crate is the
//! stable boundary for SQLite thread storage, pricing aggregation, and
//! `RuntimeThreadManager` persistence helpers.

pub mod engine;
pub mod models;
pub mod pricing;
pub mod runtime_threads;
pub mod thread_store_sqlite;
pub mod usage_aggregate;
