//! Engine configuration (`EngineConfig` + defaults).

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
            llm_client_override: None,
        }
    }
}
