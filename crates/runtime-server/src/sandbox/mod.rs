#![allow(dead_code)]

//! Sandbox module for secure command execution.
//!
//! This module provides sandboxing capabilities for shell commands executed by
//! DeepSeek TUI. Sandboxing restricts what system resources a command can access,
//! preventing accidental or malicious damage to the system.
//!
//! # Platform Support
//!
//! - **macOS**: Uses Seatbelt (sandbox-exec) for mandatory access control
//! - **Linux**: Uses Landlock (kernel 5.13+) for filesystem access control
//! - **Windows**: Windows Sandbox/AppContainer/Restricted token (best-effort)
//!
//! # Usage
//!
//! ```rust,ignore
//! use sandbox::{SandboxManager, CommandSpec, SandboxPolicy};
//!
//! let manager = SandboxManager::new();
//! let spec = CommandSpec::shell("ls -la", PathBuf::from("."), Duration::from_secs(30))
//!     .with_policy(SandboxPolicy::default());
//!
//! let exec_env = manager.prepare(&spec);
//! // exec_env.command now contains the sandboxed command
//! ```

pub mod backend;
pub mod opensandbox;
pub mod policy;

#[cfg(target_os = "macos")]
pub mod seatbelt;

#[cfg(target_os = "linux")]
pub mod bwrap;

#[cfg(target_os = "linux")]
pub mod landlock;

#[cfg(target_os = "windows")]
pub mod windows;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use zagens_config::WindowsSandboxModeToml;

pub use policy::SandboxPolicy;

use self::backend::SandboxBackend;

/// Engine boundary newtype wrapping the optional external sandbox backend.
///
/// Introduced by M3 (Engine-struct strangler step). The future core-side
/// Engine struct (M7) will hold `Box<dyn SandboxHost>` instead of the
/// current `Option<Arc<dyn SandboxBackend>>` slot; this wrapper carries
/// the same value behind that trait surface so the M7 swap is mechanical.
///
/// `None` is the default and means "use local execution". `Some(backend)`
/// routes shell commands through the remote backend (e.g. OpenSandbox).
#[derive(Clone, Default)]
pub struct TuiSandboxHost(pub Option<Arc<dyn SandboxBackend>>);

impl TuiSandboxHost {
    /// Construct from the `Option<Arc<dyn SandboxBackend>>` that
    /// `crate::sandbox::backend::create_backend(&Config)` produces.
    #[must_use]
    pub fn new(backend: Option<Arc<dyn SandboxBackend>>) -> Self {
        Self(backend)
    }
}

impl zagens_core::engine::hosts::SandboxHost for TuiSandboxHost {
    fn backend(&self) -> Option<&Arc<dyn SandboxBackend>> {
        self.0.as_ref()
    }
}

/// Specification for a command to be executed, potentially within a sandbox.
///
/// This struct captures all the information needed to execute a command:
/// the program and arguments, working directory, environment variables,
/// timeout, and sandbox policy.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    /// The program to execute (e.g., "sh", "python", "cargo").
    pub program: String,

    /// Arguments to pass to the program.
    pub args: Vec<String>,

    /// Working directory for the command.
    pub cwd: PathBuf,

    /// Additional environment variables to set.
    pub env: HashMap<String, String>,

    /// Maximum execution time before the command is killed.
    pub timeout: Duration,

    /// Sandbox policy controlling resource access.
    pub sandbox_policy: SandboxPolicy,

    /// Optional justification for why this command needs to run.
    /// Used for logging and audit purposes.
    pub justification: Option<String>,
}

/// Returns the best-available Windows shell as (program, arg_prefix).
/// Tries pwsh (PowerShell 7+) first, then powershell (Windows PowerShell 5.1);
/// falls back to cmd.exe only when neither PowerShell is available.
#[cfg(windows)]
pub(crate) fn windows_shell() -> (&'static str, &'static str) {
    windows_shell_for(None)
}

/// Returns the Windows shell for a given preference (agent.shell config).
///
/// | preference     | result                                       |
/// |----------------|----------------------------------------------|
/// | `"pwsh"`       | pwsh.exe (PowerShell 7+)                     |
/// | `"powershell"` | powershell.exe (Windows PowerShell 5.1)       |
/// | `"cmd"`        | cmd.exe                                      |
/// | `"auto"`       | auto-detect: pwsh → powershell → cmd         |
/// | `None`         | same as `"auto"`                             |
#[cfg(windows)]
pub(crate) fn windows_shell_for(preference: Option<&str>) -> (&'static str, &'static str) {
    let mode = preference.unwrap_or("auto").trim().to_ascii_lowercase();
    match mode.as_str() {
        "pwsh" => ("pwsh", "-Command"),
        "powershell" => ("powershell", "-Command"),
        "cmd" => ("cmd", "/C"),
        "auto" | "" => {
            use std::sync::OnceLock;
            static DETECTED: OnceLock<(&'static str, &'static str)> = OnceLock::new();
            *DETECTED.get_or_init(|| {
                for ps in &["pwsh", "powershell"] {
                    if std::process::Command::new(ps)
                        .args(["-NoProfile", "-NonInteractive", "-Command", "exit 0"])
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                    {
                        return (ps, "-Command");
                    }
                }
                let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
                let leaked = Box::leak(comspec.into_boxed_str());
                (leaked, "/C")
            })
        }
        other => {
            tracing::warn!(
                "unknown agent.shell value '{}', falling back to auto-detect",
                other
            );
            windows_shell_for(Some("auto"))
        }
    }
}

/// Whether `program` names a PowerShell executable (short name or full path).
#[cfg(windows)]
pub(crate) fn is_powershell_program(program: &str) -> bool {
    let lower = program.to_ascii_lowercase();
    lower == "pwsh"
        || lower == "powershell"
        || lower.ends_with("\\pwsh.exe")
        || lower.ends_with("/pwsh.exe")
        || lower.ends_with("\\powershell.exe")
        || lower.ends_with("/powershell.exe")
}

/// argv for `CommandSpec::shell` / hooks / gate runner (OpenCode-aligned PS flags).
#[cfg(windows)]
pub(crate) fn windows_shell_argv(program: &str, command: &str) -> Vec<String> {
    if is_powershell_program(program) {
        vec![
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-Command".to_string(),
            command.to_string(),
        ]
    } else {
        vec!["/C".to_string(), command.to_string()]
    }
}

impl CommandSpec {
    /// Create a `CommandSpec` for running a shell command via the platform shell.
    pub fn shell(command: &str, cwd: PathBuf, timeout: Duration) -> Self {
        Self::shell_with_pref(command, cwd, timeout, None)
    }

    /// Create a `CommandSpec` with an optional shell preference (Windows: agent.shell config).
    pub fn shell_with_pref(
        command: &str,
        cwd: PathBuf,
        timeout: Duration,
        shell_preference: Option<&str>,
    ) -> Self {
        #[cfg(windows)]
        let (program, args) = {
            let (program, _) = windows_shell_for(shell_preference);
            (program.to_string(), windows_shell_argv(program, command))
        };
        #[cfg(not(windows))]
        let (program, args) = (
            "sh".to_string(),
            vec!["-c".to_string(), command.to_string()],
        );

        Self {
            program,
            args,
            cwd,
            env: HashMap::new(),
            timeout,
            sandbox_policy: SandboxPolicy::default(),
            justification: None,
        }
    }

    /// Create a `CommandSpec` for running a program directly.
    pub fn program(program: &str, args: Vec<String>, cwd: PathBuf, timeout: Duration) -> Self {
        Self {
            program: program.to_string(),
            args,
            cwd,
            env: HashMap::new(),
            timeout,
            sandbox_policy: SandboxPolicy::default(),
            justification: None,
        }
    }

    /// Set the sandbox policy for this command.
    pub fn with_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    /// Add environment variables for this command.
    pub fn with_env(mut self, env: HashMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// Add a single environment variable.
    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    /// Set a justification for this command (for logging/audit).
    pub fn with_justification(mut self, justification: &str) -> Self {
        self.justification = Some(justification.to_string());
        self
    }

    /// Get the original command as a single string (for display).
    pub fn display_command(&self) -> String {
        if self.program == "sh" && self.args.len() == 2 && self.args[0] == "-c" {
            // For shell commands, show the actual command
            self.args[1].clone()
        } else if (self.program.eq_ignore_ascii_case("cmd")
            && self.args.len() >= 2
            && self.args[0].eq_ignore_ascii_case("/C"))
            || (cfg!(windows)
                && is_powershell_program(&self.program)
                && self.args.iter().any(|a| a.eq_ignore_ascii_case("-Command")))
        {
            if let Some(idx) = self
                .args
                .iter()
                .position(|a| a.eq_ignore_ascii_case("-Command"))
            {
                self.args.get(idx + 1).cloned().unwrap_or_default()
            } else {
                self.args.last().cloned().unwrap_or_default()
            }
        } else {
            // For other commands, join program and args
            let mut parts = vec![self.program.clone()];
            parts.extend(self.args.clone());
            parts.join(" ")
        }
    }
}

/// The type of sandbox being used for execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SandboxType {
    /// No sandboxing - command runs with full permissions.
    #[default]
    None,

    /// macOS Seatbelt (sandbox-exec) sandboxing.
    #[cfg(target_os = "macos")]
    MacosSeatbelt,

    /// Linux Landlock sandboxing (kernel 5.13+).
    #[cfg(target_os = "linux")]
    LinuxLandlock,

    /// Linux Bubblewrap (bwrap) sandboxing — opt-in via `prefer_bwrap` (M0.4).
    #[cfg(target_os = "linux")]
    LinuxBwrap,

    /// Windows sandboxing (Windows Sandbox/AppContainer/Restricted token).
    #[cfg(target_os = "windows")]
    Windows,
}

impl std::fmt::Display for SandboxType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxType::None => write!(f, "none"),
            #[cfg(target_os = "macos")]
            SandboxType::MacosSeatbelt => write!(f, "macos-seatbelt"),
            #[cfg(target_os = "linux")]
            SandboxType::LinuxLandlock => write!(f, "linux-landlock"),
            #[cfg(target_os = "linux")]
            SandboxType::LinuxBwrap => write!(f, "linux-bwrap"),
            #[cfg(target_os = "windows")]
            SandboxType::Windows => write!(f, "windows-sandbox"),
        }
    }
}

/// The execution environment after sandbox transformation.
///
/// This contains the actual command to run (which may include sandbox wrapper
/// commands) and all necessary environment configuration.
#[derive(Debug)]
pub struct ExecEnv {
    /// The full command to execute (may include sandbox wrapper).
    pub command: Vec<String>,

    /// Working directory for execution.
    pub cwd: PathBuf,

    /// Environment variables to set.
    pub env: HashMap<String, String>,

    /// Timeout for the command.
    pub timeout: Duration,

    /// The type of sandbox being used.
    pub sandbox_type: SandboxType,

    /// The original policy (for reference).
    pub policy: SandboxPolicy,

    /// Whether OS-level isolation is actively enforced for this execution.
    pub enforced: bool,

    /// Windows native sandbox plan (when `enforced` on Windows).
    #[cfg(target_os = "windows")]
    pub windows_plan: Option<zagens_windows_sandbox::WindowsExecPlan>,
}

impl ExecEnv {
    /// Get the program to execute (first element of command).
    pub fn program(&self) -> &str {
        self.command
            .first()
            .map_or("sh", std::string::String::as_str)
    }

    /// Get the arguments (all elements after the first).
    pub fn args(&self) -> &[String] {
        if self.command.len() > 1 {
            &self.command[1..]
        } else {
            &[]
        }
    }

    /// Check if this execution is sandboxed.
    pub fn is_sandboxed(&self) -> bool {
        !matches!(self.sandbox_type, SandboxType::None)
    }

    /// Whether OS-level sandbox restrictions are enforced (not policy-declaration-only).
    pub fn is_enforced(&self) -> bool {
        self.enforced
    }
}

/// Detect what sandbox technology is available on the current platform.
pub fn get_platform_sandbox() -> Option<SandboxType> {
    #[cfg(target_os = "macos")]
    {
        if seatbelt::is_available() {
            return Some(SandboxType::MacosSeatbelt);
        }
    }

    #[cfg(target_os = "linux")]
    {
        if landlock::is_available() {
            return Some(SandboxType::LinuxLandlock);
        }
    }

    #[cfg(target_os = "windows")]
    {
        if windows::is_available() {
            return Some(SandboxType::Windows);
        }
    }

    None
}

/// Check if sandboxing is available on this platform.
pub fn is_sandbox_available() -> bool {
    get_platform_sandbox().is_some()
}

/// User-facing notice when `sandbox_mode` declares policy but OS isolation is degraded (A6.2).
///
/// macOS with Seatbelt is fully enforced; Linux/Windows (and macOS without `sandbox-exec`)
/// run in degraded mode — shell stderr also gets [`ExecEnv::sandbox_enforcement_warning`].
#[must_use]
pub fn policy_degraded_mode_notice() -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        if seatbelt::is_available() {
            return None;
        }
        Some(
            "Degraded mode: sandbox-exec (Seatbelt) is unavailable; sandbox_mode declares policy only.",
        )
    }

    #[cfg(target_os = "linux")]
    {
        let _ = landlock::is_available();
        Some(
            "Degraded mode: Landlock rules are not enforced yet; sandbox_mode declares policy only. Install bubblewrap and set `prefer_bwrap = true` for enforced isolation.",
        )
    }

    #[cfg(target_os = "windows")]
    {
        let home = zagens_windows_sandbox::zagens_home();
        if zagens_windows_sandbox::sandbox_setup_is_complete(&home) {
            return None;
        }
        if zagens_windows_sandbox::is_enforcement_available() {
            return Some(
                "Degraded mode: elevated sandbox setup is not complete; using unelevated write isolation only (no profile read isolation). Run `zagens sandbox setup`.",
            );
        }
        Some(
            "Degraded mode: Windows sandbox is not enforced yet; sandbox_mode declares policy only.",
        )
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Some("Degraded mode: OS sandbox is not supported on this platform.")
}

/// Manager for sandbox operations.
///
/// The `SandboxManager` is responsible for:
/// - Detecting available sandbox technologies
/// - Transforming `CommandSpecs` into sandboxed `ExecEnvs`
/// - Detecting sandbox denials from command output
#[derive(Debug, Default)]
pub struct SandboxManager {
    /// Cached sandbox availability check.
    sandbox_available: Option<bool>,

    /// Force a specific sandbox type (for testing).
    #[allow(dead_code)]
    forced_sandbox: Option<SandboxType>,

    /// Windows native sandbox mode from `[windows] sandbox` config.
    windows_sandbox_mode: WindowsSandboxModeToml,

    /// Optional isolated desktop for sandbox children (`[windows] sandbox_private_desktop`).
    windows_private_desktop: bool,

    /// Prefer the Bubblewrap backend on Linux (`prefer_bwrap` config, M0.4).
    /// Only takes effect when bwrap is installed; otherwise the Landlock
    /// declare-only fallback applies unchanged.
    prefer_bwrap: bool,
}

impl SandboxManager {
    /// Create a new `SandboxManager`.
    pub fn new() -> Self {
        Self {
            sandbox_available: None,
            forced_sandbox: None,
            windows_sandbox_mode: WindowsSandboxModeToml::Unelevated,
            windows_private_desktop: false,
            prefer_bwrap: false,
        }
    }

    /// Enable/disable the Bubblewrap backend preference (`prefer_bwrap`).
    pub fn set_prefer_bwrap(&mut self, prefer: bool) {
        self.prefer_bwrap = prefer;
    }

    #[must_use]
    pub fn prefer_bwrap(&self) -> bool {
        self.prefer_bwrap
    }

    /// Set the Windows native sandbox mode (`[windows] sandbox`).
    pub fn set_windows_sandbox_mode(&mut self, mode: WindowsSandboxModeToml) {
        self.windows_sandbox_mode = mode;
    }

    pub fn set_windows_private_desktop(&mut self, enabled: bool) {
        self.windows_private_desktop = enabled;
    }

    #[must_use]
    pub fn windows_private_desktop(&self) -> bool {
        self.windows_private_desktop
    }

    #[must_use]
    pub fn windows_sandbox_mode(&self) -> WindowsSandboxModeToml {
        self.windows_sandbox_mode
    }

    /// Check if sandboxing is available.
    pub fn is_available(&mut self) -> bool {
        if let Some(available) = self.sandbox_available {
            return available;
        }

        let available = is_sandbox_available();
        self.sandbox_available = Some(available);
        available
    }

    /// Select the appropriate sandbox type for the given policy.
    pub fn select_sandbox(&self, policy: &SandboxPolicy) -> SandboxType {
        // If the policy doesn't want sandboxing, return None
        if !policy.should_sandbox() {
            return SandboxType::None;
        }

        // Check for forced sandbox (testing)
        if let Some(forced) = self.forced_sandbox {
            return forced;
        }

        // Linux: opt-in bwrap takes precedence when installed (M0.4).
        #[cfg(target_os = "linux")]
        if self.prefer_bwrap && bwrap::is_available() {
            return SandboxType::LinuxBwrap;
        }

        // Use platform default
        get_platform_sandbox().unwrap_or(SandboxType::None)
    }

    /// Transform a `CommandSpec` into a sandboxed `ExecEnv`.
    ///
    /// This is the main entry point for sandboxing. It takes a command
    /// specification and returns the actual command to run, which may
    /// include sandbox wrapper commands.
    pub fn prepare(&self, spec: &CommandSpec) -> ExecEnv {
        let sandbox_type = self.select_sandbox(&spec.sandbox_policy);

        match sandbox_type {
            SandboxType::None => Self::prepare_unsandboxed(spec),

            #[cfg(target_os = "macos")]
            SandboxType::MacosSeatbelt => Self::prepare_seatbelt(spec),

            #[cfg(target_os = "linux")]
            SandboxType::LinuxLandlock => Self::prepare_landlock(spec),

            #[cfg(target_os = "linux")]
            SandboxType::LinuxBwrap => Self::prepare_bwrap(spec),

            #[cfg(target_os = "windows")]
            SandboxType::Windows => self.prepare_windows(spec),
        }
    }

    /// Prepare an unsandboxed execution environment.
    fn prepare_unsandboxed(spec: &CommandSpec) -> ExecEnv {
        let mut command = vec![spec.program.clone()];
        command.extend(spec.args.clone());

        ExecEnv {
            command,
            cwd: spec.cwd.clone(),
            env: spec.env.clone(),
            timeout: spec.timeout,
            sandbox_type: SandboxType::None,
            policy: spec.sandbox_policy.clone(),
            enforced: false,
            #[cfg(target_os = "windows")]
            windows_plan: None,
        }
    }

    /// Prepare a Seatbelt-sandboxed execution environment (macOS).
    #[cfg(target_os = "macos")]
    fn prepare_seatbelt(spec: &CommandSpec) -> ExecEnv {
        // Build the original command
        let mut original_command = vec![spec.program.clone()];
        original_command.extend(spec.args.clone());

        // Generate sandbox-exec arguments
        let seatbelt_args =
            seatbelt::create_seatbelt_args(original_command, &spec.sandbox_policy, &spec.cwd);

        // Prepend sandbox-exec to the command
        let mut command = vec![seatbelt::SANDBOX_EXEC_PATH.to_string()];
        command.extend(seatbelt_args);

        // Add sandbox indicator to environment
        let mut env = spec.env.clone();
        env.insert("DEEPSEEK_SANDBOX".to_string(), "seatbelt".to_string());

        ExecEnv {
            command,
            cwd: spec.cwd.clone(),
            env,
            timeout: spec.timeout,
            sandbox_type: SandboxType::MacosSeatbelt,
            policy: spec.sandbox_policy.clone(),
            enforced: true,
        }
    }

    /// Prepare a Landlock-sandboxed execution environment (Linux).
    ///
    /// **⚠️ SECURITY NOTICE**: This function currently does **not** apply any
    /// Landlock rules. It only sets the `DEEPSEEK_SANDBOX` environment marker.
    /// The command will execute with **full system access** — the Landlock
    /// sandbox is not yet enforced. Full Landlock isolation requires a helper
    /// binary (see `sandbox/landlock.rs` in the Step 6 implementation plan at
    /// `docs/CODE_REVIEW_2025-05-11.md#625-实现-linux-landlock-沙箱`).
    ///
    /// Technical context: Landlock restricts the current process, so for
    /// subprocess sandboxing we would need a helper binary that:
    /// 1. Sets up the Landlock ruleset based on the policy
    /// 2. Applies restrictions to itself (LandlockRestrictSelf)
    /// 3. Execs the target command
    #[cfg(target_os = "linux")]
    fn prepare_landlock(spec: &CommandSpec) -> ExecEnv {
        // Build the original command
        let mut command = vec![spec.program.clone()];
        command.extend(spec.args.clone());

        // Add sandbox indicator to environment
        let mut env = spec.env.clone();
        env.insert("DEEPSEEK_SANDBOX".to_string(), "landlock".to_string());

        // Note: Full Landlock implementation would use a helper binary that:
        // 1. Sets up the Landlock ruleset based on policy
        // 2. Applies restrictions to itself
        // 3. Execs the target command
        //
        // For now, we just mark that Landlock would be used

        let mut exec = ExecEnv {
            command,
            cwd: spec.cwd.clone(),
            env,
            timeout: spec.timeout,
            sandbox_type: SandboxType::LinuxLandlock,
            policy: spec.sandbox_policy.clone(),
            enforced: false,
        };
        mark_sandbox_policy_unenforced(&mut exec);
        exec
    }

    /// Prepare a Bubblewrap-sandboxed execution environment (Linux, M0.4).
    ///
    /// Unlike [`Self::prepare_landlock`], this path is **enforced**: the
    /// kernel mount namespace gives a read-only root view with write access
    /// limited to the policy's writable roots.
    #[cfg(target_os = "linux")]
    fn prepare_bwrap(spec: &CommandSpec) -> ExecEnv {
        let command =
            bwrap::build_bwrap_command(&spec.sandbox_policy, &spec.cwd, &spec.program, &spec.args);

        let mut env = spec.env.clone();
        env.insert("DEEPSEEK_SANDBOX".to_string(), "bwrap".to_string());

        ExecEnv {
            command,
            cwd: spec.cwd.clone(),
            env,
            timeout: spec.timeout,
            sandbox_type: SandboxType::LinuxBwrap,
            policy: spec.sandbox_policy.clone(),
            enforced: true,
        }
    }

    /// Prepare a Windows-sandboxed execution environment (unelevated MVP).
    #[cfg(target_os = "windows")]
    fn prepare_windows(&self, spec: &CommandSpec) -> ExecEnv {
        let mut command = vec![spec.program.clone()];
        command.extend(spec.args.clone());

        let writable = spec.sandbox_policy.get_writable_roots(&spec.cwd);
        let mut roots: Vec<PathBuf> = writable.iter().map(|entry| entry.root.clone()).collect();
        roots = zagens_windows_sandbox::filter_ssh_config_dependency_roots(&roots);

        let mut protected = Vec::new();
        for entry in &writable {
            protected.extend(entry.read_only_subpaths.clone());
        }
        for root in &roots {
            protected.extend(zagens_windows_sandbox::protected_subdirs_for_root(root));
        }

        let plan_mode = Self::resolve_windows_plan_mode(self.windows_sandbox_mode);

        match zagens_windows_sandbox::plan_exec(zagens_windows_sandbox::PlanInput {
            program: spec.program.clone(),
            args: spec.args.clone(),
            cwd: spec.cwd.clone(),
            env: spec.env.clone(),
            writable_roots: roots,
            protected_write_paths: protected,
            network_allowed: spec.sandbox_policy.has_network_access(),
            mode: plan_mode,
            private_desktop: self.windows_private_desktop,
            tty: false,
        }) {
            Ok(plan) => ExecEnv {
                command: plan.argv.clone(),
                cwd: plan.cwd.clone(),
                env: plan.env.clone(),
                timeout: spec.timeout,
                sandbox_type: SandboxType::Windows,
                policy: spec.sandbox_policy.clone(),
                enforced: true,
                windows_plan: Some(plan),
            },
            Err(err) => {
                tracing::warn!(
                    target: "sandbox",
                    "Windows sandbox plan failed; falling back to degraded mode: {err:#}"
                );
                let mut env = spec.env.clone();
                env.insert(
                    "DEEPSEEK_SANDBOX".to_string(),
                    "windows:unelevated".to_string(),
                );
                let mut exec = ExecEnv {
                    command,
                    cwd: spec.cwd.clone(),
                    env,
                    timeout: spec.timeout,
                    sandbox_type: SandboxType::Windows,
                    policy: spec.sandbox_policy.clone(),
                    enforced: false,
                    windows_plan: None,
                };
                mark_sandbox_policy_unenforced(&mut exec);
                exec
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn resolve_windows_plan_mode(
        configured: WindowsSandboxModeToml,
    ) -> zagens_windows_sandbox::WindowsSandboxMode {
        use zagens_windows_sandbox::WindowsSandboxMode;

        match crate::config::effective_windows_sandbox_execution_mode(configured) {
            WindowsSandboxModeToml::Unelevated => WindowsSandboxMode::Unelevated,
            WindowsSandboxModeToml::Elevated => WindowsSandboxMode::Elevated,
        }
    }

    pub(crate) fn noop_sandbox_warning(sandbox_type: SandboxType) -> Option<&'static str> {
        match sandbox_type {
            #[cfg(target_os = "linux")]
            SandboxType::LinuxLandlock => Some(
                "Linux Landlock sandbox is not enforced yet; command runs with full user privileges.",
            ),
            #[cfg(target_os = "windows")]
            SandboxType::Windows => {
                Some("Windows sandbox is not enforced yet; command runs with full user privileges.")
            }
            _ => None,
        }
    }

    /// Check if a command failure was due to sandbox denial.
    pub fn was_denied(sandbox_type: SandboxType, exit_code: i32, stderr: &str) -> bool {
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let _ = (exit_code, stderr);

        match sandbox_type {
            SandboxType::None => false,

            #[cfg(target_os = "macos")]
            SandboxType::MacosSeatbelt => seatbelt::detect_denial(exit_code, stderr),

            #[cfg(target_os = "linux")]
            SandboxType::LinuxLandlock => landlock::detect_denial(exit_code, stderr),

            #[cfg(target_os = "linux")]
            SandboxType::LinuxBwrap => bwrap::detect_denial(exit_code, stderr),

            #[cfg(target_os = "windows")]
            SandboxType::Windows => windows::detect_denial(exit_code, stderr),
        }
    }

    /// Get a human-readable description of why a command was blocked.
    pub fn denial_message(sandbox_type: SandboxType, stderr: &str) -> String {
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let _ = stderr;

        match sandbox_type {
            SandboxType::None => "Command failed (no sandbox)".to_string(),

            #[cfg(target_os = "macos")]
            SandboxType::MacosSeatbelt => {
                if stderr.contains("file-write") {
                    "Sandbox blocked write access. The command tried to write to a protected location.".to_string()
                } else if stderr.contains("network") {
                    "Sandbox blocked network access. Enable network_access in sandbox policy if needed.".to_string()
                } else {
                    format!(
                        "Sandbox blocked operation: {}",
                        stderr.lines().next().unwrap_or("unknown")
                    )
                }
            }

            #[cfg(target_os = "linux")]
            SandboxType::LinuxLandlock => {
                if stderr.contains("Permission denied") {
                    "Landlock blocked access. The command tried to access a restricted path."
                        .to_string()
                } else {
                    format!(
                        "Landlock blocked operation: {}",
                        stderr.lines().next().unwrap_or("unknown")
                    )
                }
            }

            #[cfg(target_os = "linux")]
            SandboxType::LinuxBwrap => {
                if stderr.contains("Read-only file system") {
                    "Sandbox blocked write access (bwrap read-only root). The command tried to write outside the policy's writable roots.".to_string()
                } else if stderr.contains("Network is unreachable")
                    || stderr.contains("Temporary failure in name resolution")
                {
                    "Sandbox blocked network access (bwrap). Enable network_access in sandbox policy if needed.".to_string()
                } else {
                    format!(
                        "Sandbox (bwrap) blocked operation: {}",
                        stderr.lines().next().unwrap_or("unknown")
                    )
                }
            }

            #[cfg(target_os = "windows")]
            SandboxType::Windows => {
                if stderr.contains("Access is denied") {
                    "Windows sandbox blocked access. The command lacked required privileges."
                        .to_string()
                } else if stderr.contains("network") {
                    "Windows sandbox blocked network access. Enable network_access in policy if needed."
                        .to_string()
                } else {
                    format!(
                        "Windows sandbox blocked operation: {}",
                        stderr.lines().next().unwrap_or("unknown")
                    )
                }
            }
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn mark_sandbox_policy_unenforced(exec: &mut ExecEnv) {
    exec.env
        .insert("DEEPSEEK_SANDBOX_UNENFORCED".to_string(), "1".to_string());
    // The command-level warning (ExecEnv::sandbox_enforcement_warning) is
    // also emitted per-execution. This log makes the degraded posture visible
    // in the runtime log for operators.
    tracing::warn!(
        target: "sandbox",
        "OS sandbox isolation is NOT enforced on this platform; command runs with full user privileges. {}",
        policy_degraded_mode_notice().unwrap_or("")
    );
}

impl ExecEnv {
    /// Warning when the selected sandbox type does not yet isolate the process (H12).
    #[must_use]
    pub fn sandbox_enforcement_warning(&self) -> Option<&'static str> {
        if self.enforced {
            return None;
        }
        SandboxManager::noop_sandbox_warning(self.sandbox_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected_shell_command(command: &str) -> Vec<String> {
        #[cfg(windows)]
        {
            let (shell, _) = windows_shell();
            let mut argv = vec![shell.to_string()];
            argv.extend(windows_shell_argv(shell, command));
            argv
        }
        #[cfg(not(windows))]
        {
            vec!["sh".to_string(), "-c".to_string(), command.to_string()]
        }
    }

    #[test]
    fn test_command_spec_shell() {
        let spec = CommandSpec::shell("echo hello", PathBuf::from("/tmp"), Duration::from_secs(30));

        #[cfg(windows)]
        {
            let (shell, _) = windows_shell();
            assert_eq!(spec.program, shell);
            assert_eq!(spec.args, windows_shell_argv(shell, "echo hello"));
            if is_powershell_program(shell) {
                assert!(spec.args.contains(&"-NoProfile".to_string()));
                assert!(spec.args.contains(&"-NonInteractive".to_string()));
            }
        }
        #[cfg(not(windows))]
        {
            assert_eq!(spec.program, "sh");
            assert_eq!(spec.args, vec!["-c", "echo hello"]);
        }
        assert_eq!(spec.display_command(), "echo hello");
    }

    #[test]
    fn test_command_spec_program() {
        let spec = CommandSpec::program(
            "cargo",
            vec!["build".to_string(), "--release".to_string()],
            PathBuf::from("/project"),
            Duration::from_secs(300),
        );

        assert_eq!(spec.program, "cargo");
        assert_eq!(spec.display_command(), "cargo build --release");
    }

    #[test]
    fn test_command_spec_builder() {
        let spec = CommandSpec::shell("test", PathBuf::from("."), Duration::from_secs(10))
            .with_policy(SandboxPolicy::ReadOnly)
            .with_env_var("FOO", "bar")
            .with_justification("Testing");

        assert!(matches!(spec.sandbox_policy, SandboxPolicy::ReadOnly));
        assert_eq!(spec.env.get("FOO"), Some(&"bar".to_string()));
        assert_eq!(spec.justification, Some("Testing".to_string()));
    }

    #[test]
    fn test_sandbox_manager_new() {
        let manager = SandboxManager::new();
        assert!(manager.sandbox_available.is_none());
    }

    #[test]
    fn test_sandbox_manager_select_sandbox() {
        let manager = SandboxManager::new();

        // DangerFullAccess should never sandbox
        let no_sandbox = manager.select_sandbox(&SandboxPolicy::DangerFullAccess);
        assert_eq!(no_sandbox, SandboxType::None);

        // ExternalSandbox should never sandbox
        let external = manager.select_sandbox(&SandboxPolicy::ExternalSandbox {
            network_access: true,
        });
        assert_eq!(external, SandboxType::None);
    }

    #[test]
    fn test_prepare_unsandboxed() {
        let manager = SandboxManager::new();
        let spec = CommandSpec::shell("echo test", PathBuf::from("/tmp"), Duration::from_secs(30))
            .with_policy(SandboxPolicy::DangerFullAccess);

        let env = manager.prepare(&spec);

        assert_eq!(env.sandbox_type, SandboxType::None);
        assert_eq!(env.command, expected_shell_command("echo test"));
        assert!(!env.is_sandboxed());
    }

    #[test]
    fn test_exec_env_helpers() {
        let env = ExecEnv {
            command: vec![
                "sandbox-exec".to_string(),
                "-p".to_string(),
                "policy".to_string(),
                "--".to_string(),
                "echo".to_string(),
                "hello".to_string(),
            ],
            cwd: PathBuf::from("/tmp"),
            env: HashMap::new(),
            timeout: Duration::from_secs(30),
            sandbox_type: SandboxType::None,
            policy: SandboxPolicy::default(),
            enforced: false,
            #[cfg(target_os = "windows")]
            windows_plan: None,
        };

        assert_eq!(env.program(), "sandbox-exec");
        assert_eq!(env.args().len(), 5);
    }

    #[test]
    fn test_sandbox_type_display() {
        assert_eq!(format!("{}", SandboxType::None), "none");

        #[cfg(target_os = "macos")]
        assert_eq!(format!("{}", SandboxType::MacosSeatbelt), "macos-seatbelt");
    }
}
