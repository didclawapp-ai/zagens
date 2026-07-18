//! Run pending night-queue tasks (Phase 1a.1–1a.3).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use zagens_runtime_adapters::snapshot::{SnapshotId, SnapshotRepo};
use zagens_runtime_adapters::worktree::{
    WorktreesRuntimeConfig, create_session_worktree, is_git_repository, remove_worktree,
    resolve_git_root,
};

use crate::cli::context::CliContext;
use crate::cli::runner::run_queue_exec_agent;
use crate::config::Config;

use super::briefing;
use super::gate;
use super::model::{QueueEventRecord, QueueTask, QueueTaskStatus};
use super::run_control;
use super::store;

pub struct RunOptions {
    pub max_parallel: usize,
    pub use_worktree: bool,
    pub write_briefing: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            max_parallel: 1,
            use_worktree: true,
            write_briefing: true,
        }
    }
}

pub struct RunReport {
    pub ran: usize,
    pub passed: usize,
    pub failed: usize,
    pub canceled: usize,
}

pub async fn run_pending(ctx: &CliContext, config: &Config, opts: RunOptions) -> Result<RunReport> {
    let cancel = run_control::begin_run(&ctx.workspace);
    let claimed = match store::claim_pending_tasks(&ctx.workspace, opts.max_parallel) {
        Ok(tasks) => tasks,
        Err(err) => {
            run_control::end_run(&ctx.workspace);
            return Err(err);
        }
    };

    if claimed.is_empty() {
        run_control::end_run(&ctx.workspace);
        return Ok(RunReport {
            ran: 0,
            passed: 0,
            failed: 0,
            canceled: 0,
        });
    }

    super::hooks::dispatch_run_start(config, &ctx.workspace);

    let run_result: Result<RunReport> = async {
        let wt_config = config.worktrees_config().runtime_config();
        let model = config.default_model();
        let mut report = RunReport {
            ran: 0,
            passed: 0,
            failed: 0,
            canceled: 0,
        };
        let mut skip_rest = false;
        let mut restore_ids = Vec::new();

        for mut task in claimed {
            if skip_rest || cancel.is_cancelled() {
                restore_ids.push(task.id.clone());
                continue;
            }

            let (run_workspace, worktree_cleanup) =
                allocate_workspace(&ctx.workspace, &task, opts.use_worktree, &wt_config)?;
            task.worktree_path = Some(run_workspace.clone());

            let pre_snapshot = take_pre_snapshot(&run_workspace, &task.id);
            task.pre_snapshot_id = pre_snapshot.clone();
            store::persist_task(&ctx.workspace, task.clone())?;

            let exec_result = run_queue_exec_agent(
                config,
                &model,
                &task.prompt,
                run_workspace.clone(),
                config.max_subagents(),
                Some(cancel.clone()),
            )
            .await;

            if cancel.is_cancelled()
                || exec_result
                    .as_ref()
                    .err()
                    .is_some_and(|e| e.to_string().contains("canceled"))
            {
                let _ =
                    store::mark_canceled(&ctx.workspace, &task.id, "canceled by user during run");
                report.ran += 1;
                report.canceled += 1;
                skip_rest = true;
                cleanup_worktree(worktree_cleanup);
                continue;
            }

            if let Err(err) = exec_result {
                task.status = QueueTaskStatus::Failed;
                task.error = Some(err.to_string());
                task.finished_at = Some(Utc::now());
                store::persist_task(&ctx.workspace, task)?;
                report.ran += 1;
                report.failed += 1;
                cleanup_worktree(worktree_cleanup);
                continue;
            }

            if cancel.is_cancelled() {
                let _ =
                    store::mark_canceled(&ctx.workspace, &task.id, "canceled by user during run");
                report.ran += 1;
                report.canceled += 1;
                skip_rest = true;
                cleanup_worktree(worktree_cleanup);
                continue;
            }

            let gate_result = gate::run_gate(&run_workspace, &task.gate, None).await?;
            if cancel.is_cancelled() {
                let _ =
                    store::mark_canceled(&ctx.workspace, &task.id, "canceled by user during run");
                report.ran += 1;
                report.canceled += 1;
                skip_rest = true;
                cleanup_worktree(worktree_cleanup);
                continue;
            }

            task.gate_summary = Some(gate_result.summary.clone());
            task.finished_at = Some(Utc::now());

            // HL-2: persist harness_verify records on the queue event stream + sessions.db
            append_harness_verify_for_task(&ctx.workspace, &task.id, &gate_result.records);

            if gate_result.passed {
                store::append_event(
                    &ctx.workspace,
                    &QueueEventRecord {
                        kind: "queue_gate_result".into(),
                        ts: Utc::now(),
                        task_id: task.id.clone(),
                        payload: Some(serde_json::json!({
                            "pass": gate_result.passed,
                            "failing_predicate": gate_result.failing_predicate,
                            "harness_verify": gate_result.records,
                        })),
                    },
                )?;
                task.status = QueueTaskStatus::Passed;
                report.passed += 1;
            } else {
                // HL-3: snapshot restore aligns with verify-loop rollback_triggered
                let did_rollback = if let Some(ref snap) = pre_snapshot {
                    rollback_snapshot(&run_workspace, snap);
                    true
                } else {
                    false
                };
                let records = if did_rollback {
                    crate::long_horizon::harness_verify_loop::mark_records_rollback(
                        gate_result.records.clone(),
                    )
                } else {
                    gate_result.records.clone()
                };
                if did_rollback {
                    append_harness_verify_for_task(&ctx.workspace, &task.id, &records);
                }
                store::append_event(
                    &ctx.workspace,
                    &QueueEventRecord {
                        kind: "queue_gate_result".into(),
                        ts: Utc::now(),
                        task_id: task.id.clone(),
                        payload: Some(serde_json::json!({
                            "pass": gate_result.passed,
                            "failing_predicate": gate_result.failing_predicate,
                            "harness_verify": records,
                            "rollback_triggered": did_rollback,
                        })),
                    },
                )?;
                if did_rollback {
                    store::append_event(
                        &ctx.workspace,
                        &QueueEventRecord {
                            kind: "queue_rollback".into(),
                            ts: Utc::now(),
                            task_id: task.id.clone(),
                            payload: Some(serde_json::json!({
                                "snapshot_id": pre_snapshot,
                                "reason": gate_result.failing_predicate,
                            })),
                        },
                    )?;
                }
                task.status = QueueTaskStatus::RolledBack;
                task.error = gate_result.suggestion.clone();
                report.failed += 1;
            }

            report.ran += 1;
            store::persist_task(&ctx.workspace, task)?;
            cleanup_worktree(worktree_cleanup);
        }

        if !restore_ids.is_empty() {
            store::restore_pending(&ctx.workspace, &restore_ids)?;
        }

        let doc = store::finalize_run(&ctx.workspace, Utc::now())?;

        if opts.write_briefing {
            briefing::write_briefing_to_handoff(&ctx.workspace, &doc)?;
        }

        Ok(report)
    }
    .await;

    run_control::end_run(&ctx.workspace);

    match &run_result {
        Ok(rep) => super::hooks::dispatch_run_end(config, &ctx.workspace, rep, None),
        Err(err) => super::hooks::dispatch_run_end(
            config,
            &ctx.workspace,
            &RunReport {
                ran: 0,
                passed: 0,
                failed: 0,
                canceled: 0,
            },
            Some(&err.to_string()),
        ),
    }

    run_result
}

struct WorktreeCleanup {
    git_root: PathBuf,
    worktree_path: PathBuf,
    remove: bool,
}

fn allocate_workspace(
    base: &Path,
    task: &QueueTask,
    use_worktree: bool,
    wt_config: &WorktreesRuntimeConfig,
) -> Result<(PathBuf, Option<WorktreeCleanup>)> {
    if !use_worktree || !wt_config.enabled || !is_git_repository(base) {
        return Ok((base.to_path_buf(), None));
    }
    let git_root = resolve_git_root(base)?;
    let name = format!("queue-{}", &task.id[task.id.len().saturating_sub(8)..]);
    let wt = create_session_worktree(&git_root, wt_config, &name)
        .with_context(|| format!("worktree for task {}", task.id))?;
    Ok((
        wt.worktree_path.clone(),
        Some(WorktreeCleanup {
            git_root,
            worktree_path: wt.worktree_path,
            remove: false,
        }),
    ))
}

fn cleanup_worktree(cleanup: Option<WorktreeCleanup>) {
    if let Some(c) = cleanup
        && c.remove
    {
        let _ = remove_worktree(&c.git_root, &c.worktree_path, true);
    }
}

fn take_pre_snapshot(workspace: &Path, task_id: &str) -> Option<String> {
    SnapshotRepo::open_or_init(workspace)
        .ok()
        .and_then(|repo| repo.snapshot(&format!("queue-pre:{task_id}")).ok())
        .map(|id| id.0)
}

fn rollback_snapshot(workspace: &Path, snapshot_id: &str) {
    if let Ok(repo) = SnapshotRepo::open_or_init(workspace) {
        let _ = repo.restore(&SnapshotId(snapshot_id.to_string()));
    }
}

/// HL-2: write `HarnessVerify` rows into the shared sessions kernel_events log.
fn append_harness_verify_for_task(
    _workspace: &Path,
    task_id: &str,
    records: &[crate::long_horizon::harness_verify_loop::HarnessVerifyRecord],
) {
    if records.is_empty() {
        return;
    }
    let turn_id = format!("queue:{task_id}");
    crate::harness::telemetry::append_harness_verify_records(&turn_id, records);
}
