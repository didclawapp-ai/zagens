//! Runtime adapter Engine wiring (M7 strangler shim).
//!
//! The struct, channels, and `Engine::with_hosts` live in `zagens-core`;
//! this module provides the sidecar builder and host injection.

#![allow(
    unused_imports,
    reason = "prelude_uses imports consumed by engine submodules via `super::*`"
)]

pub use handle::EngineHandle;
pub use types::EngineConfig;
pub use zagens_core::engine::EngineHostBundle;

use crate::config::Config;
use crate::utils::spawn_supervised;

mod accessors;
mod approval;
mod build;
mod capacity_flow;
mod compaction_ops;
mod context;
mod context_recovery;
mod context_trim;
mod continuation_ops;
mod cycle_briefing_ops;
mod cycle_hooks;
mod dispatch;
mod edit_turn_ops;
mod effect_interpreter;
mod effect_replay_anchor;
mod engine_helpers;
mod engine_v3_step;
mod handle;
mod hook_dispatch;
mod inject_steer_ops;
pub(crate) mod kernel_compaction_artifact_shadow;
pub(crate) mod kernel_compaction_replay_anchor_shadow;
pub(crate) mod kernel_continuation_anchor_shadow;
pub(crate) mod kernel_effect_shadow;
pub(crate) mod kernel_guard_shadow;
pub(crate) mod kernel_memory_plane_replay_anchor_shadow;
pub(crate) mod kernel_memory_shadow;
pub(crate) mod kernel_message_compaction_shadow;
pub(crate) mod kernel_message_coverage_shadow;
pub(crate) mod kernel_message_memory_plane_shadow;
pub(crate) mod kernel_message_role_shadow;
pub(crate) mod kernel_message_timeline_shadow;
pub(crate) mod kernel_notify_lsp_anchor_shadow;
pub(crate) mod kernel_projection_shadow;
pub(crate) mod kernel_replay_shadow;
mod kernel_resume;
pub(crate) mod kernel_resume_replay_anchor_shadow;
pub(crate) mod kernel_v3_effect_shadow;
pub(crate) mod kernel_v3_replay_counts;
mod layered_context;
mod loop_guard;
mod lsp_hooks;
mod memory_plane_ops;
mod message_handlers;
#[cfg(test)]
mod mock;
mod platform_dispatch;
mod prelude;
mod runtime_ext;
pub mod scratchpad_flow;
mod scratchpad_sync;
mod session_messages;
mod session_ops;
mod startup_warnings;
mod streaming;
mod subagent_spawn;
mod tool_catalog;
mod tool_context;
mod tool_dispatch_port;
mod tool_execution;
mod tool_setup;
pub(crate) mod turn_loop;
mod types;

#[allow(unused_imports, reason = "submodules use `super::*` from this prelude")]
use prelude::*;

include!("engine/prelude_uses.rs");

/// Sandbox/user-input–specialized core engine (M7).
#[repr(transparent)]
pub struct Engine(
    pub(crate)  zagens_core::engine::Engine<
        crate::sandbox::SandboxPolicy,
        crate::tools::user_input::UserInputResponse,
    >,
);

/// Reborrow the runtime adapter over a core engine reference (M8 op dispatch).
pub(in crate::core::engine) fn engine_from_core(
    core: &mut zagens_core::engine::Engine<
        crate::sandbox::SandboxPolicy,
        crate::tools::user_input::UserInputResponse,
    >,
) -> &mut Engine {
    // SAFETY: `Engine` is `repr(transparent)` over the core engine struct.
    unsafe { &mut *(core as *mut _ as *mut Engine) }
}

impl std::ops::Deref for Engine {
    type Target = zagens_core::engine::Engine<
        crate::sandbox::SandboxPolicy,
        crate::tools::user_input::UserInputResponse,
    >;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for Engine {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Engine {
    pub(crate) fn with_hosts(
        config: zagens_core::engine::config::EngineConfig,
        session: crate::core::session::Session,
        hosts: EngineHostBundle<
            crate::sandbox::SandboxPolicy,
            crate::tools::user_input::UserInputResponse,
        >,
    ) -> (Self, EngineHandle) {
        let (inner, handle) = zagens_core::engine::Engine::with_hosts(config, session, hosts);
        (Self(inner), handle)
    }

    pub fn new(config: EngineConfig, api_config: &Config) -> (Self, EngineHandle) {
        build::build_engine(config, api_config)
    }

    pub async fn run(self) {
        self.0.run().await;
    }
}

pub fn spawn_engine(config: EngineConfig, api_config: &Config) -> EngineHandle {
    let (engine, handle) = build::build_engine(config, api_config);
    spawn_supervised(
        "engine-event-loop",
        std::panic::Location::caller(),
        async move { engine.run().await },
    );
    handle
}

#[cfg(test)]
pub(crate) use mock::{MockApprovalEvent, MockEngineHandle, mock_engine_handle};
pub(crate) use tool_dispatch_port::RegistryToolDispatch;

#[cfg(test)]
mod tests;
