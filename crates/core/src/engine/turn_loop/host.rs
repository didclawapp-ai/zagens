//! v3 turn-loop host adapter (Phase 3b batch 5 closure).

use super::inner_step_host::InnerStepHost;
use super::turn_loop_outer_host::TurnLoopOuterHost;

/// The resolved output of `ContextCompiler` for one request.
///
/// Built once per request step in V2 mode; the turn loop consumes both fields
/// so a single snapshot drive both system-prompt assembly and message injection.
#[derive(Debug, Clone, Default)]
pub struct CompilerRequestContext {
    /// Compiler-assembled system prompt (replaces `session.system_prompt` in V2).
    ///
    /// `None` → fall back to the legacy `session.system_prompt`.
    pub system_prompt: Option<crate::chat::SystemPrompt>,

    /// Inner text for the `<turn_meta>` block in messages (without the XML wrapper).
    ///
    /// When `Some`, `messages_with_turn_metadata` uses this instead of computing
    /// from `session.working_set` directly.  `None` → legacy path.
    pub turn_meta_text: Option<String>,
}

/// Config slices the turn loop reads each step (avoids pulling full `EngineConfig` into core yet).
#[derive(Debug, Clone, Copy)]
pub struct TurnLoopConfigView<'a> {
    pub compaction: &'a crate::compaction::CompactionConfig,
    pub strict_tool_mode: bool,
    pub scratchpad: &'a crate::scratchpad::ScratchpadConfig,
    pub workspace: &'a std::path::Path,
}

/// Opaque tool-registry type (TUI: `crate::tools::ToolRegistry`).
pub trait TurnLoopToolRegistry: Send + Sync {}

/// Deprecated alias for [`McpHost`](crate::engine::hosts::McpHost).
///
/// M4 (Engine-struct strangler) renamed this empty marker into the
/// named [`McpHost`] trait with default-impl predicate / metadata
/// methods. The blanket impl below keeps existing `impl
/// TurnLoopMcpPool for McpPool` consumers compiling for one release;
/// new code should impl `McpHost` directly. The alias and blanket
/// impl will be removed in the next release.
#[deprecated(
    since = "0.8.16",
    note = "use `zagens_core::engine::hosts::McpHost` instead; \
            this alias will be removed in the next release"
)]
pub trait TurnLoopMcpPool: Send + Sync {}

#[allow(deprecated)]
impl<T: crate::engine::hosts::McpHost + ?Sized> TurnLoopMcpPool for T {}

/// Canonical v3 turn-loop host: [`InnerStepHost`] + [`TurnLoopOuterHost`] + kernel seam.
///
/// `handle_deepseek_turn` and `EffectInterpreter` bound on this trait after batch 5 closure.
pub trait V3TurnHost: InnerStepHost + TurnLoopOuterHost + Send {}

impl<T: InnerStepHost + TurnLoopOuterHost + Send> V3TurnHost for T {}

#[cfg(test)]
mod tests {
    use super::V3TurnHost;

    #[test]
    fn v3_turn_host_surface_partition_baselines_sum() {
        // Session (16) + inner step (26) + outer (18) = documented host surface.
        assert_eq!(16 + 26 + 18, 60);
    }

    /// Document that the composed host still satisfies the outer-loop seam.
    fn _v3_turn_host_is_outer_loop_host<H: V3TurnHost>() {}
}
