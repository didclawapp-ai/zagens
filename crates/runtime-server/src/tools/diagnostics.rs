//! Workspace diagnostics tool: `diagnostics`.
//!
//! This tool gathers lightweight, best-effort environment information without
//! failing hard when optional commands are unavailable.

use std::env;
use std::path::Path;
use std::process::Command;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::misc_inputs::diagnostics_input_schema;
use super::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
};

/// Tool for collecting workspace and toolchain diagnostics.
pub struct DiagnosticsTool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxPosture {
    /// Top-level `sandbox_mode` from config when load succeeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox_mode: Option<String>,
    /// Whether the effective shell policy allows outbound network.
    shell_network_access: bool,
    /// `[windows] sandbox` in config: `elevated`, `unelevated`, or `auto`.
    #[serde(skip_serializing_if = "Option::is_none")]
    windows_sandbox_configured: Option<String>,
    /// Resolved runtime mode for the next `exec_shell`: `elevated` or `unelevated`.
    #[serde(skip_serializing_if = "Option::is_none")]
    windows_sandbox_effective: Option<String>,
    /// Elevated setup artifacts present (`zagens sandbox setup`).
    #[serde(skip_serializing_if = "Option::is_none")]
    windows_setup_complete: Option<bool>,
    /// `DEEPSEEK_SANDBOX` env value injected into sandboxed shell children.
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_shell_env_marker: Option<String>,
    /// Parent env inheritance for the next `exec_shell` (`all` / `core` / `none`).
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_shell_env_inherit: Option<String>,
    /// Whether the parent process currently exposes MSVC/SDK linker env (`LIB`, `INCLUDE`, …).
    #[serde(skip_serializing_if = "Option::is_none")]
    exec_shell_parent_toolchain_env: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiagnosticsOutput {
    workspace_root: String,
    current_dir: Option<String>,
    current_dir_error: Option<String>,
    git_repo: bool,
    git_branch: Option<String>,
    git_error: Option<String>,
    sandbox_available: bool,
    sandbox_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sandbox_posture: Option<SandboxPosture>,
    rustc_version: Option<String>,
    cargo_version: Option<String>,
    /// User-trusted external paths the agent may access from this workspace
    /// (`/trust add <path>` from the slash command, persisted in
    /// `~/.deepseek/workspace-trust.json`). See issue #29.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    trusted_external_paths: Vec<String>,
    /// Kernel-v2 M3 shadow counters when `[tools] policy = "shadow"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    policy_shadow: Option<PolicyShadowDiagnostics>,
    /// Kernel-v2 M4 shadow counters when `[tools] scheduler = "shadow"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    scheduler_shadow: Option<SchedulerShadowDiagnostics>,
    /// Kernel-v2 Phase 2-A shadow counters when `[tools] compiler = "shadow"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_compiler_shadow: Option<ContextCompilerShadowDiagnostics>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ContextCompilerShadowDiagnostics {
    context_compiler: String,
    comparisons: u64,
    static_diffs: u64,
    full_diffs: u64,
    static_diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SchedulerShadowDiagnostics {
    tools_scheduler: String,
    comparisons: u64,
    diffs: u64,
    diff_rate_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PolicyShadowDiagnostics {
    tools_policy: String,
    comparisons: u64,
    diffs: u64,
    diff_rate_pct: f64,
}

#[derive(Debug, Clone, Default)]
struct GitProbe {
    detected: bool,
    branch: Option<String>,
    error: Option<String>,
}

#[async_trait]
impl ToolSpec for DiagnosticsTool {
    fn name(&self) -> &'static str {
        "diagnostics"
    }

    fn description(&self) -> &'static str {
        "Report workspace info, git detection, sandbox availability (including Windows elevated vs unelevated when applicable), and Rust toolchain versions."
    }

    fn input_schema(&self) -> Value {
        diagnostics_input_schema()
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadOnly]
    }

    fn approval_requirement(&self) -> ApprovalRequirement {
        ApprovalRequirement::Auto
    }

    fn supports_parallel(&self) -> bool {
        true
    }

    async fn execute(&self, _input: Value, context: &ToolContext) -> Result<ToolResult, ToolError> {
        let workspace_root = context.workspace.display().to_string();

        let (current_dir, current_dir_error) = match env::current_dir() {
            Ok(dir) => (Some(dir.display().to_string()), None),
            Err(err) => (None, Some(err.to_string())),
        };

        let sandbox_type = crate::sandbox::get_platform_sandbox().map(|s| s.to_string());
        let sandbox_available = sandbox_type.is_some();

        // C4: the git probe + version probes all shell out via blocking
        // `Command::output()`. Run them on a blocking thread so we don't stall a
        // tokio worker (this tool advertises `supports_parallel`).
        let workspace = context.workspace.clone();
        let (git, rustc_version, cargo_version) = tokio::task::spawn_blocking(move || {
            let git = probe_git(&workspace);
            let rustc = probe_version("rustc", &["--version"], &workspace);
            let cargo = probe_version("cargo", &["--version"], &workspace);
            (git, rustc, cargo)
        })
        .await
        .map_err(|e| ToolError::execution_failed(format!("diagnostics task failed: {e}")))?;

        let trusted_external_paths = context
            .trusted_external_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        let sandbox_posture = probe_sandbox_posture(context);
        let policy_shadow = probe_policy_shadow();
        let scheduler_shadow = probe_scheduler_shadow();
        let context_compiler_shadow = probe_context_compiler_shadow();
        let diagnostics = DiagnosticsOutput {
            workspace_root,
            current_dir,
            current_dir_error,
            git_repo: git.detected,
            git_branch: git.branch,
            git_error: git.error,
            sandbox_available,
            sandbox_type,
            sandbox_posture,
            rustc_version,
            cargo_version,
            trusted_external_paths,
            policy_shadow,
            scheduler_shadow,
            context_compiler_shadow,
        };

        ToolResult::json(&diagnostics).map_err(|e| ToolError::execution_failed(e.to_string()))
    }
}

// === Helpers ===

fn shell_network_access(context: &ToolContext) -> bool {
    context
        .elevated_sandbox_policy
        .as_ref()
        .is_some_and(|policy| policy.has_network_access())
}

fn probe_policy_shadow() -> Option<PolicyShadowDiagnostics> {
    let config = crate::config::Config::load(None, None).ok()?;
    let mode = config.tools_policy_mode();
    if mode != crate::config::ToolsPolicyMode::Shadow {
        return None;
    }
    let stats = zagens_tools::policy_shadow_stats();
    let diff_rate_pct = if stats.comparisons == 0 {
        0.0
    } else {
        (stats.diffs as f64 / stats.comparisons as f64) * 100.0
    };
    Some(PolicyShadowDiagnostics {
        tools_policy: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        diffs: stats.diffs,
        diff_rate_pct,
    })
}

fn probe_scheduler_shadow() -> Option<SchedulerShadowDiagnostics> {
    let config = crate::config::Config::load(None, None).ok()?;
    let mode = config.tools_scheduler_mode();
    if mode != crate::config::ToolsSchedulerMode::Shadow {
        return None;
    }
    let stats = zagens_tools::scheduler_shadow_stats();
    let diff_rate_pct = if stats.comparisons == 0 {
        0.0
    } else {
        (stats.diffs as f64 / stats.comparisons as f64) * 100.0
    };
    Some(SchedulerShadowDiagnostics {
        tools_scheduler: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        diffs: stats.diffs,
        diff_rate_pct,
    })
}

fn probe_context_compiler_shadow() -> Option<ContextCompilerShadowDiagnostics> {
    let config = crate::config::Config::load(None, None).ok()?;
    let mode = config.context_compiler_mode();
    if mode != zagens_core::engine::ContextCompilerMode::Shadow {
        return None;
    }
    let stats = crate::context_compiler_shadow::context_compiler_shadow_stats();
    let static_diff_rate_pct = if stats.comparisons == 0 {
        0.0
    } else {
        (stats.static_diffs as f64 / stats.comparisons as f64) * 100.0
    };
    Some(ContextCompilerShadowDiagnostics {
        context_compiler: mode.as_str().to_string(),
        comparisons: stats.comparisons,
        static_diffs: stats.static_diffs,
        full_diffs: stats.full_diffs,
        static_diff_rate_pct,
    })
}

fn probe_sandbox_posture(context: &ToolContext) -> Option<SandboxPosture> {
    let shell_network_access = shell_network_access(context);

    let config = crate::config::Config::load(None, None).ok()?;
    let sandbox_mode = config.sandbox_mode.clone();
    let exec_shell_env_inherit = Some(
        crate::shell_environment::default_exec_shell_inherit()
            .as_str()
            .to_string(),
    );
    let exec_shell_parent_toolchain_env =
        Some(crate::shell_environment::parent_has_toolchain_env());

    #[cfg(windows)]
    {
        let configured = crate::config::resolve_windows_sandbox_mode(&config);
        let effective = crate::config::effective_windows_sandbox_execution_label(configured);
        let setup_complete = zagens_windows_sandbox::sandbox_setup_is_complete(
            &zagens_windows_sandbox::zagens_home(),
        );
        Some(SandboxPosture {
            sandbox_mode,
            shell_network_access,
            windows_sandbox_configured: Some(
                crate::config::windows_sandbox_configured_label(&config).to_string(),
            ),
            windows_sandbox_effective: Some(effective.to_string()),
            windows_setup_complete: Some(setup_complete),
            exec_shell_env_marker: Some(
                crate::config::exec_shell_sandbox_env_marker(configured).to_string(),
            ),
            exec_shell_env_inherit,
            exec_shell_parent_toolchain_env,
        })
    }

    #[cfg(not(windows))]
    Some(SandboxPosture {
        sandbox_mode,
        shell_network_access,
        windows_sandbox_configured: None,
        windows_sandbox_effective: None,
        windows_setup_complete: None,
        exec_shell_env_marker: None,
        exec_shell_env_inherit,
        exec_shell_parent_toolchain_env,
    })
}

fn probe_git(workspace: &Path) -> GitProbe {
    let rev_parse = run_command("git", &["rev-parse", "--is-inside-work-tree"], workspace);
    match rev_parse {
        CommandProbe::Success(out) => {
            if out.trim() != "true" {
                return GitProbe {
                    detected: false,
                    branch: None,
                    error: Some(format!("unexpected git rev-parse output: {out}")),
                };
            }
            let branch = run_command("git", &["rev-parse", "--abbrev-ref", "HEAD"], workspace)
                .into_success();
            GitProbe {
                detected: true,
                branch,
                error: None,
            }
        }
        CommandProbe::Failed { stderr, .. } => GitProbe {
            detected: false,
            branch: None,
            error: stderr,
        },
        CommandProbe::Missing => GitProbe {
            detected: false,
            branch: None,
            error: Some("git is not installed or not in PATH".to_string()),
        },
    }
}

fn probe_version(program: &str, args: &[&str], cwd: &Path) -> Option<String> {
    run_command(program, args, cwd).into_success()
}

enum CommandProbe {
    Success(String),
    Failed { stderr: Option<String> },
    Missing,
}

impl CommandProbe {
    fn into_success(self) -> Option<String> {
        match self {
            CommandProbe::Success(out) => Some(out),
            CommandProbe::Failed { .. } | CommandProbe::Missing => None,
        }
    }
}

fn run_command(program: &str, args: &[&str], cwd: &Path) -> CommandProbe {
    let output = Command::new(program).args(args).current_dir(cwd).output();
    let output = match output {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return CommandProbe::Missing,
        Err(_) => return CommandProbe::Failed { stderr: None },
    };

    if output.status.success() {
        CommandProbe::Success(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        CommandProbe::Failed {
            stderr: if stderr.is_empty() {
                None
            } else {
                Some(stderr)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use tempfile::tempdir;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn init_git_repo(root: &Path) {
        let run = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .expect("git should spawn");
            assert!(status.success(), "git {:?} failed", args);
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "test@example.com"]);
        run(&["config", "user.name", "Test User"]);
        fs::write(root.join("README.md"), "init\n").expect("write");
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "init"]);
    }

    #[tokio::test]
    async fn diagnostics_runs_best_effort_outside_git_repo() {
        let tmp = tempdir().expect("tempdir");
        let ctx = ToolContext::new(tmp.path());
        let tool = DiagnosticsTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(result.success);

        let parsed: DiagnosticsOutput =
            serde_json::from_str(&result.content).expect("tool result should be json");
        assert_eq!(parsed.workspace_root, tmp.path().display().to_string());
    }

    #[tokio::test]
    async fn diagnostics_detects_git_repo_when_available() {
        if !git_available() {
            return;
        }
        let tmp = tempdir().expect("tempdir");
        init_git_repo(tmp.path());

        let ctx = ToolContext::new(tmp.path());
        let tool = DiagnosticsTool;
        let result = tool.execute(json!({}), &ctx).await.expect("execute");
        assert!(result.success);

        let parsed: DiagnosticsOutput =
            serde_json::from_str(&result.content).expect("tool result should be json");
        assert!(parsed.git_repo);
        assert!(!parsed.git_branch.as_deref().unwrap_or("").is_empty());
    }
}
