//! Layer-2 completion gate — harness-active manifest oracle (§6.1, §6.4).
//!
//! Execution is delegated to [`super::predicate`] (single oracle implementation).

use std::path::Path;

use zagens_core::long_horizon::CompletionGateVerifyEntry;

pub use super::predicate::{
    CompletionGateExec, VerifyExitClass, VerifyRunResult, run_manifest_verify_entry,
};

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

/// Run every manifest verify entry. Entries whose command was already run
/// successfully in this session (`recent_execs`) are trusted without re-exec.
pub async fn run_manifest_gate(
    workspace: &Path,
    entries: &[CompletionGateVerifyEntry],
    exec: &CompletionGateExec<'_>,
    recent_execs: &[String],
) -> ManifestGateResult {
    let mut results = Vec::with_capacity(entries.len());
    let mut failing_ids = Vec::new();

    for entry in entries {
        if exec
            .cancel_token
            .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
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

        if entry
            .cmd
            .as_deref()
            .is_some_and(|cmd| super::verify::manifest_command_trusted(cmd, recent_execs))
        {
            results.push(VerifyRunResult {
                id: entry.id.clone(),
                command_display: command_display(entry),
                exit_code: 0,
                exit_class: VerifyExitClass::Ok,
                stdout_tail: "(trusted recent exec)".to_string(),
                stderr_tail: String::new(),
            });
            continue;
        }

        let mut run = run_manifest_verify_entry(workspace, entry, exec).await;
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
        shell: zagens_core::long_horizon::ManifestShell::Default,
        timeout_secs: 300,
        source: zagens_core::long_horizon::VerifySource::Operator,
    };
    run_manifest_verify_entry(workspace, &entry, exec).await
}

fn command_display(entry: &CompletionGateVerifyEntry) -> String {
    if !entry.argv.is_empty() {
        return entry.argv.join(" ");
    }
    entry.cmd.clone().unwrap_or_else(|| "<empty>".to_string())
}
