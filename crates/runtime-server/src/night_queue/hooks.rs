//! Night queue lifecycle hooks (Phase 1a · schedule integration).

use std::path::Path;

use crate::config::Config;
use crate::hooks::{HookContext, HookEvent, HookExecutor};

use super::RunReport;

pub fn dispatch_enqueue(config: &Config, workspace: &Path, task_id: &str, prompt: &str) {
    let ctx = HookContext::new()
        .with_workspace(workspace.to_path_buf())
        .with_message(prompt)
        .with_night_queue_action("enqueue")
        .with_night_queue_task_id(task_id);
    dispatch(config, HookEvent::NightQueueEnqueue, &ctx);
}

pub fn dispatch_run_start(config: &Config, workspace: &Path) {
    let ctx = HookContext::new()
        .with_workspace(workspace.to_path_buf())
        .with_night_queue_action("run");
    dispatch(config, HookEvent::NightQueueRunStart, &ctx);
}

pub fn dispatch_run_end(config: &Config, workspace: &Path, report: &RunReport, err: Option<&str>) {
    let mut ctx = HookContext::new()
        .with_workspace(workspace.to_path_buf())
        .with_night_queue_action("run")
        .with_night_queue_report(report.ran, report.passed, report.failed);
    if let Some(msg) = err {
        ctx = ctx.with_error(msg);
    }
    dispatch(config, HookEvent::NightQueueRunEnd, &ctx);
}

fn dispatch(config: &Config, event: HookEvent, ctx: &HookContext) {
    let hooks_cfg = config.hooks_config();
    if hooks_cfg.hooks.is_empty() {
        return;
    }
    let workspace = ctx
        .workspace
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let executor = HookExecutor::new(hooks_cfg, workspace);
    if !executor.has_hooks_for_event(event) {
        return;
    }
    executor.execute(event, ctx);
}
