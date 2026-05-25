//! [`WorkshopHost`] — engine boundary for the large-output workshop
//! (variable store fed by the `[workshop]` config table).
//!
//! ## Why an empty marker trait?
//!
//! The live `Engine` **never invokes a method** on `workshop_vars`. The
//! only call site is `crates/tui/src/core/engine/tool_context.rs:51`:
//!
//! ```ignore
//! if let Some(workshop_cfg) = self.config.workshop.as_ref()
//!     && let Some(vars_arc) = self.workshop_vars.as_ref()
//! {
//!     let router = LargeOutputRouter::new(workshop_cfg.clone());
//!     ctx = ctx.with_large_output_router(router, vars_arc.clone());
//! }
//! ```
//!
//! Engine reads the `Arc` clone and forwards it into `ToolContext` —
//! every method on `WorkshopVariables` (`alloc_ref`, `vars_snapshot`,
//! …) is called **from inside tool implementations**
//! (`crates/tui/src/tools/large_output_router.rs`), not from Engine.
//! Per R1 (call-graph driven, not method-dump), the host trait is
//! therefore an empty marker — exactly like
//! [`ShellHost`](crate::engine::hosts::ShellHost) in M3.
//!
//! ## Field-shape role
//!
//! Introducing the marker now (per spike §2.2 target layout) lets M7
//! swap `workshop_vars: Option<Arc<Mutex<WorkshopVariables>>>` to
//! `Option<Box<dyn WorkshopHost>>` without inventing the surface
//! later. The M5 tui call site keeps the inherent `self.workshop_vars`
//! read — converting it to `&dyn WorkshopHost` would force a downcast
//! to clone the concrete `Arc<Mutex<...>>` into `ToolContext`, which
//! the marker trait deliberately cannot express.

/// Engine-side workshop host (large-output variable store).
///
/// Implemented by `crates/tui/src/tools/large_output_router.rs`'s
/// `TuiWorkshopHost(Option<Arc<Mutex<WorkshopVariables>>>)` newtype.
/// The trait is intentionally empty — see module docs for the
/// rationale.
pub trait WorkshopHost: Send + Sync {}
