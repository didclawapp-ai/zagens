//! Layer-2 manifest verify entry execution (extracted from `manifest_gate`).

use std::path::Path;

use zagens_core::long_horizon::{CompletionGateVerifyEntry, ManifestShell};

use super::manifest_exec::CompletionGateExec;
use super::shell_exec::{run_argv_command, run_shell_command, wrap_shell_command};
use super::verify_result::{VerifyExitClass, VerifyRunResult};

fn command_display(entry: &CompletionGateVerifyEntry) -> String {
    if !entry.argv.is_empty() {
        return entry.argv.join(" ");
    }
    entry.cmd.clone().unwrap_or_else(|| "<empty>".to_string())
}

fn native_to_verify(
    id: &str,
    native: super::super::verify_platform::NativeVerifyResult,
) -> VerifyRunResult {
    VerifyRunResult {
        id: id.to_string(),
        command_display: native.command_display,
        exit_code: native.exit_code,
        exit_class: match native.exit_class {
            super::super::verify_platform::NativeExitClass::Ok => VerifyExitClass::Ok,
            super::super::verify_platform::NativeExitClass::Assertion => VerifyExitClass::Assertion,
            super::super::verify_platform::NativeExitClass::Infra => VerifyExitClass::Infra,
        },
        stdout_tail: native.stdout_tail,
        stderr_tail: native.stderr_tail,
    }
}

/// Run one manifest verify entry — **single implementation** for layer-2 gate.
pub async fn run_manifest_verify_entry(
    workspace: &Path,
    entry: &CompletionGateVerifyEntry,
    exec: &CompletionGateExec<'_>,
) -> VerifyRunResult {
    let timeout_ms = u64::from(entry.timeout_secs.clamp(1, 600)) * 1000;
    let display = command_display(entry);

    let run_dir = super::super::generic_gate::resolve_command_root(workspace, &display);
    let workspace = run_dir.as_path();

    if let Some(cmd) = entry.cmd.as_deref().filter(|c| !c.trim().is_empty())
        && let Some(native) = super::super::verify_platform::try_native_verify(workspace, cmd)
    {
        return native_to_verify(&entry.id, native);
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
        return run_argv_command(
            workspace,
            &entry.argv,
            &entry.id,
            &display,
            timeout_ms,
            exec.cancel_token,
        )
        .await;
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
        return run_shell_command(workspace, &cmdline, &entry.id, &display, timeout_ms, exec).await;
    }

    let adapted = super::super::verify_platform::adapt_verify_command_for_platform(cmd);
    run_shell_command(
        workspace,
        adapted.as_ref(),
        &entry.id,
        &display,
        timeout_ms,
        exec,
    )
    .await
}
