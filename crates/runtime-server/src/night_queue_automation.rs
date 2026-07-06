//! Scheduled automation ↔ night queue bridge (Phase 1a · schedule integration).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::automation_manager::{
    AutomationRecord, AutomationRunRecord, AutomationRunStatus, AutomationTriggerKind,
    SharedAutomationManager,
};
use crate::cli::context::CliContext;
use crate::config::Config;
use crate::night_queue::{self, EnqueueGateInput, RunOptions, resolve_gate_specs};
use crate::utils::spawn_supervised;

pub fn workspace_for_automation(automation: &AutomationRecord) -> Result<PathBuf> {
    automation
        .cwds
        .first()
        .cloned()
        .filter(|p| !p.as_os_str().is_empty())
        .context("Night queue automations require a workspace (cwds[0])")
}

pub fn run_options_for_automation(automation: &AutomationRecord) -> RunOptions {
    RunOptions {
        max_parallel: 1,
        use_worktree: automation.use_worktree.unwrap_or(true),
        write_briefing: automation.write_briefing.unwrap_or(true),
    }
}

pub fn resolve_gate_for_automation(
    automation: &AutomationRecord,
) -> Result<Vec<night_queue::GatePredicateSpec>> {
    resolve_gate_specs(&EnqueueGateInput {
        gate: automation.gate.clone(),
        gate_file: None,
        gate_preset: automation.gate_preset.clone(),
    })
}

pub async fn enqueue_from_automation(
    config: &Config,
    automation: &AutomationRecord,
) -> Result<(String, String)> {
    let workspace = workspace_for_automation(automation)?;
    let gate = resolve_gate_for_automation(automation)?;
    let task = night_queue::enqueue(
        &workspace,
        automation.prompt.clone(),
        gate,
        automation.use_worktree.unwrap_or(true),
    )?;
    crate::night_queue::dispatch_enqueue(config, &workspace, &task.id, &automation.prompt);
    Ok((task.id, workspace.display().to_string()))
}

pub fn spawn_scheduled_queue_run(
    automations: SharedAutomationManager,
    config: Config,
    automation: AutomationRecord,
    run: AutomationRunRecord,
) {
    spawn_supervised(
        "automation-night-queue-run",
        std::panic::Location::caller(),
        async move {
            let workspace = match workspace_for_automation(&automation) {
                Ok(path) => path,
                Err(err) => {
                    mark_run_failed(&automations, &run, &automation.id, err.to_string()).await;
                    return;
                }
            };
            let ctx = CliContext {
                config: config.clone(),
                workspace: workspace.clone(),
            };
            let opts = run_options_for_automation(&automation);
            match night_queue::run_pending(&ctx, &config, opts).await {
                Ok(report) => {
                    mark_run_finished(
                        &automations,
                        &run,
                        &automation.id,
                        AutomationRunStatus::Completed,
                        Some(format!(
                            "ran={} passed={} failed={}",
                            report.ran, report.passed, report.failed
                        )),
                        None,
                    )
                    .await;
                }
                Err(err) => {
                    mark_run_finished(
                        &automations,
                        &run,
                        &automation.id,
                        AutomationRunStatus::Failed,
                        None,
                        Some(err.to_string()),
                    )
                    .await;
                }
            }
        },
    );
}

async fn mark_run_finished(
    automations: &SharedAutomationManager,
    run: &AutomationRunRecord,
    automation_id: &str,
    status: AutomationRunStatus,
    result_summary: Option<String>,
    error: Option<String>,
) {
    let manager = automations.lock().await;
    update_run_record(&manager, run, automation_id, status, result_summary, error);
}

async fn mark_run_failed(
    automations: &SharedAutomationManager,
    run: &AutomationRunRecord,
    automation_id: &str,
    error: String,
) {
    mark_run_finished(
        automations,
        run,
        automation_id,
        AutomationRunStatus::Failed,
        None,
        Some(error),
    )
    .await;
}

fn update_run_record(
    manager: &crate::automation_manager::AutomationManager,
    run: &AutomationRunRecord,
    automation_id: &str,
    status: AutomationRunStatus,
    result_summary: Option<String>,
    error: Option<String>,
) {
    let Ok(mut saved) = manager.get_run(automation_id, &run.id) else {
        return;
    };
    saved.status = status;
    saved.started_at = saved.started_at.or(Some(Utc::now()));
    saved.ended_at = Some(Utc::now());
    saved.result_summary = result_summary;
    saved.error = error;
    if let Err(err) = manager.save_run(&saved) {
        tracing::warn!(target: "automations", "failed to save night queue run: {err}");
        return;
    }
    if let Ok(mut automation) = manager.get_automation(automation_id) {
        automation.last_run_at = saved.ended_at;
        automation.updated_at = Utc::now();
        let _ = manager.save_automation(&automation);
    }
}

pub fn validate_night_queue_workspace(cwds: &[PathBuf]) -> Result<()> {
    if cwds.first().is_none_or(|p| p.as_os_str().is_empty()) {
        bail!("Night queue automations require a workspace (cwds)");
    }
    Ok(())
}

pub fn is_night_queue_trigger(kind: AutomationTriggerKind) -> bool {
    matches!(
        kind,
        AutomationTriggerKind::NightQueueEnqueue | AutomationTriggerKind::NightQueueRun
    )
}
