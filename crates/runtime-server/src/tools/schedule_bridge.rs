//! Tool plan → DAG scheduler view (kernel-v2 M4).

use std::collections::HashSet;

use serde_json::Value;
use zagens_core::engine::dispatch::{
    ToolParallelPlanFlags, should_parallelize_tool_batch as core_should_parallelize,
};
use zagens_core::engine::turn_loop::ToolExecutionPlan;
use zagens_tools::{
    DagPlanView, ScheduleResource, build_execution_waves, record_scheduler_shadow_diff,
};

use crate::command_safety::{SafetyLevel, analyze_command};
use crate::config::ToolsSchedulerMode;

/// Runtime context for schedule resource inference (§8.1.1 shell dynamic footprint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScheduleContext {
    /// Whether the next `exec_shell` would run under an enforced OS sandbox.
    pub sandbox_enforced: bool,
}

fn normalize_path(raw: &str) -> String {
    raw.trim().replace('\\', "/").to_ascii_lowercase()
}

fn path_field(input: &Value) -> Option<String> {
    for key in ["path", "file_path", "target_path", "file", "filename"] {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                return Some(normalize_path(trimmed));
            }
        }
    }
    None
}

fn workspace_scan_tool(name: &str) -> bool {
    matches!(
        name,
        "grep"
            | "glob"
            | "glob_files"
            | "list_dir"
            | "file_search"
            | "search_files"
            | "find_files"
            | "git_status"
            | "git_diff"
            | "git_log"
    )
}

fn shell_tool(name: &str) -> bool {
    matches!(
        name,
        "exec_shell" | "exec_shell_wait" | "exec_shell_interact"
    )
}

/// §8.1.1: read-only shell scheduling only when command analysis is Safe and sandbox enforced.
#[must_use]
pub fn shell_schedule_read_only(tool_name: &str, input: &Value, sandbox_enforced: bool) -> bool {
    if !shell_tool(tool_name) || !sandbox_enforced {
        return false;
    }
    let Some(command) = input.get("command").and_then(Value::as_str) else {
        return false;
    };
    analyze_command(command).level == SafetyLevel::Safe
}

/// Infer read/write resource sets for scheduling (conservative when unknown).
#[must_use]
pub fn extract_schedule_resources(
    tool_name: &str,
    input: &Value,
    read_only: bool,
) -> (HashSet<ScheduleResource>, HashSet<ScheduleResource>) {
    let mut reads = HashSet::new();
    let mut writes = HashSet::new();

    if let Some(path) = path_field(input) {
        if read_only {
            reads.insert(ScheduleResource::Path(path));
        } else {
            writes.insert(ScheduleResource::Path(path));
        }
        return (reads, writes);
    }

    if workspace_scan_tool(tool_name) || (read_only && tool_name.starts_with("grep")) {
        reads.insert(ScheduleResource::WorkspaceScan);
        return (reads, writes);
    }

    if read_only {
        reads.insert(ScheduleResource::WorkspaceScan);
    } else {
        writes.insert(ScheduleResource::WorkspaceWrite);
    }
    (reads, writes)
}

/// Build a [`DagPlanView`] from a turn-loop execution plan.
#[must_use]
pub fn dag_plan_view(plan: &ToolExecutionPlan, ctx: &ScheduleContext) -> DagPlanView {
    let shell_read = shell_schedule_read_only(&plan.name, &plan.input, ctx.sandbox_enforced);
    let effective_read_only = plan.read_only || shell_read;
    let (reads, writes) = extract_schedule_resources(&plan.name, &plan.input, effective_read_only);
    let parallel_eligible = effective_read_only
        && (plan.supports_parallel || shell_read)
        && !plan.approval_required
        && !plan.interactive;
    DagPlanView {
        index: plan.index,
        parallel_eligible,
        reads,
        writes,
    }
}

fn legacy_parallel_batch(plans: &[ToolExecutionPlan]) -> bool {
    let flags: Vec<ToolParallelPlanFlags> = plans
        .iter()
        .map(|plan| ToolParallelPlanFlags {
            read_only: plan.read_only,
            supports_parallel: plan.supports_parallel,
            approval_required: plan.approval_required,
            interactive: plan.interactive,
        })
        .collect();
    core_should_parallelize(&flags)
}

#[must_use]
pub fn legacy_execution_groups(plans: &[ToolExecutionPlan]) -> Vec<Vec<usize>> {
    if plans.is_empty() {
        return Vec::new();
    }
    if legacy_parallel_batch(plans) {
        return vec![(0..plans.len()).collect()];
    }
    (0..plans.len()).map(|i| vec![i]).collect()
}

#[must_use]
pub fn dag_execution_groups(plans: &[ToolExecutionPlan], ctx: &ScheduleContext) -> Vec<Vec<usize>> {
    if plans.is_empty() {
        return Vec::new();
    }
    if plans.len() == 1 {
        return vec![vec![plans[0].index]];
    }
    let views: Vec<_> = plans.iter().map(|p| dag_plan_view(p, ctx)).collect();
    build_execution_waves(&views)
}

/// Resolve batch execution groups (legacy / shadow / dag).
#[must_use]
pub fn resolve_execution_groups(
    scheduler: ToolsSchedulerMode,
    plans: &[ToolExecutionPlan],
    ctx: &ScheduleContext,
) -> Vec<Vec<usize>> {
    let legacy = legacy_execution_groups(plans);
    match scheduler {
        ToolsSchedulerMode::Legacy => legacy,
        ToolsSchedulerMode::Dag => dag_execution_groups(plans, ctx),
        ToolsSchedulerMode::Shadow => {
            let dag = dag_execution_groups(plans, ctx);
            record_scheduler_shadow_diff(&legacy, &dag);
            legacy
        }
    }
}

#[must_use]
pub fn wave_parallel_allowed(
    plans: &[ToolExecutionPlan],
    group: &[usize],
    ctx: &ScheduleContext,
) -> bool {
    if group.len() <= 1 {
        return false;
    }
    let views: Vec<_> = group
        .iter()
        .filter_map(|&idx| plans.get(idx).map(|p| dag_plan_view(p, ctx)))
        .collect();
    zagens_tools::wave_parallel_eligible(&views, group)
}

/// Split one DAG wave into execution sub-groups (M4 approval branch rule).
///
/// All auto-eligible tools in the wave run in one parallel batch first (when
/// there are two or more). Approval-gated and other non-parallel tools then run
/// serially in index order — they do not block unrelated reads in the parallel
/// batch.
#[must_use]
pub fn split_wave_execution_subgroups(
    plans: &[ToolExecutionPlan],
    wave: &[usize],
    ctx: &ScheduleContext,
) -> Vec<Vec<usize>> {
    if wave.len() <= 1 {
        return vec![wave.to_vec()];
    }
    let mut sorted = wave.to_vec();
    sorted.sort_unstable();

    let is_eligible = |idx: usize| -> bool { dag_plan_view(&plans[idx], ctx).parallel_eligible };
    let parallel: Vec<usize> = sorted.iter().copied().filter(|&i| is_eligible(i)).collect();

    if parallel.len() > 1 {
        let mut subgroups = vec![parallel];
        for idx in sorted {
            if !is_eligible(idx) {
                subgroups.push(vec![idx]);
            }
        }
        return subgroups;
    }

    let mut subgroups: Vec<Vec<usize>> = Vec::new();
    for idx in sorted {
        subgroups.push(vec![idx]);
    }
    subgroups
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::HashSet;
    use zagens_core::engine::turn_loop::ToolExecutionPlan;
    use zagens_tools::{ScheduleResource, scheduler_shadow_stats};

    fn plan(name: &str, input: Value, read_only: bool) -> ToolExecutionPlan {
        ToolExecutionPlan {
            index: 0,
            id: "t1".to_string(),
            name: name.to_string(),
            input,
            caller: None,
            interactive: false,
            approval_required: false,
            approval_description: String::new(),
            supports_parallel: read_only,
            read_only,
            blocked_error: None,
            guard_result: None,
        }
    }

    fn plan_at(index: usize, name: &str, input: Value, read_only: bool) -> ToolExecutionPlan {
        ToolExecutionPlan {
            index,
            ..plan(name, input, read_only)
        }
    }

    fn plan_with_approval(
        index: usize,
        name: &str,
        input: Value,
        read_only: bool,
    ) -> ToolExecutionPlan {
        ToolExecutionPlan {
            index,
            approval_required: true,
            ..plan(name, input, read_only)
        }
    }

    const ENFORCED: ScheduleContext = ScheduleContext {
        sandbox_enforced: true,
    };

    #[test]
    fn read_file_yields_path_read() {
        let (reads, writes) =
            extract_schedule_resources("read_file", &json!({"path": "src/Lib.rs"}), true);
        assert!(writes.is_empty());
        assert_eq!(
            reads,
            HashSet::from([ScheduleResource::Path("src/lib.rs".to_string())])
        );
    }

    #[test]
    fn edit_file_yields_path_write() {
        let (_, writes) =
            extract_schedule_resources("edit_file", &json!({"path": "src/main.rs"}), false);
        assert_eq!(
            writes,
            HashSet::from([ScheduleResource::Path("src/main.rs".to_string())])
        );
    }

    #[test]
    fn grep_without_path_is_workspace_scan() {
        let (reads, _) = extract_schedule_resources("grep", &json!({"pattern": "foo"}), true);
        assert_eq!(reads, HashSet::from([ScheduleResource::WorkspaceScan]));
    }

    #[test]
    fn dag_view_marks_parallel_eligible_reads() {
        let view = dag_plan_view(&plan("read_file", json!({"path": "a.rs"}), true), &ENFORCED);
        assert!(view.parallel_eligible);
        assert!(
            view.reads
                .contains(&ScheduleResource::Path("a.rs".to_string()))
        );
    }

    #[test]
    fn safe_shell_with_enforced_sandbox_is_schedule_read_only() {
        assert!(shell_schedule_read_only(
            "exec_shell",
            &json!({"command": "git status"}),
            true,
        ));
        let view = dag_plan_view(
            &plan("exec_shell", json!({"command": "git status"}), false),
            &ENFORCED,
        );
        assert!(view.parallel_eligible);
        assert!(view.reads.contains(&ScheduleResource::WorkspaceScan));
        assert!(view.writes.is_empty());
    }

    #[test]
    fn safe_shell_without_enforced_sandbox_stays_write_locked() {
        let ctx = ScheduleContext {
            sandbox_enforced: false,
        };
        let view = dag_plan_view(
            &plan("exec_shell", json!({"command": "git status"}), false),
            &ctx,
        );
        assert!(!view.parallel_eligible);
        assert!(view.writes.contains(&ScheduleResource::WorkspaceWrite));
    }

    #[test]
    fn chained_shell_is_not_schedule_read_only() {
        assert!(!shell_schedule_read_only(
            "exec_shell",
            &json!({"command": "git status && git diff"}),
            true,
        ));
    }

    #[test]
    fn dag_mixed_batch_splits_waves() {
        let plans = vec![
            plan_at(0, "read_file", json!({"path": "a"}), true),
            plan_at(1, "read_file", json!({"path": "b"}), true),
            plan_at(2, "edit_file", json!({"path": "a"}), false),
            plan_at(3, "grep", json!({"pattern": "x"}), true),
        ];
        let waves = dag_execution_groups(&plans, &ENFORCED);
        assert_eq!(waves, vec![vec![0, 1, 3], vec![2]]);
    }

    #[test]
    fn shadow_records_diff_without_changing_legacy_groups() {
        let before = scheduler_shadow_stats();
        let plans = vec![
            plan_at(0, "read_file", json!({"path": "a"}), true),
            plan_at(1, "read_file", json!({"path": "b"}), true),
            plan_at(2, "edit_file", json!({"path": "a"}), false),
            plan_at(3, "grep", json!({"pattern": "x"}), true),
        ];
        let groups = resolve_execution_groups(ToolsSchedulerMode::Shadow, &plans, &ENFORCED);
        assert_eq!(groups, legacy_execution_groups(&plans));
        let after = scheduler_shadow_stats();
        assert!(after.comparisons > before.comparisons);
        assert!(after.diffs > before.diffs);
    }

    #[test]
    fn wave_split_parallelizes_reads_before_approval_gated_tool() {
        let plans = vec![
            plan_at(0, "read_file", json!({"path": "a"}), true),
            plan_with_approval(1, "write_file", json!({"path": "b"}), false),
            plan_at(2, "read_file", json!({"path": "c"}), true),
        ];
        let wave = vec![0, 1, 2];
        let sub = split_wave_execution_subgroups(&plans, &wave, &ENFORCED);
        assert_eq!(sub, vec![vec![0, 2], vec![1]]);
    }

    #[test]
    fn wave_split_single_read_stays_serial() {
        let plans = vec![
            plan_at(0, "read_file", json!({"path": "a"}), true),
            plan_with_approval(1, "write_file", json!({"path": "b"}), false),
        ];
        let sub = split_wave_execution_subgroups(&plans, &[0, 1], &ENFORCED);
        assert_eq!(sub, vec![vec![0], vec![1]]);
    }
}
