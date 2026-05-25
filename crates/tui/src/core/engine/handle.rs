//! Engine handle — TUI-side type alias over the generic
//! [`deepseek_core::engine::handle::EngineHandle`] (M1 strangler step).
//!
//! The concrete sandbox policy and `request_user_input` payload types stay
//! in tui; this alias bakes them in so existing call sites do not change.

pub type EngineHandle = deepseek_core::engine::handle::EngineHandle<
    crate::sandbox::SandboxPolicy,
    crate::tools::user_input::UserInputResponse,
>;
