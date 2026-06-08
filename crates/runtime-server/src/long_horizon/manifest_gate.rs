//! Layer-2 completion gate — harness-active manifest oracle (§6.1, §6.4).

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio_util::sync::CancellationToken;
use zagens_core::long_horizon::{CompletionGateVerifyEntry, ManifestShell};

use crate::command_safety::{SafetyLevel, analyze_command};
use crate::tools::shell::{SharedShellManager, ShellStatus};

const DEFAULT_TIMEOUT_SECS: u32 = 300;
const STDOUT_TAIL_MAX: usize = 2_048;

/// Per-verify exit classification (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyExitClass {
    Ok,
    Assertion,
    Infra,
    Timeout,
    Cancelled,
}

/// One harness-active verify run.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyRunResult {
    pub id: String,
    pub command_display: String,
    pub exit_code: i32,
    pub exit_class: VerifyExitClass,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

/// Run an ad-hoc verify command for deliverable `optional_verify_cmd` (§6.2).
pub async fn run_optional_verify_cmd(
    workspace: &Path,
    deliverable_id: &str,
    command: &str,
    exec: &CompletionGateExec<'_>,
) -> VerifyRunResult {
    let entry = CompletionGateVerifyEntry {
        id: format!("{deliverable_id}_optional_verify"),
        cmd: Some(command.to_string()),
        argv: Vec::new(),
        shell: ManifestShell::Default,
        timeout_secs: DEFAULT_TIMEOUT_SECS,
        source: zagens_core::long_horizon::VerifySource::Operator,
    };
    run_single_verify(workspace, &entry, exec).await
}

/// Cached layer-2 outcome for the current gate evaluation round (§7.7).
#[derive(Debug, Clone)]
pub struct ManifestGateResult {
    pub passed: bool,
    pub results: Vec<VerifyRunResult>,
    pub failing_ids: Vec<String>,
}

impl ManifestGateResult {
    #[must_use]
    pub fn all_green(&self) -> bool {
        self.passed && self.failing_ids.is_empty()
    }
}

/// Shell + argv execution context for the completion gate.
pub struct CompletionGateExec<'a> {
    pub shell_manager: &'a SharedShellManager,
    pub cancel_token: Option<&'a CancellationToken>,
}

/// Run every manifest verify entry; does not trust model-side history.
pub async fn run_manifest_gate(
    workspace: &Path,
    entries: &[CompletionGateVerifyEntry],
    exec: &CompletionGateExec<'_>,
) -> ManifestGateResult {
    let mut results = Vec::with_capacity(entries.len());
    let mut failing_ids = Vec::new();

    for entry in entries {
        if exec
            .cancel_token
            .is_some_and(CancellationToken::is_cancelled)
        {
            results.push(VerifyRunResult {
                id: entry.id.clone(),
                command_display: command_display(entry),
                exit_code: -1,
                exit_class: VerifyExitClass::Cancelled,
                stdout_tail: String::new(),
                stderr_tail: "cancelled".to_string(),
            });
            failing_ids.push(entry.id.clone());
            continue;
        }

        let mut run = run_single_verify(workspace, entry, exec).await;
        if run.exit_code == 0
            && run.exit_class == VerifyExitClass::Ok
            && let Some(msg) = super::go_toolchain_audit::audit_go_test_output(
                &run.command_display,
                &run.stdout_tail,
                &run.stderr_tail,
            )
        {
            run.exit_code = 1;
            run.exit_class = VerifyExitClass::Assertion;
            if run.stderr_tail.is_empty() {
                run.stderr_tail = msg;
            } else {
                run.stderr_tail = format!("{}\n{}", run.stderr_tail, msg);
            }
        }
        if run.exit_code != 0 || run.exit_class != VerifyExitClass::Ok {
            failing_ids.push(entry.id.clone());
        }
        results.push(run);
    }

    ManifestGateResult {
        passed: failing_ids.is_empty(),
        results,
        failing_ids,
    }
}

fn command_display(entry: &CompletionGateVerifyEntry) -> String {
    if !entry.argv.is_empty() {
        return entry.argv.join(" ");
    }
    entry.cmd.clone().unwrap_or_else(|| "<empty>".to_string())
}

async fn run_single_verify(
    workspace: &Path,
    entry: &CompletionGateVerifyEntry,
    exec: &CompletionGateExec<'_>,
) -> VerifyRunResult {
    let timeout_ms = u64::from(entry.timeout_secs.clamp(1, 600)) * 1000;
    let display = command_display(entry);

    // Per-command working directory by toolchain marker. The gate's single
    // `cmd_root` cannot serve a polyglot/nested layout — e.g. an npm frontend at
    // the workspace root with a Cargo backend in `src-tauri/`. Without this, the
    // model's `cargo check` replay ran at the root and failed with
    // "could not find Cargo.toml" (a false RED) even though the code built fine.
    let run_dir = super::generic_gate::resolve_command_root(workspace, &display);
    let workspace = run_dir.as_path();

    if let Some(cmd) = entry.cmd.as_deref().filter(|c| !c.trim().is_empty())
        && let Some(native) = super::verify_platform::try_native_verify(workspace, cmd)
    {
        return VerifyRunResult {
            id: entry.id.clone(),
            command_display: native.command_display,
            exit_code: native.exit_code,
            exit_class: match native.exit_class {
                super::verify_platform::NativeExitClass::Ok => VerifyExitClass::Ok,
                super::verify_platform::NativeExitClass::Assertion => VerifyExitClass::Assertion,
                super::verify_platform::NativeExitClass::Infra => VerifyExitClass::Infra,
            },
            stdout_tail: native.stdout_tail,
            stderr_tail: native.stderr_tail,
        };
    }

    if entry.shell == ManifestShell::None {
        if entry.argv.is_empty() {
            return VerifyRunResult {
                id: entry.id.clone(),
                command_display: display,
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: "shell=none requires argv".to_string(),
            };
        }
        // The argv path runs the binary directly (no shell, no string splitting),
        // but must still pass the same danger screen as the shell path (§6.4): the
        // manifest comes from a trusted source, yet a dangerous trusted command is
        // still refused rather than executed.
        let safety = analyze_command(&entry.argv.join(" "));
        if matches!(safety.level, SafetyLevel::Dangerous) {
            return VerifyRunResult {
                id: entry.id.clone(),
                command_display: display,
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: format!("blocked dangerous command: {}", safety.reasons.join("; ")),
            };
        }
        return run_argv_direct(workspace, entry, timeout_ms, exec.cancel_token).await;
    }

    let Some(cmd) = entry.cmd.as_deref().filter(|c| !c.trim().is_empty()) else {
        return VerifyRunResult {
            id: entry.id.clone(),
            command_display: display,
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: "missing cmd (use argv when shell=none)".to_string(),
        };
    };

    if let Some(cmdline) = wrap_shell_command(entry.shell, cmd) {
        let safety = analyze_command(&cmdline);
        if matches!(safety.level, SafetyLevel::Dangerous) {
            return VerifyRunResult {
                id: entry.id.clone(),
                command_display: display,
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: format!("blocked dangerous command: {}", safety.reasons.join("; ")),
            };
        }
        return run_via_shell_manager(workspace, &cmdline, &entry.id, &display, timeout_ms, exec)
            .await;
    }

    let adapted = super::verify_platform::adapt_verify_command_for_platform(cmd);
    let cmd_to_run = adapted.as_ref();

    let safety = analyze_command(cmd_to_run);
    if matches!(safety.level, SafetyLevel::Dangerous) {
        return VerifyRunResult {
            id: entry.id.clone(),
            command_display: display,
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: format!("blocked dangerous command: {}", safety.reasons.join("; ")),
        };
    }

    run_via_shell_manager(workspace, cmd_to_run, &entry.id, &display, timeout_ms, exec).await
}

/// Wrap a manifest `cmd` for an explicit shell. For complex commands prefer
/// `shell = "none"` + `argv` (§6.4) — these wrappers cover the common case but
/// shell quoting on Windows is intrinsically fragile.
fn wrap_shell_command(shell: ManifestShell, cmd: &str) -> Option<String> {
    match shell {
        // PowerShell single-quotes: escape embedded `'` by doubling it.
        ManifestShell::Pwsh => Some(format!(
            "powershell -NoProfile -Command '{}'",
            cmd.replace('\'', "''")
        )),
        // bash single-quotes: close-quote, escaped quote, reopen (`'\''`).
        ManifestShell::Bash => Some(format!("bash -lc '{}'", cmd.replace('\'', "'\\''"))),
        // cmd.exe: wrap in double quotes so spaces/operators stay intact.
        ManifestShell::Cmd => Some(format!("cmd /C \"{cmd}\"")),
        ManifestShell::None | ManifestShell::Default => None,
    }
}

async fn run_via_shell_manager(
    workspace: &Path,
    command: &str,
    id: &str,
    display: &str,
    timeout_ms: u64,
    exec: &CompletionGateExec<'_>,
) -> VerifyRunResult {
    let workspace = workspace.to_path_buf();
    let command = command.to_string();
    let id_owned = id.to_string();
    let display_owned = display.to_string();
    let shell_manager = Arc::clone(exec.shell_manager);
    let cancel = exec.cancel_token.cloned();

    tokio::task::spawn_blocking(move || {
        poll_foreground_shell(
            &shell_manager,
            &workspace,
            &command,
            &id_owned,
            &display_owned,
            timeout_ms,
            cancel.as_ref(),
        )
    })
    .await
    .unwrap_or_else(|e| VerifyRunResult {
        id: id.to_string(),
        command_display: display.to_string(),
        exit_code: 1,
        exit_class: VerifyExitClass::Infra,
        stdout_tail: String::new(),
        stderr_tail: format!("spawn_blocking failed: {e}"),
    })
}

fn poll_foreground_shell(
    shell_manager: &SharedShellManager,
    workspace: &Path,
    command: &str,
    id: &str,
    display: &str,
    timeout_ms: u64,
    cancel: Option<&CancellationToken>,
) -> VerifyRunResult {
    let work_dir = workspace.display().to_string();
    let spawned = {
        let mut manager = match shell_manager.lock() {
            Ok(m) => m,
            Err(_) => {
                return VerifyRunResult {
                    id: id.to_string(),
                    command_display: display.to_string(),
                    exit_code: 1,
                    exit_class: VerifyExitClass::Infra,
                    stdout_tail: String::new(),
                    stderr_tail: "shell manager lock poisoned".to_string(),
                };
            }
        };
        manager.execute_with_options_env(
            command,
            Some(&work_dir),
            timeout_ms,
            true,
            None,
            false,
            None,
            HashMap::new(),
        )
    };

    let Ok(spawned) = spawned else {
        return VerifyRunResult {
            id: id.to_string(),
            command_display: display.to_string(),
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: spawned
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "spawn failed".to_string()),
        };
    };

    let Some(task_id) = spawned.task_id else {
        return VerifyRunResult {
            id: id.to_string(),
            command_display: display.to_string(),
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: "no task_id from shell spawn".to_string(),
        };
    };

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            if let Ok(mut m) = shell_manager.lock() {
                let _ = m.kill(&task_id);
            }
            return VerifyRunResult {
                id: id.to_string(),
                command_display: display.to_string(),
                exit_code: -1,
                exit_class: VerifyExitClass::Cancelled,
                stdout_tail: String::new(),
                stderr_tail: "cancelled".to_string(),
            };
        }

        let snapshot = {
            let mut manager = match shell_manager.lock() {
                Ok(m) => m,
                Err(_) => break,
            };
            manager.get_output(&task_id, false, 0)
        };

        let Ok(snapshot) = snapshot else {
            return VerifyRunResult {
                id: id.to_string(),
                command_display: display.to_string(),
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: snapshot.err().map(|e| e.to_string()).unwrap_or_default(),
            };
        };

        match snapshot.status {
            ShellStatus::Running => {
                if Instant::now() >= deadline {
                    if let Ok(mut m) = shell_manager.lock() {
                        let _ = m.kill(&task_id);
                    }
                    return VerifyRunResult {
                        id: id.to_string(),
                        command_display: display.to_string(),
                        exit_code: 124,
                        exit_class: VerifyExitClass::Timeout,
                        stdout_tail: tail(&snapshot.stdout),
                        stderr_tail: "timeout".to_string(),
                    };
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            ShellStatus::Completed
            | ShellStatus::Failed
            | ShellStatus::Killed
            | ShellStatus::TimedOut => {
                let code = snapshot.exit_code.unwrap_or(-1);
                let timed_out = snapshot.status == ShellStatus::TimedOut;
                let exit_class = classify_exit(code, &snapshot.stderr, timed_out);
                return VerifyRunResult {
                    id: id.to_string(),
                    command_display: display.to_string(),
                    exit_code: code,
                    exit_class,
                    stdout_tail: tail(&snapshot.stdout),
                    stderr_tail: tail(&snapshot.stderr),
                };
            }
        }
    }

    VerifyRunResult {
        id: id.to_string(),
        command_display: display.to_string(),
        exit_code: 1,
        exit_class: VerifyExitClass::Infra,
        stdout_tail: String::new(),
        stderr_tail: "shell manager lock poisoned".to_string(),
    }
}

async fn run_argv_direct(
    workspace: &Path,
    entry: &CompletionGateVerifyEntry,
    timeout_ms: u64,
    cancel: Option<&CancellationToken>,
) -> VerifyRunResult {
    let workspace = workspace.to_path_buf();
    let argv = entry.argv.clone();
    let id = entry.id.clone();
    let display = command_display(entry);
    let cancel = cancel.cloned();

    tokio::task::spawn_blocking(move || {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return VerifyRunResult {
                id,
                command_display: display,
                exit_code: -1,
                exit_class: VerifyExitClass::Cancelled,
                stdout_tail: String::new(),
                stderr_tail: "cancelled".to_string(),
            };
        }

        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..])
            .current_dir(&workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                return VerifyRunResult {
                    id,
                    command_display: display,
                    exit_code: 1,
                    exit_class: VerifyExitClass::Infra,
                    stdout_tail: String::new(),
                    stderr_tail: e.to_string(),
                };
            }
        };

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let output = loop {
            if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
                let _ = child.kill();
                return VerifyRunResult {
                    id,
                    command_display: display,
                    exit_code: -1,
                    exit_class: VerifyExitClass::Cancelled,
                    stdout_tail: String::new(),
                    stderr_tail: "cancelled".to_string(),
                };
            }
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    return VerifyRunResult {
                        id,
                        command_display: display,
                        exit_code: 124,
                        exit_class: VerifyExitClass::Timeout,
                        stdout_tail: String::new(),
                        stderr_tail: "timeout".to_string(),
                    };
                }
                Ok(None) => {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => break Err(e),
            }
        };

        match output {
            Ok(status) => {
                let code = status.code().unwrap_or(-1);
                VerifyRunResult {
                    id,
                    command_display: display,
                    exit_code: code,
                    exit_class: classify_exit(code, "", false),
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                }
            }
            Err(e) => VerifyRunResult {
                id,
                command_display: display,
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: e.to_string(),
            },
        }
    })
    .await
    .unwrap_or_else(|e| VerifyRunResult {
        id: entry.id.clone(),
        command_display: command_display(entry),
        exit_code: 1,
        exit_class: VerifyExitClass::Infra,
        stdout_tail: String::new(),
        stderr_tail: format!("spawn_blocking failed: {e}"),
    })
}

fn classify_exit(code: i32, stderr: &str, timed_out: bool) -> VerifyExitClass {
    if timed_out {
        return VerifyExitClass::Timeout;
    }
    if code == 0 {
        return VerifyExitClass::Ok;
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("not found")
        || lower.contains("not recognized")
        || lower.contains("no such file")
        || lower.contains("segmentation fault")
        || lower.contains("access violation")
    {
        VerifyExitClass::Infra
    } else {
        VerifyExitClass::Assertion
    }
}

fn tail(s: &str) -> String {
    if s.len() <= STDOUT_TAIL_MAX {
        s.to_string()
    } else {
        format!("...{}", &s[s.len().saturating_sub(STDOUT_TAIL_MAX)..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_zero_is_ok() {
        assert_eq!(classify_exit(0, "", false), VerifyExitClass::Ok);
    }

    #[test]
    fn classify_test_failure_as_assertion() {
        assert_eq!(
            classify_exit(1, "FAIL: TestFoo", false),
            VerifyExitClass::Assertion
        );
    }

    #[test]
    fn classify_command_not_found_as_infra() {
        assert_eq!(
            classify_exit(127, "bash: foo: command not found", false),
            VerifyExitClass::Infra
        );
        assert_eq!(
            classify_exit(1, "'go' is not recognized as ...", false),
            VerifyExitClass::Infra
        );
    }

    #[test]
    fn pwsh_wrap_escapes_single_quotes() {
        let wrapped = wrap_shell_command(ManifestShell::Pwsh, "echo 'hi'").unwrap();
        assert!(wrapped.starts_with("powershell -NoProfile -Command '"));
        assert!(wrapped.contains("echo ''hi''"));
    }

    #[test]
    fn bash_wrap_escapes_single_quotes() {
        let wrapped = wrap_shell_command(ManifestShell::Bash, "echo 'hi'").unwrap();
        assert!(wrapped.starts_with("bash -lc '"));
        assert!(wrapped.contains("'\\''hi'\\''"));
    }

    #[test]
    fn cmd_wrap_double_quotes() {
        assert_eq!(
            wrap_shell_command(ManifestShell::Cmd, "go build ./...").unwrap(),
            "cmd /C \"go build ./...\""
        );
    }

    #[test]
    fn shell_none_has_no_wrapper() {
        assert!(wrap_shell_command(ManifestShell::None, "x").is_none());
        assert!(wrap_shell_command(ManifestShell::Default, "x").is_none());
    }
}
