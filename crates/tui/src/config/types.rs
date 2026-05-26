use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::features::{Features, FeaturesToml};
use crate::hooks::HooksConfig;

use super::providers::ApiProvider;

// === Types ===

/// Raw retry configuration loaded from config files.
#[derive(Debug, Clone, Deserialize)]
pub struct RetryConfig {
    pub enabled: Option<bool>,
    pub max_retries: Option<u32>,
    pub initial_delay: Option<f64>,
    pub max_delay: Option<f64>,
    pub exponential_base: Option<f64>,
}

/// UI configuration loaded from config files.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TuiConfig {
    pub alternate_screen: Option<String>,
    pub mouse_capture: Option<bool>,
    /// Timeout for startup terminal mode/probe calls in milliseconds.
    /// Defaults to 500ms when omitted.
    pub terminal_probe_timeout_ms: Option<u64>,
    /// Ordered list of footer items the user wants visible. `None` (the field
    /// missing from `config.toml`) means "use the built-in default order"; an
    /// empty `Some(vec![])` means "show nothing in the footer".
    ///
    /// Edited interactively via `/statusline`; persisted to `tui.status_items`
    /// in `~/.deepseek/config.toml`.
    pub status_items: Option<Vec<StatusItem>>,
    /// Emit OSC 8 hyperlink escape sequences around URLs in the transcript so
    /// supporting terminals (iTerm2, Terminal.app 13+, Ghostty, Kitty,
    /// WezTerm, Alacritty, recent gnome-terminal/konsole) make them
    /// Cmd+click-openable. Terminals without OSC 8 support render the plain
    /// label and ignore the escape. Defaults to `true`; set `false` for
    /// terminals that misrender the sequence.
    pub osc8_links: Option<bool>,
    /// High-level notification trigger condition. When set, overrides the
    /// `[notifications].threshold_secs` gate from the lower-level
    /// `[notifications]` block:
    ///
    /// - `Always` — fire a turn-completion notification on every successful
    ///   turn regardless of duration. The configured `[notifications].method`
    ///   and `include_summary` flag are still respected.
    /// - `Never` — suppress all turn-completion notifications.
    /// - Unset (default) — fall back to the `[notifications]` defaults.
    pub notification_condition: Option<NotificationCondition>,
}

/// High-level notification trigger override. See
/// [`TuiConfig::notification_condition`].
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationCondition {
    /// Notify on every successful turn (no duration threshold).
    Always,
    /// Suppress notifications entirely.
    Never,
}

/// Notification delivery method (mirrors `tui::notifications::Method`).
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum NotificationMethod {
    /// Auto-detect: OSC 9 for iTerm.app / Ghostty / WezTerm; BEL on
    /// macOS / Linux otherwise; on Windows the fallback is `Off`
    /// because BEL maps to the system error chime there (#583).
    #[default]
    Auto,
    /// OSC 9 escape.
    Osc9,
    /// Plain BEL character.
    Bel,
    /// Disable notifications.
    Off,
}

fn default_threshold_secs() -> u64 {
    30
}

/// Desktop-notification configuration (OSC 9 / BEL on turn completion).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct NotificationsConfig {
    /// Delivery method: `auto` | `osc9` | `bel` | `off`. Default: `auto`.
    /// `auto` resolves to OSC 9 in iTerm.app / Ghostty / WezTerm; on
    /// macOS / Linux it falls back to BEL, and on Windows it falls
    /// back to `Off` so the post-turn notification doesn't ring the
    /// system error chime (#583).
    #[serde(default)]
    pub method: NotificationMethod,
    /// Only notify when the turn took at least this many seconds. Default: 30.
    #[serde(default = "default_threshold_secs")]
    pub threshold_secs: u64,
    /// Include a short summary (elapsed time + cost) in the notification body.
    /// Default: `false`.
    #[serde(default)]
    pub include_summary: bool,
}

fn default_snapshots_enabled() -> bool {
    true
}

fn default_snapshot_max_age_days() -> u64 {
    crate::snapshot::DEFAULT_MAX_AGE.as_secs() / (24 * 60 * 60)
}

/// Workspace side-git snapshot configuration (#137).
#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotsConfig {
    /// Snapshot the workspace before and after each interactive agent turn.
    #[serde(default = "default_snapshots_enabled")]
    pub enabled: bool,
    /// Prune side-git snapshots older than this many days at session boot.
    #[serde(default = "default_snapshot_max_age_days")]
    pub max_age_days: u64,
}

impl Default for SnapshotsConfig {
    fn default() -> Self {
        Self {
            enabled: default_snapshots_enabled(),
            max_age_days: default_snapshot_max_age_days(),
        }
    }
}

/// User-level memory configuration (#489).
///
/// Default is opt-in: when this table is absent or `enabled = false`, the
/// memory file is neither read nor written, and `# foo` quick-adds in the
/// composer fall through to the normal turn-submission path.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MemoryConfig {
    /// When `true`, load the user memory file at `Config::memory_path()`
    /// into the system prompt as a `<user_memory>` block, and intercept
    /// `# foo` typed in the composer to append to that file. Default `false`.
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// `[topic_memory]` — automatic topic graph injection (B2).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct TopicMemoryConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub graph_path: Option<String>,
    #[serde(default)]
    pub inject_interval: Option<u32>,
    #[serde(default)]
    pub attribution: Option<String>,
}

/// Session file size limit (#402). Default 5 MB.
/// 0 = no limit (maps to `u64::MAX` in bytes).
#[derive(Debug, Clone, Deserialize)]
pub struct SessionConfig {
    #[serde(default = "default_session_max_file_mb")]
    pub max_file_mb: u64,
}

fn default_session_max_file_mb() -> u64 {
    5
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_file_mb: default_session_max_file_mb(),
        }
    }
}

impl SnapshotsConfig {
    #[must_use]
    pub fn max_age(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.max_age_days.saturating_mul(24 * 60 * 60))
    }
}

/// One configurable footer item.
///
/// Order in the user's `Vec<StatusItem>` is preserved: items in the left
/// cluster (`Mode`, `Model`, `Cost`, `Status`) render in the order given;
/// right-cluster chips (`Coherence`, `Agents`, `ReasoningReplay`, `Cache`,
/// `ContextPercent`, `GitBranch`, `LastToolElapsed`, `RateLimit`) likewise
/// honour ordering inside their cluster. The split between left and right is
/// deliberate — left holds steady identity (mode/model/cost), right holds
/// transient signals — so we route each variant to the correct side rather
/// than letting users reorder across the spacer.
///
/// Variants without a current data source (`RateLimit`, `LastToolElapsed`)
/// are intentionally exposed today so the picker is forward-compatible; they
/// render empty until the supporting fields land. Empty spans don't take
/// up footer width, so the user sees no visual artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StatusItem {
    /// "agent" / "yolo" / "plan" chip.
    Mode,
    /// Model identifier (e.g. `deepseek-v4-pro`).
    Model,
    /// Session cost in the configured display currency.
    Cost,
    /// Activity label: "ready" / "draft" / "working".
    Status,
    /// Coherence intervention label: "refreshing context" / "verifying" / "resetting plan".
    Coherence,
    /// Sub-agent count chip ("3 agents").
    Agents,
    /// Reasoning-replay token count ("rsn 12.3k").
    ReasoningReplay,
    /// Cache hit rate ("cache 73%").
    Cache,
    /// Context-window utilisation percent ("48%").
    ContextPercent,
    /// Current git branch name (placeholder until wired).
    GitBranch,
    /// Elapsed time of the most recent tool call (placeholder until wired).
    LastToolElapsed,
    /// Remaining rate-limit budget (placeholder until wired).
    RateLimit,
}

impl StatusItem {
    /// Default footer composition matching v0.6.6 behaviour exactly. Used when
    /// `tui.status_items` is missing from `config.toml` so upgraders see the
    /// same footer they had before.
    #[must_use]
    pub fn default_footer() -> Vec<StatusItem> {
        vec![
            StatusItem::Mode,
            StatusItem::Model,
            StatusItem::Cost,
            StatusItem::Status,
            StatusItem::Coherence,
            StatusItem::Agents,
            StatusItem::ReasoningReplay,
            StatusItem::Cache,
            StatusItem::ContextPercent,
        ]
    }

    /// Stable canonical name used in TOML and the picker label.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            StatusItem::Mode => "mode",
            StatusItem::Model => "model",
            StatusItem::Cost => "cost",
            StatusItem::Status => "status",
            StatusItem::Coherence => "coherence",
            StatusItem::Agents => "agents",
            StatusItem::ReasoningReplay => "reasoning_replay",
            StatusItem::Cache => "cache",
            StatusItem::ContextPercent => "context_percent",
            StatusItem::GitBranch => "git_branch",
            StatusItem::LastToolElapsed => "last_tool_elapsed",
            StatusItem::RateLimit => "rate_limit",
        }
    }

    /// Human-readable label for the picker.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StatusItem::Mode => "Mode",
            StatusItem::Model => "Model",
            StatusItem::Cost => "Session cost",
            StatusItem::Status => "Activity (ready/draft/working)",
            StatusItem::Coherence => "Coherence interventions",
            StatusItem::Agents => "Sub-agents in flight",
            StatusItem::ReasoningReplay => "Reasoning replay tokens",
            StatusItem::Cache => "Prompt cache hit rate",
            StatusItem::ContextPercent => "Context window %",
            StatusItem::GitBranch => "Git branch",
            StatusItem::LastToolElapsed => "Last tool elapsed",
            StatusItem::RateLimit => "Rate-limit remaining",
        }
    }

    /// One-line hint shown beside the label so the user knows what each item
    /// surfaces without having to toggle it on first.
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            StatusItem::Mode => "agent · yolo · plan",
            StatusItem::Model => "the model id you'll send to",
            StatusItem::Cost => "running total for this session",
            StatusItem::Status => "what the agent is doing right now",
            StatusItem::Coherence => "shown only when the engine intervenes",
            StatusItem::Agents => "agents or RLM work in progress",
            StatusItem::ReasoningReplay => "thinking tokens replayed each turn",
            StatusItem::Cache => "% of prompt served from cache",
            StatusItem::ContextPercent => "tokens used / model context window",
            StatusItem::GitBranch => "current branch (placeholder)",
            StatusItem::LastToolElapsed => "ms of the most recent tool call (placeholder)",
            StatusItem::RateLimit => "remaining requests in the budget (placeholder)",
        }
    }

    /// Every variant in display order — used by the picker to enumerate rows.
    #[must_use]
    pub fn all() -> &'static [StatusItem] {
        &[
            StatusItem::Mode,
            StatusItem::Model,
            StatusItem::Cost,
            StatusItem::Status,
            StatusItem::Coherence,
            StatusItem::Agents,
            StatusItem::ReasoningReplay,
            StatusItem::Cache,
            StatusItem::ContextPercent,
            StatusItem::GitBranch,
            StatusItem::LastToolElapsed,
            StatusItem::RateLimit,
        ]
    }

    /// Items that belong in the footer's left cluster (steady identity).
    #[must_use]
    pub fn is_left_cluster(self) -> bool {
        matches!(
            self,
            StatusItem::Mode | StatusItem::Model | StatusItem::Cost | StatusItem::Status
        )
    }
}

/// Resolved retry policy with defaults applied.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub enabled: bool,
    pub max_retries: u32,
    pub initial_delay: f64,
    pub max_delay: f64,
    pub exponential_base: f64,
}

/// Capacity-controller config loaded from config files/environment.
#[derive(Debug, Clone, Deserialize)]
pub struct CapacityConfig {
    pub enabled: Option<bool>,
    pub low_risk_max: Option<f64>,
    pub medium_risk_max: Option<f64>,
    pub severe_min_slack: Option<f64>,
    pub severe_violation_ratio: Option<f64>,
    pub refresh_cooldown_turns: Option<u64>,
    pub replan_cooldown_turns: Option<u64>,
    pub max_replay_per_turn: Option<usize>,
    pub min_turns_before_guardrail: Option<u64>,
    pub profile_window: Option<usize>,
    pub deepseek_v3_2_chat_prior: Option<f64>,
    pub deepseek_v3_2_reasoner_prior: Option<f64>,
    pub deepseek_v4_pro_prior: Option<f64>,
    pub deepseek_v4_flash_prior: Option<f64>,
    pub fallback_default_prior: Option<f64>,
}

impl RetryPolicy {
    /// Compute the backoff delay for a retry attempt.
    #[must_use]
    #[allow(dead_code)] // used by runtime_api; will be wired into client retry loop
    pub fn delay_for_attempt(&self, attempt: u32) -> std::time::Duration {
        let exponent = i32::try_from(attempt).unwrap_or(i32::MAX);
        let delay = self.initial_delay * self.exponential_base.powi(exponent);
        let delay = delay.min(self.max_delay);
        // Clamp to a sane range to guard against NaN/negative from misconfigured values
        let delay = delay.clamp(0.0, 300.0);
        std::time::Duration::from_secs_f64(delay)
    }
}

/// Context compaction settings (`[compaction]` in config.toml). Zagens system
/// settings and TUI `/config auto_compact` share this table when present.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CompactionConfigToml {
    /// When true, automatic compaction runs when estimated context exceeds
    /// `token_threshold` (subject to the 500K auto floor in the engine).
    #[serde(default)]
    pub auto_compact: Option<bool>,
    /// Token threshold for automatic compaction. Defaults to 80% of the
    /// active model context window when unset.
    #[serde(default)]
    pub token_threshold: Option<usize>,
}

/// Context management configuration (append-only layered context with Flash seams).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContextConfig {
    /// Master enable for layered context management. Default: false while
    /// v0.7.5 audits V4 prefix-cache behavior.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Verbatim window: last N turns never summarized. Default: 16.
    #[serde(default)]
    pub verbatim_window_turns: Option<usize>,
    /// Soft seam thresholds based on the active request input estimate.
    #[serde(default)]
    pub l1_threshold: Option<usize>,
    #[serde(default)]
    pub l2_threshold: Option<usize>,
    #[serde(default)]
    pub l3_threshold: Option<usize>,
    /// Hard cycle boundary. Default: 768000.
    #[serde(default)]
    pub cycle_threshold: Option<usize>,
    /// Model used for seam/briefing work. Default: "deepseek-v4-flash".
    #[serde(default)]
    pub seam_model: Option<String>,
    /// Per-model threshold overrides.
    #[serde(default)]
    pub per_model: Option<HashMap<String, PerModelContextConfig>>,
}

/// Sub-agent model overrides. Keys in `models` can be role names (`worker`,
/// `explorer`, `awaiter`) or type names (`general`, `explore`, `plan`,
/// `review`, `custom`). Per-call explicit model choices still win.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SubagentsConfig {
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub worker_model: Option<String>,
    #[serde(default)]
    pub explorer_model: Option<String>,
    #[serde(default)]
    pub awaiter_model: Option<String>,
    #[serde(default)]
    pub review_model: Option<String>,
    #[serde(default)]
    pub custom_model: Option<String>,
    #[serde(default)]
    pub models: Option<HashMap<String, String>>,
    /// Maximum concurrent sub-agents. Overrides the top-level max_subagents
    /// setting. Clamped to [1, MAX_SUBAGENTS].
    #[serde(default)]
    pub max_concurrent: Option<usize>,
    /// Per-step LLM API timeout for sub-agents (seconds). Used when `agent_spawn`
    /// omits `step_timeout_ms`. Default 120; runtime clamps to [10, 600].
    #[serde(default)]
    pub step_timeout_secs: Option<u64>,
}

/// Default sub-agent per-step API timeout (seconds).
pub const DEFAULT_SUBAGENT_STEP_TIMEOUT_SECS: u64 = 120;
pub const MIN_SUBAGENT_STEP_TIMEOUT_SECS: u64 = 10;
pub const MAX_SUBAGENT_STEP_TIMEOUT_SECS: u64 = 600;

/// Per-model context tuning.
#[derive(Debug, Clone, Deserialize)]
pub struct PerModelContextConfig {
    #[serde(default)]
    pub l1_threshold: Option<usize>,
    #[serde(default)]
    pub l2_threshold: Option<usize>,
    #[serde(default)]
    pub l3_threshold: Option<usize>,
    #[serde(default)]
    pub cycle_threshold: Option<usize>,
}

/// Resolved CLI configuration, including defaults and environment overrides.
#[derive(Clone, Default, Deserialize)]
pub struct Config {
    pub provider: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    /// Optional extra HTTP headers sent to model API requests.
    pub http_headers: Option<HashMap<String, String>>,
    pub default_text_model: Option<String>,
    /// DeepSeek reasoning-effort tier: `"off" | "low" | "medium" | "high" | "max"`.
    /// Defaults to `"max"` at runtime if unset.
    pub reasoning_effort: Option<String>,
    /// Cost display currency: `"usd"` | `"cny"`. Defaults to `"usd"` at runtime
    /// if unset. Consumed via `Settings.cost_currency` bridge on TUI boot.
    #[serde(default)]
    pub cost_currency: Option<String>,
    pub tools_file: Option<String>,
    pub skills_dir: Option<String>,
    pub mcp_config_path: Option<String>,
    pub notes_path: Option<String>,
    pub memory_path: Option<String>,
    /// When true, set `tool_choice: "required"` in all API requests so the
    /// model MUST call a tool on every step (V4 strict tool-following mode).
    pub strict_tool_mode: Option<bool>,
    /// Additional system-prompt sources concatenated in declared order
    /// (#454). Paths are expanded via `expand_path` so `~` and env
    /// vars work. Project config overrides user config (replace, not
    /// merge) — that's the typical "this repo needs X plus everything
    /// I already have" pattern, where users put `~/global.md` in the
    /// project's array if they want both. Each file is loaded, capped
    /// at 100 KiB, and skipped (with a warning) on read errors so a
    /// missing optional file doesn't fail the launch.
    pub instructions: Option<Vec<String>>,
    pub allow_shell: Option<bool>,
    pub approval_policy: Option<String>,
    pub sandbox_mode: Option<String>,
    /// External sandbox backend: `"none"` or `"opensandbox"`.
    /// When set, exec_shell routes commands through the backend's HTTP API
    /// instead of spawning a local process.
    pub sandbox_backend: Option<String>,
    /// Base URL for the external sandbox backend (default: `"http://localhost:8080"`).
    pub sandbox_url: Option<String>,
    /// Optional API key for the external sandbox backend (sent as Bearer token).
    pub sandbox_api_key: Option<String>,
    pub managed_config_path: Option<String>,
    pub requirements_path: Option<String>,
    pub max_subagents: Option<usize>,
    pub retry: Option<RetryConfig>,
    pub capacity: Option<CapacityConfig>,
    pub features: Option<FeaturesToml>,

    /// TUI configuration (alternate screen, etc.)
    pub tui: Option<TuiConfig>,

    /// Lifecycle hooks configuration
    #[serde(default)]
    pub hooks: Option<HooksConfig>,

    /// Provider-specific credentials and defaults shared with the `deepseek` facade.
    #[serde(default)]
    pub providers: Option<ProvidersConfig>,

    /// Vision bridge for the `describe_image` tool — enables image→text
    /// extraction via an external vision model (e.g. SiliconFlow Qwen3-VL).
    #[serde(default)]
    pub vision: Option<VisionConfig>,

    /// Desktop notification settings (OSC 9 / BEL on long turn completion).
    #[serde(default)]
    pub notifications: Option<NotificationsConfig>,

    /// Per-domain network policy (#135). When absent, network tools fall back
    /// to a permissive default that mirrors pre-v0.7.0 behavior.
    #[serde(default)]
    pub network: Option<NetworkPolicyToml>,

    /// Community skill installer settings (#140). When absent, installer
    /// commands fall back to the bundled defaults
    /// ([`crate::skills::install::DEFAULT_REGISTRY_URL`] +
    /// [`crate::skills::install::DEFAULT_MAX_SIZE_BYTES`]).
    #[serde(default)]
    pub skills: Option<SkillsConfig>,

    /// Workspace side-git snapshots (#137). Defaults to enabled with 7-day
    /// retention when the table is absent.
    #[serde(default)]
    pub snapshots: Option<SnapshotsConfig>,

    /// User-level memory file (#489). Default behaviour is **opt-in**:
    /// loading + injection happens only when `[memory] enabled = true` or
    /// `DEEPSEEK_MEMORY=on` is set.
    #[serde(default)]
    pub memory: Option<MemoryConfig>,

    /// Topic memory graph (B2). Opt-in via `[topic_memory] enabled = true`.
    #[serde(default)]
    pub topic_memory: Option<TopicMemoryConfig>,

    /// Session file size limit (#402). Default 5 MB.
    /// 0 = no limit (maps to `u64::MAX` in bytes).
    #[serde(default)]
    pub session: Option<SessionConfig>,

    /// Post-edit LSP diagnostics injection (#136). When absent, the engine
    /// applies the defaults documented in [`LspConfigToml`].
    #[serde(default)]
    pub lsp: Option<LspConfigToml>,

    /// Append-only layered context management with Flash seam manager (#159).
    #[serde(default)]
    pub context: ContextConfig,

    /// Sub-agent model overrides.
    #[serde(default)]
    pub subagents: Option<SubagentsConfig>,

    /// Runtime API server tuning (`deepseek serve --http`). Currently only
    /// hosts the CORS allow-list extension (whalescale#255 / #561). When the
    /// table is absent, the daemon ships with localhost:3000 / localhost:1420
    /// / tauri://localhost as the only allowed dev origins.
    #[serde(default)]
    pub runtime_api: Option<RuntimeApiConfig>,

    /// Workshop / large-tool-output routing (#548). When absent, the global
    /// default threshold of 4 096 tokens applies and routing is active.
    #[serde(default)]
    pub workshop: Option<crate::tools::large_output_router::WorkshopConfig>,

    /// Full-repo audit scratchpad tools and engine hooks (Phase B).
    #[serde(default)]
    pub scratchpad: Option<crate::scratchpad::ScratchpadConfigToml>,

    /// Automatic context compaction (`[compaction]` table).
    #[serde(default)]
    pub compaction: Option<CompactionConfigToml>,
}

/// `[runtime_api]` table — knobs for the local HTTP/SSE daemon.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct RuntimeApiConfig {
    /// Additional CORS origins to allow on top of the built-in defaults
    /// (`http://localhost:{3000,1420}`, `http://127.0.0.1:{3000,1420}`,
    /// `tauri://localhost`). Useful when developing a UI against a non-default
    /// dev server port (e.g. Vite's default `:5173`).
    ///
    /// Resolution order (highest priority first): `--cors-origin` CLI flag,
    /// `DEEPSEEK_CORS_ORIGINS` env var (comma-separated), this field. Whalescale#255 / #561.
    #[serde(default)]
    pub cors_origins: Option<Vec<String>>,
}

/// `[skills]` table — knobs for the community-skill installer.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillsConfig {
    /// Curated registry index. `/skill install <name>` looks up the spec here.
    /// Defaults to [`crate::skills::install::DEFAULT_REGISTRY_URL`].
    #[serde(default)]
    pub registry_url: Option<String>,
    /// Per-skill maximum *uncompressed* size in bytes. Tarballs that exceed
    /// this limit are rejected during validation. Defaults to 5 MiB.
    #[serde(default)]
    pub max_install_size_bytes: Option<u64>,
}

impl SkillsConfig {
    /// Resolve the registry URL with the bundled default.
    #[must_use]
    pub fn registry_url(&self) -> String {
        self.registry_url
            .clone()
            .unwrap_or_else(|| crate::skills::install::DEFAULT_REGISTRY_URL.to_string())
    }

    /// Resolve the max install size with the bundled default.
    #[must_use]
    pub fn max_install_size_bytes(&self) -> u64 {
        self.max_install_size_bytes
            .unwrap_or(crate::skills::install::DEFAULT_MAX_SIZE_BYTES)
    }
}

/// `[network]` table — mirrors `deepseek_config::NetworkPolicyToml` so the live
/// TUI runtime can construct a [`crate::network_policy::NetworkPolicy`]
/// without reaching into the workspace config crate. See `config.example.toml`
/// for documentation.
#[derive(Debug, Clone, Deserialize)]
pub struct NetworkPolicyToml {
    /// Decision for hosts that are not in `allow` or `deny`. One of
    /// `"allow" | "deny" | "prompt"`. Defaults to `"prompt"`.
    #[serde(default = "default_network_decision")]
    pub default: String,
    /// Hosts that are always allowed. Subdomain rules: a leading dot
    /// (`.example.com`) matches subdomains but not the apex.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Hosts that are always denied. Deny entries win over allow entries.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Whether to record one audit-log line per outbound network call.
    #[serde(default = "default_network_audit")]
    pub audit: bool,
}

fn default_network_decision() -> String {
    "prompt".to_string()
}

fn default_network_audit() -> bool {
    true
}

impl Default for NetworkPolicyToml {
    fn default() -> Self {
        Self {
            default: default_network_decision(),
            allow: Vec::new(),
            deny: Vec::new(),
            audit: default_network_audit(),
        }
    }
}

impl NetworkPolicyToml {
    /// Build a runtime [`crate::network_policy::NetworkPolicy`] from the
    /// on-disk schema.
    #[must_use]
    pub fn into_runtime(self) -> crate::network_policy::NetworkPolicy {
        crate::network_policy::NetworkPolicy {
            default: crate::network_policy::Decision::parse(&self.default).into(),
            allow: self.allow,
            deny: self.deny,
            audit: self.audit,
        }
    }
}

/// `[lsp]` table — mirrors [`crate::lsp::LspConfig`]. Documented in
/// `config.example.toml`. When omitted, defaults from `LspConfig::default()`
/// apply (enabled, 5 s poll, 20 diagnostics/file, errors only, no overrides).
#[derive(Debug, Clone, Deserialize, Default)]
pub struct LspConfigToml {
    /// Master switch. Defaults to `true`.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// How long to wait for the LSP server to publish diagnostics after a
    /// `didOpen`/`didChange`. Defaults to 5000 ms.
    #[serde(default)]
    pub poll_after_edit_ms: Option<u64>,
    /// Cap on diagnostics surfaced per file. Defaults to 20.
    #[serde(default)]
    pub max_diagnostics_per_file: Option<usize>,
    /// Whether to surface warnings in addition to errors. Defaults to `false`.
    #[serde(default)]
    pub include_warnings: Option<bool>,
    /// Optional override for the `Language -> [cmd, ...args]` table. Keys
    /// are language slugs (`"rust"`, `"go"`, etc.).
    #[serde(default)]
    pub servers: Option<HashMap<String, Vec<String>>>,
}

impl LspConfigToml {
    /// Build a runtime [`crate::lsp::LspConfig`] from the on-disk schema,
    /// falling back to defaults for any unset fields.
    #[must_use]
    pub fn into_runtime(self) -> crate::lsp::LspConfig {
        let defaults = crate::lsp::LspConfig::default();
        crate::lsp::LspConfig {
            enabled: self.enabled.unwrap_or(defaults.enabled),
            poll_after_edit_ms: self
                .poll_after_edit_ms
                .unwrap_or(defaults.poll_after_edit_ms),
            max_diagnostics_per_file: self
                .max_diagnostics_per_file
                .unwrap_or(defaults.max_diagnostics_per_file),
            include_warnings: self.include_warnings.unwrap_or(defaults.include_warnings),
            servers: self.servers.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub http_headers: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VisionConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default)]
    pub deepseek: ProviderConfig,
    #[serde(default)]
    pub deepseek_cn: ProviderConfig,
    #[serde(default)]
    pub nvidia_nim: ProviderConfig,
    #[serde(default)]
    pub openrouter: ProviderConfig,
    #[serde(default)]
    pub novita: ProviderConfig,
    #[serde(default)]
    pub fireworks: ProviderConfig,
    #[serde(default)]
    pub sglang: ProviderConfig,
    #[serde(default)]
    pub vllm: ProviderConfig,
    #[serde(default)]
    pub ollama: ProviderConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ConfigFile {
    #[serde(flatten)]
    pub(crate) base: Config,
    pub(crate) profiles: Option<HashMap<String, Config>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct RequirementsFile {
    #[serde(default)]
    pub(crate) allowed_approval_policies: Vec<String>,
    #[serde(default)]
    pub(crate) allowed_sandbox_modes: Vec<String>,
}

fn debug_redact_secret(value: &Option<String>) -> &'static str {
    if value.as_ref().is_some_and(|s| !s.is_empty()) {
        "<redacted>"
    } else {
        "None"
    }
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("provider", &self.provider)
            .field("api_key", &debug_redact_secret(&self.api_key))
            .field("sandbox_api_key", &debug_redact_secret(&self.sandbox_api_key))
            .field("default_text_model", &self.default_text_model)
            .field("allow_shell", &self.allow_shell)
            .field("approval_policy", &self.approval_policy)
            .field("sandbox_mode", &self.sandbox_mode)
            .finish_non_exhaustive()
    }
}

