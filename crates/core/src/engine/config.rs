//! Engine configuration — **lean** subset that depends only on core types
//! (M2 strangler step of the `Engine` struct → `deepseek-core` migration).
//!
//! See [`PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE`](../../../../docs/tech/adr/PR_M0_ENGINE_STRUCT_TO_CORE_SPIKE.md)
//! §3 row #1 and §5 R2 for the design rationale. The tui crate keeps a fat
//! facade `EngineConfig` whose accessor `lean()` / `into_parts()` produce
//! a value of **this** struct plus an `EngineConfigExt` carrying the
//! tui-only fields. When the Engine struct itself moves into core (M7),
//! `Engine::with_hosts(lean, ext_via_host)` will replace the current
//! `Engine::new(tui::EngineConfig)` entry point.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::capacity::CapacityControllerConfig;
use crate::compaction::CompactionConfig;
use crate::cycle::CycleConfig;
use crate::features::Features;
use crate::scratchpad::ScratchpadConfig;
use crate::task_type::TaskType;

/// Plain-types / core-types subset of the live engine configuration.
///
/// Fields are intentionally `pub` so the tui facade can build it field by
/// field; future core-only callers will go through builder methods.
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
    /// Additional instruction files concatenated into the system prompt
    /// (deepseek-tui #454). Loaded in declared order from the user's
    /// `instructions = [...]` config (or the per-project override). The
    /// caller is responsible for `expand_path`-style `~` substitution
    /// before constructing this list — `deepseek-core` does no disk I/O
    /// on these paths.
    pub instructions: Vec<PathBuf>,
    /// Maximum number of assistant steps before stopping.
    pub max_steps: u32,
    /// Maximum number of concurrently active subagents.
    pub max_subagents: usize,
    /// Per-step sub-agent LLM API timeout.
    pub subagent_step_timeout: Duration,
    /// Feature flags controlling tool availability.
    pub features: Features,
    /// Auto-compaction settings for long conversations.
    pub compaction: CompactionConfig,
    /// Checkpoint-restart cycle settings (#124).
    pub cycle: CycleConfig,
    /// Capacity-controller settings.
    pub capacity: CapacityControllerConfig,
    /// Maximum sub-agent recursion depth.
    pub max_spawn_depth: u32,
    /// Whether to take side-git workspace snapshots before/after each turn.
    pub snapshots_enabled: bool,
    /// Per-role/type sub-agent model overrides already resolved from config.
    pub subagent_model_overrides: HashMap<String, String>,
    /// Whether the user-memory feature is enabled (#489).
    pub memory_enabled: bool,
    /// Path to the user memory file (#489).
    pub memory_path: PathBuf,
    /// Long-horizon goal seed (#CRAFT) injected into the system prompt.
    pub goal_objective: Option<String>,
    /// Resolved BCP-47 locale tag (e.g. `"en"`, `"zh-Hans"`, `"ja"`).
    pub locale_tag: String,
    /// When true, force `tool_choice: "required"`.
    pub strict_tool_mode: bool,
    /// Office vs Code task surface (session-fixed).
    pub task_type: TaskType,
    /// Audit scratchpad engine hooks (Phase B).
    pub scratchpad: ScratchpadConfig,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            workspace: PathBuf::from("."),
            allow_shell: true,
            trust_mode: false,
            notes_path: PathBuf::from("notes.txt"),
            mcp_config_path: PathBuf::from("mcp.json"),
            // Disk-dependent paths get a placeholder; the tui facade fills
            // them in via `default_skills_dir()`. core-only callers must
            // override before use.
            skills_dir: PathBuf::new(),
            instructions: Vec::new(),
            max_steps: 100,
            max_subagents: 0,
            subagent_step_timeout: Duration::from_secs(0),
            features: Features::with_defaults(),
            compaction: CompactionConfig::default(),
            cycle: CycleConfig::default(),
            capacity: CapacityControllerConfig::default(),
            max_spawn_depth: 0,
            snapshots_enabled: true,
            subagent_model_overrides: HashMap::new(),
            memory_enabled: false,
            memory_path: PathBuf::from("./memory.md"),
            goal_objective: None,
            locale_tag: "en".to_string(),
            strict_tool_mode: false,
            task_type: TaskType::default(),
            scratchpad: ScratchpadConfig::default(),
        }
    }
}
