//! Shell-backed command execution for predicates and layer-2 gate.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;
use zagens_core::long_horizon::ManifestShell;

use crate::command_safety::{SafetyLevel, analyze_command};
use crate::tools::shell::{SharedShellManager, ShellStatus};

use super::manifest_exec::CompletionGateExec;
use super::verify_result::{VerifyExitClass, VerifyRunResult, classify_exit, tail};

#[must_use]
pub fn wrap_shell_command(shell: ManifestShell, cmd: &str) -> Option<String> {
    match shell {
        ManifestShell::Pwsh => Some(format!(
            "powershell -NoProfile -Command '{}'",
            cmd.replace('\'', "''")
        )),
        ManifestShell::Bash => Some(format!("bash -lc '{}'", cmd.replace('\'', "'\\''"))),
        ManifestShell::Cmd => Some(format!("cmd /C \"{cmd}\"")),
        ManifestShell::None | ManifestShell::Default => None,
    }
}

pub async fn run_shell_command(
    workspace: &Path,
    command: &str,
    id: &str,
    display: &str,
    timeout_ms: u64,
    exec: &CompletionGateExec<'_>,
) -> VerifyRunResult {
    let safety = analyze_command(command);
    if matches!(safety.level, SafetyLevel::Dangerous) {
        return VerifyRunResult {
            id: id.to_string(),
            command_display: display.to_string(),
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: format!("blocked dangerous command: {}", safety.reasons.join("; ")),
        };
    }
    run_via_shell_manager(workspace, command, id, display, timeout_ms, exec).await
}

pub async fn run_argv_command(
    workspace: &Path,
    argv: &[String],
    id: &str,
    display: &str,
    timeout_ms: u64,
    cancel: Option<&CancellationToken>,
) -> VerifyRunResult {
    if argv.is_empty() {
        return VerifyRunResult {
            id: id.to_string(),
            command_display: display.to_string(),
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: "empty argv".to_string(),
        };
    }
    let safety = analyze_command(&argv.join(" "));
    if matches!(safety.level, SafetyLevel::Dangerous) {
        return VerifyRunResult {
            id: id.to_string(),
            command_display: display.to_string(),
            exit_code: 1,
            exit_class: VerifyExitClass::Infra,
            stdout_tail: String::new(),
            stderr_tail: format!("blocked dangerous command: {}", safety.reasons.join("; ")),
        };
    }

    let workspace = workspace.to_path_buf();
    let argv = argv.to_vec();
    let id_owned = id.to_string();
    let display_owned = display.to_string();
    let cancel = cancel.cloned();

    tokio::task::spawn_blocking(move || {
        if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return VerifyRunResult {
                id: id_owned.clone(),
                command_display: display_owned.clone(),
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
                    id: id_owned.clone(),
                    command_display: display_owned.clone(),
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
                    id: id_owned,
                    command_display: display_owned,
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
                        id: id_owned,
                        command_display: display_owned,
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
                    id: id_owned,
                    command_display: display_owned,
                    exit_code: code,
                    exit_class: classify_exit(code, "", false),
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                }
            }
            Err(e) => VerifyRunResult {
                id: id_owned,
                command_display: display_owned,
                exit_code: 1,
                exit_class: VerifyExitClass::Infra,
                stdout_tail: String::new(),
                stderr_tail: e.to_string(),
            },
        }
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
                    if let Ok(mut mut_manager) = shell_manager.lock() {
                        let _ = mut_manager.kill(&task_id);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pwsh_wrap_escapes_single_quotes() {
        let wrapped = wrap_shell_command(ManifestShell::Pwsh, "echo 'hi'").unwrap();
        assert!(wrapped.contains("echo ''hi''"));
    }

    #[test]
    fn cmd_wrap_double_quotes() {
        assert_eq!(
            wrap_shell_command(ManifestShell::Cmd, "go build ./...").unwrap(),
            "cmd /C \"go build ./...\""
        );
    }
}
