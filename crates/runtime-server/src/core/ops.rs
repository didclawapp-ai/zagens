//! Operations submitted by the UI to the core engine.
//!
//! The definition lives in [`zagens_core::engine::op`] since M1; this file
//! is a re-export shim so existing `crate::core::ops::Op` imports keep working
//! through the M-series strangler.

pub use zagens_core::engine::op::Op;
