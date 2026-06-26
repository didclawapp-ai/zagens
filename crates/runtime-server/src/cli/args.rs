//! CLI argument types (`clap`) — B3 split from `main.rs`.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::config::Config;

#[derive(Parser, Debug)]
#[command(
    name = "zagens",
    author,
    version,
    about = "Zagens headless CLI for DeepSeek agent runtime",
    long_about = "Scriptable CLI for the Zagens agent runtime.\n\nRun `zagens exec '…'` for one-shot tasks, `zagens doctor` for diagnostics, or `zagens serve --http` for the local API.\n\nNot affiliated with DeepSeek Inc."
)]
pub struct Cli {
    /// Subcommand to run
    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub feature_toggles: FeatureToggles,

    /// Send a one-shot prompt (non-interactive)
    #[arg(short, long)]
    pub prompt: Option<String>,

    /// YOLO mode: enable agent tools + shell execution
    #[arg(long)]
    pub yolo: bool,

    /// Maximum number of concurrent sub-agents (1-20)
    #[arg(long)]
    pub max_subagents: Option<usize>,

    /// Path to config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Config profile name
    #[arg(long, global = true)]
    pub profile: Option<String>,

    /// Workspace directory for file operations
    #[arg(short, long, global = true)]
    pub workspace: Option<PathBuf>,

    /// Resume a previous session by ID or prefix
    #[arg(short, long)]
    pub resume: Option<String>,

    /// Continue the most recent session in this workspace
    #[arg(short = 'c', long = "continue")]
    pub continue_session: bool,

    /// Disable the alternate screen buffer (inline mode)
    #[arg(long = "no-alt-screen")]
    pub no_alt_screen: bool,

    /// Enable TUI mouse capture for internal scrolling and transcript selection
    /// (default off on Windows)
    #[arg(long = "mouse-capture", conflicts_with = "no_mouse_capture")]
    pub mouse_capture: bool,

    /// Disable TUI mouse capture so terminal-native text selection works
    #[arg(long = "no-mouse-capture", conflicts_with = "mouse_capture")]
    pub no_mouse_capture: bool,

    /// Skip onboarding screens
    #[arg(long)]
    pub skip_onboarding: bool,

    /// Start a fresh session, ignoring any crash-recovery checkpoint
    #[arg(long = "fresh")]
    pub fresh: bool,

    /// Skip loading project-level config from $WORKSPACE/.zagens/config.toml
    #[arg(long = "no-project-config", global = true)]
    pub no_project_config: bool,
}

#[derive(Subcommand, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Commands {
    /// Run system diagnostics and check configuration
    Doctor(DoctorArgs),
    /// Open a `zagens://` deep link (launch desktop when installed)
    OpenUrl(OpenUrlArgs),
    /// Bootstrap MCP config and/or skills directories
    Setup(SetupArgs),
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },
    /// List saved sessions
    Sessions {
        /// Maximum number of sessions to display
        #[arg(short, long, default_value = "20")]
        limit: usize,
        /// Search sessions by title
        #[arg(short, long)]
        search: Option<String>,
    },
    /// Create default AGENTS.md in current directory
    Init,
    /// Save a DeepSeek API key to the shared user config
    Login {
        /// API key to store (otherwise read from stdin)
        #[arg(long)]
        api_key: Option<String>,
    },
    /// Remove the saved API key
    Logout,
    /// List available models from the configured API endpoint
    Models(ModelsArgs),
    /// Run a non-interactive prompt
    Exec(ExecArgs),
    /// Run a code review over a git diff
    Review(ReviewArgs),
    /// Open the TUI pre-seeded with a GitHub PR's title, body, and diff (#451)
    Pr {
        /// PR number
        #[arg(value_name = "NUMBER")]
        number: u32,
        /// Repository in `owner/name` form. Defaults to the current
        /// workspace's `gh` config (i.e. the repo gh thinks you're in).
        #[arg(short = 'R', long)]
        repo: Option<String>,
        /// Skip `gh pr checkout` even if gh is available. By default
        /// the working tree is left as-is — checkout is opt-in via
        /// `--checkout` because dirty trees fail it loudly.
        #[arg(long, default_value_t = false)]
        checkout: bool,
    },
    /// Apply a patch file (or stdin) to the working tree
    Apply(ApplyArgs),
    /// Run the offline evaluation harness (no network/LLM calls)
    Eval(EvalArgs),
    /// Layer-2 cross-platform completion gate check (replaces PowerShell scripts)
    CoverageGate(CoverageGateArgs),
    /// Export Kernel V3 event trace as HTML or JSON bundle (Flight Recorder)
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
    /// Manage MCP servers
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Execpolicy tooling
    Execpolicy(ExecpolicyCommand),
    /// Inspect feature flags
    Features(FeaturesCli),
    /// Run a command inside the sandbox
    Sandbox(SandboxArgs),
    /// Run a local server (e.g. MCP)
    Serve(ServeArgs),
    /// Resume a previous session by ID (use --last for most recent)
    Resume {
        /// Conversation/session id (UUID or prefix)
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Continue the most recent session in this workspace without a picker
        #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
        last: bool,
    },
    /// Fork a previous session by ID (use --last for most recent)
    Fork {
        /// Conversation/session id (UUID or prefix)
        #[arg(value_name = "SESSION_ID")]
        session_id: Option<String>,
        /// Fork the most recent session in this workspace without a picker
        #[arg(long = "last", default_value_t = false, conflicts_with = "session_id")]
        last: bool,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ExecArgs {
    /// Prompt to send to the model
    pub prompt: String,
    /// Override model for this run
    #[arg(long)]
    pub model: Option<String>,
    /// Enable agentic mode with tool access and auto-approvals
    #[arg(long, default_value_t = false)]
    pub auto: bool,
    /// Emit machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct SetupArgs {
    /// Initialize MCP configuration at the configured path
    #[arg(long, default_value_t = false)]
    pub mcp: bool,
    /// Initialize skills directory and an example skill
    #[arg(long, default_value_t = false)]
    pub skills: bool,
    /// Initialize tools directory with a self-describing example script
    #[arg(long, default_value_t = false)]
    pub tools: bool,
    /// Initialize plugins directory with a self-describing example
    #[arg(long, default_value_t = false)]
    pub plugins: bool,
    /// Initialize MCP config, skills, tools, and plugins
    #[arg(long, default_value_t = false)]
    pub all: bool,
    /// Create a local workspace skills directory (./skills)
    #[arg(long, default_value_t = false)]
    pub local: bool,
    /// Overwrite existing template files
    #[arg(long, default_value_t = false)]
    pub force: bool,
    /// Print a compact, read-only status report (no network calls)
    #[arg(long, default_value_t = false, conflicts_with_all = ["mcp", "skills", "tools", "plugins", "all", "local", "clean"])]
    pub status: bool,
    /// Remove regenerable session checkpoints (latest + offline_queue)
    #[arg(long, default_value_t = false, conflicts_with_all = ["mcp", "skills", "tools", "plugins", "all", "local", "status"])]
    pub clean: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON output (skips live API connectivity check)
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct OpenUrlArgs {
    /// `zagens://open?workspace=...&prompt=...&task_type=code`
    pub url: String,
    /// Parse/validate and emit JSON (still launches unless `--validate-only`)
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Validate only; do not launch the desktop app
    #[arg(long, default_value_t = false)]
    pub validate_only: bool,
}

#[derive(Args, Debug, Clone)]
pub struct EvalArgs {
    /// Intentionally fail a specific step (list, read, search, edit, patch, shell)
    #[arg(long, value_name = "STEP")]
    pub fail_step: Option<String>,
    /// Shell command to run during the exec step
    #[arg(long, default_value = "printf eval-harness")]
    pub shell_command: String,
    /// Token that must appear in shell output for validation
    #[arg(long, default_value = "eval-harness")]
    pub shell_expect_token: String,
    /// Maximum characters stored per step output summary
    #[arg(long, default_value_t = 240)]
    pub max_output_chars: usize,
    /// Emit machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Append one JSONL fixture line per step to `<DIR>/<scenario>.jsonl`.
    /// Mock LLM tests can later replay these fixtures.
    #[arg(long, value_name = "DIR")]
    pub record: Option<PathBuf>,
}

#[derive(Args, Debug, Clone, Default)]
pub struct ModelsArgs {
    /// Print models as pretty JSON
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Debug, Default, Clone)]
pub struct FeatureToggles {
    /// Enable a feature (repeatable). Equivalent to `features.<name>=true`.
    #[arg(long = "enable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    pub enable: Vec<String>,

    /// Disable a feature (repeatable). Equivalent to `features.<name>=false`.
    #[arg(long = "disable", value_name = "FEATURE", action = clap::ArgAction::Append, global = true)]
    pub disable: Vec<String>,
}

impl FeatureToggles {
    pub fn apply(&self, config: &mut Config) -> Result<()> {
        for feature in &self.enable {
            config.set_feature(feature, true)?;
        }
        for feature in &self.disable {
            config.set_feature(feature, false)?;
        }
        Ok(())
    }
}

#[derive(Args, Debug, Clone)]
pub struct ReviewArgs {
    /// Review staged changes instead of the working tree
    #[arg(long, conflicts_with = "base")]
    pub staged: bool,
    /// Base ref to diff against (e.g. origin/main)
    #[arg(long)]
    pub base: Option<String>,
    /// Limit diff to a specific path
    #[arg(long)]
    pub path: Option<PathBuf>,
    /// Override model for this review
    #[arg(long)]
    pub model: Option<String>,
    /// Maximum diff characters to include
    #[arg(long, default_value_t = 200_000)]
    pub max_chars: usize,
    /// Emit machine-readable JSON output
    #[arg(long, default_value_t = false)]
    pub json: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ApplyArgs {
    /// Patch file to apply (defaults to stdin)
    #[arg(value_name = "PATCH_FILE")]
    pub patch_file: Option<PathBuf>,
}

#[derive(Args, Debug, Clone)]
pub struct ServeArgs {
    /// Start MCP server over stdio
    #[arg(long)]
    pub mcp: bool,
    /// Start runtime HTTP/SSE API server
    #[arg(long)]
    pub http: bool,
    /// Start ACP server over stdio for editor clients such as Zed
    #[arg(long)]
    pub acp: bool,
    /// Bind host for HTTP server (default localhost)
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    /// Bind port for HTTP server
    #[arg(long, default_value_t = 7878)]
    pub port: u16,
    /// Background task worker count (1-16)
    #[arg(long, default_value_t = 8)]
    pub workers: usize,
    /// Additional CORS origin to allow (repeatable). Stacks on top of the
    /// built-in defaults (localhost:3000, localhost:1420, tauri://localhost).
    /// Also reads `DEEPSEEK_CORS_ORIGINS` (comma-separated) and
    /// `[runtime_api] cors_origins` from `config.toml`. Whalescale#255.
    #[arg(long = "cors-origin", value_name = "URL")]
    pub cors_origin: Vec<String>,
    /// Require this bearer token for `/v1/*` runtime API routes. Also reads
    /// `DEEPSEEK_RUNTIME_TOKEN` when omitted.
    #[arg(long = "auth-token", value_name = "TOKEN")]
    pub auth_token: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum McpCommand {
    /// List configured MCP servers
    List,
    /// Create a template MCP config at the configured path
    Init {
        /// Overwrite an existing MCP config file
        #[arg(long, default_value_t = false)]
        force: bool,
    },
    /// Connect to MCP servers and report status
    Connect {
        /// Optional server name to connect to
        #[arg(value_name = "SERVER")]
        server: Option<String>,
    },
    /// List tools discovered from MCP servers
    Tools {
        /// Optional server name to list tools for
        #[arg(value_name = "SERVER")]
        server: Option<String>,
    },
    /// Add an MCP server entry
    Add {
        /// Server name
        name: String,
        /// Command to launch stdio server
        #[arg(long, conflicts_with = "url")]
        command: Option<String>,
        /// URL for streamable HTTP/SSE server
        #[arg(long, conflicts_with = "command")]
        url: Option<String>,
        /// Arguments for command-based servers
        #[arg(long = "arg")]
        args: Vec<String>,
    },
    /// Remove an MCP server entry
    Remove {
        /// Server name
        name: String,
    },
    /// Enable an MCP server
    Enable {
        /// Server name
        name: String,
    },
    /// Disable an MCP server
    Disable {
        /// Server name
        name: String,
    },
    /// Validate MCP config and required servers
    Validate,
    /// Register this DeepSeek binary as a local MCP stdio server.
    ///
    /// This adds a config entry that runs `deepseek serve --mcp` (stdio protocol).
    /// For the HTTP/SSE runtime API, use `deepseek serve --http` directly instead.
    #[command(
        name = "add-self",
        long_about = "Register this DeepSeek binary as a local MCP stdio server.\n\nAdds a config entry to ~/.deepseek/mcp.json that launches `deepseek serve --mcp`\nvia the stdio transport. Other DeepSeek sessions (or any MCP client) can then\ndiscover and call tools exposed by this server.\n\nUse `deepseek serve --http` instead if you need the HTTP/SSE runtime API."
    )]
    AddSelf {
        /// Server name in mcp.json (default: "deepseek")
        #[arg(long, default_value = "deepseek")]
        name: String,
        /// Workspace directory for the MCP server
        #[arg(long)]
        workspace: Option<String>,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ExecpolicyCommand {
    #[command(subcommand)]
    pub command: ExecpolicySubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ExecpolicySubcommand {
    /// Check execpolicy files against a command
    Check(crate::execpolicy::ExecPolicyCheckCommand),
}

#[derive(Args, Debug, Clone)]
pub struct FeaturesCli {
    #[command(subcommand)]
    pub command: FeaturesSubcommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum FeaturesSubcommand {
    /// List known feature flags and their state
    List,
}

#[derive(Args, Debug, Clone)]
pub struct SandboxArgs {
    #[command(subcommand)]
    pub command: SandboxCommand,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SandboxCommand {
    /// Gate G0 PoC subcommands (Windows only)
    Poc {
        #[command(subcommand)]
        command: SandboxPocCommand,
    },
    /// Remove unelevated sandbox ACL state (Phase 1; no WFP/users)
    Teardown {
        /// Keep cap_sid file and sandbox logs
        #[arg(long)]
        keep_logs: bool,
    },
    /// Elevated Windows sandbox setup (UAC; creates sandbox users + marker)
    Setup,
    /// Grant an additional read path for elevated sandbox users (PR-3.3)
    AddReadDir {
        /// Directory or file to grant read (+execute) access
        path: PathBuf,
    },
    /// Run a command with sandboxing
    Run {
        /// Sandbox policy (danger-full-access, read-only, external-sandbox, workspace-write)
        #[arg(long, default_value = "workspace-write")]
        policy: String,
        /// Allow outbound network access
        #[arg(long)]
        network: bool,
        /// Additional writable roots (repeatable)
        #[arg(long, value_name = "PATH")]
        writable_root: Vec<PathBuf>,
        /// Exclude TMPDIR from writable paths
        #[arg(long)]
        exclude_tmpdir: bool,
        /// Exclude /tmp from writable paths
        #[arg(long)]
        exclude_slash_tmp: bool,
        /// Command working directory
        #[arg(long)]
        cwd: Option<PathBuf>,
        /// Timeout in milliseconds
        #[arg(long, default_value_t = 60_000)]
        timeout_ms: u64,
        /// Command and arguments to run
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum SandboxPocCommand {
    /// Verify unelevated deny-read isolation; writes ~/.zagens/.sandbox/unelevated_deny_read_poc.json
    DenyRead,
}

/// Arguments for `zagens coverage-gate`
#[derive(Args, Debug, Clone)]
pub struct CoverageGateArgs {
    /// Workspace directory (default: current directory)
    #[arg(short, long)]
    pub workspace: Option<std::path::PathBuf>,
    /// Require all todo-list items to be marked completed
    #[arg(long, default_value_t = true)]
    pub require_checklist_complete: bool,
    /// Run `cargo test` to verify test suite passes (slow; off by default)
    #[arg(long = "run-tests", default_value_t = false)]
    pub run_tests: bool,
    /// Emit machine-readable JSON output instead of human-readable text
    #[arg(long, default_value_t = false)]
    pub json: bool,
    /// Task ID to check in the CRAFT blackboard (optional; checks latest if omitted)
    #[arg(long)]
    pub task_id: Option<String>,
    /// Exit 0 even when gate fails (report-only mode)
    #[arg(long, default_value_t = false)]
    pub no_fail: bool,
}

#[derive(Subcommand, Debug, Clone)]
pub enum TraceCommand {
    /// Export a fixture or thread to HTML / JSON bundle
    Export(TraceExportArgs),
    /// Compare two threads or fixtures
    Compare(TraceCompareArgs),
    /// Local preview HTTP server (`--watch` for live thread tail)
    Serve(TraceServeArgs),
}

/// Arguments for `zagens trace export`
#[derive(Args, Debug, Clone)]
pub struct TraceExportArgs {
    /// Golden fixture JSON (`fixtures/harness/kernel-v3-replay/*.json`)
    #[arg(long, conflicts_with = "thread")]
    pub fixture: Option<PathBuf>,
    /// Runtime thread id
    #[arg(long, conflicts_with = "fixture")]
    pub thread: Option<String>,
    /// Attach offline harness task-graph snapshot (thread mode only)
    #[arg(long, default_value_t = true)]
    pub include_harness: bool,
    /// Output file path (`.html` or `.json`)
    #[arg(short, long)]
    pub out: PathBuf,
    /// Output format: `html` (default) or `bundle` (JSON only)
    #[arg(long, default_value = "html")]
    pub format: String,
    /// HTML shell template (default: `tools/trace-report/dist/report.html`)
    #[arg(long)]
    pub template: Option<PathBuf>,
    /// Disable secret redaction (thread exports redact by default)
    #[arg(long)]
    pub no_redact: bool,
}

/// Arguments for `zagens trace compare`
#[derive(Args, Debug, Clone)]
pub struct TraceCompareArgs {
    /// Left runtime thread id
    #[arg(long, conflicts_with = "left_fixture")]
    pub left: Option<String>,
    /// Left golden fixture JSON
    #[arg(long, conflicts_with = "left")]
    pub left_fixture: Option<PathBuf>,
    /// Right runtime thread id
    #[arg(long, conflicts_with = "right_fixture")]
    pub right: Option<String>,
    /// Right golden fixture JSON
    #[arg(long, conflicts_with = "right")]
    pub right_fixture: Option<PathBuf>,
    /// Attach offline harness snapshots (thread mode only)
    #[arg(long, default_value_t = true)]
    pub include_harness: bool,
    /// Output file path (`.html` or `.json`)
    #[arg(short, long)]
    pub out: PathBuf,
    /// Output format: `html` (default) or `bundle`
    #[arg(long, default_value = "html")]
    pub format: String,
    /// HTML shell template
    #[arg(long)]
    pub template: Option<PathBuf>,
    /// Disable secret redaction (thread exports redact by default)
    #[arg(long)]
    pub no_redact: bool,
}

/// Arguments for `zagens trace serve`
#[derive(Args, Debug, Clone)]
pub struct TraceServeArgs {
    /// Runtime thread id (required for `--watch`)
    #[arg(long, conflicts_with = "fixture")]
    pub thread: Option<String>,
    /// Golden fixture JSON (static preview)
    #[arg(long, conflicts_with = "thread")]
    pub fixture: Option<PathBuf>,
    #[arg(long, default_value = "127.0.0.1")]
    pub host: String,
    #[arg(long, default_value_t = 8765)]
    pub port: u16,
    #[arg(long, default_value_t = true)]
    pub include_harness: bool,
    #[arg(long)]
    pub no_redact: bool,
    /// Poll thread kernel log and reload when events change
    #[arg(long)]
    pub watch: bool,
    #[arg(long, default_value_t = 3, requires = "watch")]
    pub watch_interval_secs: u64,
    #[arg(long)]
    pub template: Option<PathBuf>,
}
