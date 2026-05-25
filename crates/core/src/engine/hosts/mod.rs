//! Subsystem host traits — the engine boundary for tui-owned subsystems.
//!
//! M3 (Engine-struct strangler step) introduces these traits so the future
//! core-side `Engine` struct (M7) can hold `Box<dyn LspHost>` /
//! `Box<dyn SubAgentHost>` / etc. without taking a tui dependency. The
//! method surfaces are **strictly call-graph driven**: each method is
//! present iff the live `Engine` (`crates/tui/src/core/engine/*`) calls it.
//! Pass-through fields (Shell, Sandbox) get marker / single-accessor traits
//! so M7 only needs to swap the field type without inventing a new surface.
//!
//! M4 adds [`McpHost`], promoting the empty
//! [`TurnLoopMcpPool`](crate::engine::turn_loop::TurnLoopMcpPool) marker
//! into a named trait with default-impl `is_mcp_tool` / `tool_is_parallel_safe`
//! / `tool_is_read_only` / `tool_approval_description` methods that delegate
//! to the existing free functions in [`crate::engine::dispatch`].
//!
//! M5 adds three more host traits — [`SeamHost`] (10 methods, the
//! widest trait so far, covering the entire layered-context Flash
//! pipeline), [`WorkshopHost`] (empty marker, mirrors [`ShellHost`]
//! because Engine never invokes a method on `workshop_vars`), and
//! [`TopicMemoryHost`] (2 methods — `compose_block` /
//! `on_turn_complete`). It also moves
//! [`ScratchpadStepState`](crate::engine::ScratchpadStepState) into
//! core (~30 LOC) per spike §3 row #28 + R12, leaving the heavy
//! `scratchpad_flow.rs` flow helpers in tui.
//!
//! See [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE`](../../../../../docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)
//! §3 rows #6–#9 + #21 + #25 + #28-#31, §6 M3 / M4 / M5 rows, and §5
//! R1 for the design rationale.

pub mod lsp;
pub mod mcp;
pub mod sandbox;
pub mod seam;
pub mod shell;
pub mod subagent;
pub mod topic_memory;
pub mod workshop;

pub use lsp::LspHost;
pub use mcp::McpHost;
pub use sandbox::SandboxHost;
pub use seam::{SeamError, SeamHost};
pub use shell::ShellHost;
pub use subagent::SubAgentHost;
pub use topic_memory::TopicMemoryHost;
pub use workshop::WorkshopHost;
