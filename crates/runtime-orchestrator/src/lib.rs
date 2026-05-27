//! Runtime orchestrator — thread/turn persist (D16 E1-b phase 1).
//!
//! Live engine orchestration (`RuntimeThreadManager`, task workers) remains in
//! `deepseek-runtime-server` until engine host boundaries are extracted.
//! Generic `EngineHandle<P, R>` and active-thread LRU state live here (`engine`,
//! `runtime_threads::active`).

pub mod engine;
pub mod models;
pub mod pricing;
pub mod runtime_threads;
pub mod thread_store_sqlite;
