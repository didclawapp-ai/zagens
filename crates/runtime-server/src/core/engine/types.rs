//! Engine configuration — runtime-side **facade** over the lean core
//! [`deepseek_core::engine::config::EngineConfig`].
//!
//! The fields remain laid out flat here so existing callers
//! (`runtime_threads/engine_spawn.rs`, tests, etc.) keep compiling. The
//! `lean()` / `ext_ref()` / `into_parts()` accessors carve the
//! configuration into the (core lean ⊕ runtime ext) shape used by
//! `Engine::with_hosts`.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::compaction::CompactionConfig;
use crate::config::{DEFAULT_MAX_SUBAGENTS, DEFAULT_TEXT_MODEL};
use crate::cycle_manager::CycleConfig;
use crate::features::Features;
use crate::tools::plan::{SharedPlanState, new_shared_plan_state};
use crate::tools::spec::RuntimeToolServices;
use crate::tools::todo::{SharedTodoList, new_shared_todo_list};

use crate::core::capacity::CapacityControllerConfig;

/// Runtime-side carve-out: fields whose types are owned by the sidecar
/// (`NetworkPolicyDecider`, `LspConfig`, `WorkshopConfig`,
/// `TopicMemorySettings`, `RuntimeToolServices`) plus shared-state pointers.
#[allow(dead_code, reason = "M2 type pillar — consumed via EngineConfig facade")]
#[derive(Clone)]
pub struct EngineConfigExt {
    /// Shared Todo list state.
    pub todos: SharedTodoList,
    /// Shared Plan state.
    pub plan_state: SharedPlanState,
    /// Per-domain network policy decider (#135).
    pub network_policy: Option<crate::network_policy::NetworkPolicyDecider>,
    /// Post-edit LSP diagnostics injection (#136).
    pub lsp_config: Option<crate::lsp::LspConfig>,
    /// Durable runtime services exposed to model-visible tools.
    pub runtime_services: RuntimeToolServices,
    /// Topic memory graph settings (B2).
    pub topic_memory: crate::topic_memory::TopicMemorySettings,
    /// Workshop / large-tool-output routing (#548). `None` disables routing.
    pub workshop: Option<crate::tools::large_output_router::WorkshopConfig>,
    /// Test/dev override: skip `DeepSeekClient::new` and use this client.
    #[doc(hidden)]
    pub llm_client_override: Option<std::sync::Arc<dyn crate::llm_client::LlmClient>>,
}

impl Default for EngineConfigExt {
    fn default() -> Self {
        Self {
            todos: new_shared_todo_list(),
            plan_state: new_shared_plan_state(),
            network_policy: None,
            lsp_config: None,
            runtime_services: RuntimeToolServices::default(),
            topic_memory: crate::topic_memory::TopicMemorySettings::default(),
            workshop: None,
            llm_client_override: None,
        }
    }
}

/// Configuration for the engine
#[derive(Clone)]
pub struct EngineConfig {
    /// Model identifier to use for responses.
    pub model: String,
    /// Workspace root for tool execution and file operations.
    pub workspace: PathBuf,
    /// Allow shell tool execution when true.
    pub allow_shell: bool,
    /// Enable trust mode (skip approvals) when true.
    pub trust_mode: bool,
    /// Path to the notes file used by the notes tool.
    pub notes_path: PathBuf,
    /// Path to the MCP configuration file.
    pub mcp_config_path: PathBuf,
    /// Directory containing discoverable skills.
    pub skills_dir: PathBuf,
    /// Additional instruction files concatenated into the system
    /// prompt (#454). Loaded in declared order from the user's
    /// `instructions = [...]` config (or the per-project override).
    /// Resolved via `expand_path` so `~` works.
    pub instructions: Vec<PathBuf>,
    /// Maximum number of assistant steps before stopping.
    pub max_steps: u32,
    /// Maximum number of concurrently active subagents.
    pub max_subagents: usize,
    /// Per-step sub-agent LLM API timeout (from `[subagents] step_timeout_secs`).
    pub subagent_step_timeout: std::time::Duration,
    /// Feature flags controlling tool availability.
    pub features: Features,
    /// Auto-compaction settings for long conversations.
    ///
    /// As of v0.6.6 the high-level summarization compaction (`compact_messages_safe`)
    /// is **disabled by default**; the checkpoint-restart cycle architecture
    /// (`cycle_manager`) replaces it. The compaction config is still wired through
    /// for the per-tool-result truncation path (`compact_tool_result_for_context`)
    /// and for users who explicitly opt back in through the `auto_compact`
    /// setting or a direct engine config.
    pub compaction: CompactionConfig,
    /// Checkpoint-restart cycle settings (issue #124).
    pub cycle: CycleConfig,
    /// Capacity-controller settings.
    pub capacity: CapacityControllerConfig,
    /// Shared Todo list state.
    pub todos: SharedTodoList,
    /// Shared Plan state.
    pub plan_state: SharedPlanState,
    /// Maximum sub-agent recursion depth (default 3). See
    /// `SubAgentRuntime::max_spawn_depth`. Override via
    /// `[runtime] max_spawn_depth = N` in `~/.deepseek/config.toml`.
    pub max_spawn_depth: u32,
    /// Per-domain network policy decider (#135). Shared across the session so
    /// session-scoped approvals (`/network allow <host>`) persist for the
    /// remainder of the run.
    pub network_policy: Option<crate::network_policy::NetworkPolicyDecider>,
    /// Whether to take side-git workspace snapshots before/after each turn.
    pub snapshots_enabled: bool,
    /// Post-edit LSP diagnostics injection (#136). When `None`, the engine
    /// constructs a disabled manager so the field is always present.
    pub lsp_config: Option<crate::lsp::LspConfig>,
    /// Durable runtime services exposed to model-visible tools.
    pub runtime_services: RuntimeToolServices,
    /// Per-role/type sub-agent model overrides already resolved from config.
    pub subagent_model_overrides: HashMap<String, String>,
    /// Whether the user-memory feature is enabled (#489). When `true` the
    /// engine reads `memory_path` on each prompt assembly and prepends a
    /// `<user_memory>` block to the system prompt.
    pub memory_enabled: bool,
    /// Path to the user memory file (#489). Always populated; only
    /// consulted when `memory_enabled` is `true`.
    pub memory_path: PathBuf,
    /// Topic memory graph settings (B2).
    pub topic_memory: crate::topic_memory::TopicMemorySettings,
    pub goal_objective: Option<String>,
    /// Resolved BCP-47 locale tag (e.g. `"en"`, `"zh-Hans"`, `"ja"`)
    /// for the `## Environment` block in the system prompt. The
    /// caller resolves this from `Settings` once at engine
    /// construction; the engine never touches disk for it.
    pub locale_tag: String,
    /// When true, force `tool_choice: "required"` so the model always calls
    /// a tool on every turn step (V4 strict tool-following mode).
    pub strict_tool_mode: bool,
    /// Office vs Code task surface (session-fixed).
    pub task_type: crate::task_type::TaskType,
    /// Workshop / large-tool-output routing (#548). `None` disables routing.
    pub workshop: Option<crate::tools::large_output_router::WorkshopConfig>,
    /// Audit scratchpad engine hooks (Phase B).
    pub scratchpad: crate::scratchpad::ScratchpadConfig,
    /// Long-horizon code task harness (LHT Phase 1).
    pub long_horizon: deepseek_core::long_horizon::LongHorizonConfig,
    /// Test/dev override: skip `DeepSeekClient::new` and use this client instead.
    #[doc(hidden)]
    pub llm_client_override: Option<std::sync::Arc<dyn crate::llm_client::LlmClient>>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            model: DEFAULT_TEXT_MODEL.to_string(),
            workspace: PathBuf::from("."),
            allow_shell: true,
            trust_mode: false,
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            skills_dir: crate::skills::default_skills_dir(),
            instructions: Vec::new(),
            max_steps: 100,
            max_subagents: DEFAULT_MAX_SUBAGENTS,
            subagent_step_timeout: std::time::Duration::from_secs(
                crate::config::DEFAULT_SUBAGENT_STEP_TIMEOUT_SECS,
            ),
            features: Features::with_defaults(),
            compaction: CompactionConfig::default(),
            cycle: CycleConfig::default(),
            capacity: CapacityControllerConfig::default(),
            todos: new_shared_todo_list(),
            plan_state: new_shared_plan_state(),
            max_spawn_depth: crate::tools::subagent::DEFAULT_MAX_SPAWN_DEPTH,
            network_policy: None,
            snapshots_enabled: true,
            lsp_config: None,
            runtime_services: RuntimeToolServices::default(),
            subagent_model_overrides: HashMap::new(),
            memory_enabled: false,
            memory_path: PathBuf::from("./memory.md"),
            topic_memory: crate::topic_memory::TopicMemorySettings::default(),
            strict_tool_mode: false,
            goal_objective: None,
            locale_tag: "en".to_string(),
            task_type: crate::task_type::TaskType::default(),
            workshop: None,
            scratchpad: crate::scratchpad::ScratchpadConfig::default(),
            long_horizon: deepseek_core::long_horizon::LongHorizonConfig::default(),
            llm_client_override: None,
        }
    }
}

impl EngineConfig {
    /// Project the lean subset (25 core-friendly fields) into the core
    /// [`deepseek_core::engine::config::EngineConfig`]. Allocates fresh
    /// `String` / `Vec` / `HashMap` clones; cheap relative to the
    /// per-turn work that follows.
    ///
    /// **Usage discipline:** today no production code consumes the result
    /// (the live `Engine` still reads from `tui::EngineConfig` directly);
    /// M7 will switch the entry point and immediately call `lean()` /
    /// `into_parts()` from the new `Engine::with_hosts(...)` builder.
    #[must_use]
    pub fn lean(&self) -> deepseek_core::engine::config::EngineConfig {
        deepseek_core::engine::config::EngineConfig {
            model: self.model.clone(),
            workspace: self.workspace.clone(),
            allow_shell: self.allow_shell,
            trust_mode: self.trust_mode,
            notes_path: self.notes_path.clone(),
            mcp_config_path: self.mcp_config_path.clone(),
            skills_dir: self.skills_dir.clone(),
            instructions: self.instructions.clone(),
            max_steps: self.max_steps,
            max_subagents: self.max_subagents,
            subagent_step_timeout: self.subagent_step_timeout,
            features: self.features.clone(),
            compaction: self.compaction.clone(),
            cycle: self.cycle.clone(),
            capacity: self.capacity.clone(),
            max_spawn_depth: self.max_spawn_depth,
            snapshots_enabled: self.snapshots_enabled,
            subagent_model_overrides: self.subagent_model_overrides.clone(),
            memory_enabled: self.memory_enabled,
            memory_path: self.memory_path.clone(),
            goal_objective: self.goal_objective.clone(),
            locale_tag: self.locale_tag.clone(),
            strict_tool_mode: self.strict_tool_mode,
            task_type: self.task_type,
            scratchpad: self.scratchpad.clone(),
            long_horizon: self.long_horizon.clone(),
        }
    }

    /// Snapshot the tui-only subset into a fresh [`EngineConfigExt`].
    /// Borrows internally where possible; `clone`s `Arc`s (cheap).
    #[must_use]
    pub fn ext(&self) -> EngineConfigExt {
        EngineConfigExt {
            todos: self.todos.clone(),
            plan_state: self.plan_state.clone(),
            network_policy: self.network_policy.clone(),
            lsp_config: self.lsp_config.clone(),
            runtime_services: self.runtime_services.clone(),
            topic_memory: self.topic_memory.clone(),
            workshop: self.workshop.clone(),
            llm_client_override: self.llm_client_override.clone(),
        }
    }

    /// Consume the facade and produce `(lean, ext)` for the future
    /// core-side `Engine::with_hosts(lean, ext)` entry point (M7).
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (deepseek_core::engine::config::EngineConfig, EngineConfigExt) {
        let lean = deepseek_core::engine::config::EngineConfig {
            model: self.model,
            workspace: self.workspace,
            allow_shell: self.allow_shell,
            trust_mode: self.trust_mode,
            notes_path: self.notes_path,
            mcp_config_path: self.mcp_config_path,
            skills_dir: self.skills_dir,
            instructions: self.instructions,
            max_steps: self.max_steps,
            max_subagents: self.max_subagents,
            subagent_step_timeout: self.subagent_step_timeout,
            features: self.features,
            compaction: self.compaction,
            cycle: self.cycle,
            capacity: self.capacity,
            max_spawn_depth: self.max_spawn_depth,
            snapshots_enabled: self.snapshots_enabled,
            subagent_model_overrides: self.subagent_model_overrides,
            memory_enabled: self.memory_enabled,
            memory_path: self.memory_path,
            goal_objective: self.goal_objective,
            locale_tag: self.locale_tag,
            strict_tool_mode: self.strict_tool_mode,
            task_type: self.task_type,
            scratchpad: self.scratchpad,
            long_horizon: self.long_horizon,
        };
        let ext = EngineConfigExt {
            todos: self.todos,
            plan_state: self.plan_state,
            network_policy: self.network_policy,
            lsp_config: self.lsp_config,
            runtime_services: self.runtime_services,
            topic_memory: self.topic_memory,
            workshop: self.workshop,
            llm_client_override: self.llm_client_override,
        };
        (lean, ext)
    }

    /// Rebuild the facade from a `(lean, ext)` pair — the inverse of
    /// `into_parts`. Useful for unit-testing the projection roundtrip
    /// and for the future M7 entry point that constructs an
    /// `EngineConfig` from caller-supplied core + ext values.
    #[must_use]
    pub fn from_parts(
        lean: deepseek_core::engine::config::EngineConfig,
        ext: EngineConfigExt,
    ) -> Self {
        Self {
            model: lean.model,
            workspace: lean.workspace,
            allow_shell: lean.allow_shell,
            trust_mode: lean.trust_mode,
            notes_path: lean.notes_path,
            mcp_config_path: lean.mcp_config_path,
            skills_dir: lean.skills_dir,
            instructions: lean.instructions,
            max_steps: lean.max_steps,
            max_subagents: lean.max_subagents,
            subagent_step_timeout: lean.subagent_step_timeout,
            features: lean.features,
            compaction: lean.compaction,
            cycle: lean.cycle,
            capacity: lean.capacity,
            todos: ext.todos,
            plan_state: ext.plan_state,
            max_spawn_depth: lean.max_spawn_depth,
            network_policy: ext.network_policy,
            snapshots_enabled: lean.snapshots_enabled,
            lsp_config: ext.lsp_config,
            runtime_services: ext.runtime_services,
            subagent_model_overrides: lean.subagent_model_overrides,
            memory_enabled: lean.memory_enabled,
            memory_path: lean.memory_path,
            topic_memory: ext.topic_memory,
            strict_tool_mode: lean.strict_tool_mode,
            goal_objective: lean.goal_objective,
            locale_tag: lean.locale_tag,
            task_type: lean.task_type,
            workshop: ext.workshop,
            scratchpad: lean.scratchpad,
            long_horizon: lean.long_horizon,
            llm_client_override: ext.llm_client_override,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip property: `from_parts(into_parts(c))` equals `c` on the
    /// projected fields. Guarantees the M2 type pillars stay aligned
    /// with the live facade as fields evolve. (`SharedTodoList` /
    /// `SharedPlanState` use pointer identity, so we compare a small
    /// subset of scalar fields rather than full struct equality.)
    #[test]
    fn lean_into_parts_round_trip() {
        let original = EngineConfig {
            model: "deepseek-roundtrip".to_string(),
            allow_shell: false,
            trust_mode: true,
            max_steps: 42,
            locale_tag: "zh-Hans".to_string(),
            strict_tool_mode: true,
            ..EngineConfig::default()
        };
        let (lean, ext) = original.clone().into_parts();
        let rebuilt = EngineConfig::from_parts(lean, ext);

        assert_eq!(rebuilt.model, original.model);
        assert_eq!(rebuilt.allow_shell, original.allow_shell);
        assert_eq!(rebuilt.trust_mode, original.trust_mode);
        assert_eq!(rebuilt.max_steps, original.max_steps);
        assert_eq!(rebuilt.locale_tag, original.locale_tag);
        assert_eq!(rebuilt.strict_tool_mode, original.strict_tool_mode);
    }

    /// Sanity: `lean()` and `into_parts().0` agree.
    #[test]
    fn lean_borrow_matches_into_parts_owned() {
        let cfg = EngineConfig {
            model: "deepseek-lean".to_string(),
            max_steps: 7,
            ..EngineConfig::default()
        };
        let by_ref = cfg.lean();
        let (by_value, _ext) = cfg.into_parts();
        assert_eq!(by_ref.model, by_value.model);
        assert_eq!(by_ref.max_steps, by_value.max_steps);
    }
}
