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
}

pub async fn run_pending(ctx: &CliContext, config: &Config, opts: RunOptions) -> Result<RunReport> {
    let claimed = store::claim_pending_tasks(&ctx.workspace, opts.max_parallel)?;

    if claimed.is_empty() {
        return Ok(RunReport {
            ran: 0,
            passed: 0,
            failed: 0,
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
        };

        for mut task in claimed {
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
            )
            .await;

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

            let gate_result = gate::run_gate(&run_workspace, &task.gate, None).await?;
            task.gate_summary = Some(gate_result.summary.clone());
            task.finished_at = Some(Utc::now());

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
                        })),
                    },
                )?;
                task.status = QueueTaskStatus::Passed;
                report.passed += 1;
            } else {
                if let Some(ref snap) = pre_snapshot {
                    rollback_snapshot(&run_workspace, snap);
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
                        })),
                    },
                )?;
                if pre_snapshot.is_some() {
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

        let doc = store::finalize_run(&ctx.workspace, Utc::now())?;

        if opts.write_briefing {
            briefing::write_briefing_to_handoff(&ctx.workspace, &doc)?;
        }

        Ok(report)
    }
    .await;

    match &run_result {
        Ok(rep) => super::hooks::dispatch_run_end(config, &ctx.workspace, rep, None),
        Err(err) => super::hooks::dispatch_run_end(
            config,
            &ctx.workspace,
            &RunReport {
                ran: 0,
                passed: 0,
                failed: 0,
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
