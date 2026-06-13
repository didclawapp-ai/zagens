//! Resource-dependency DAG scheduler (kernel-v2 M4).
//!
//! Groups tool plans into execution waves. Plans within a wave may run in
//! parallel when `parallel_eligible`; later waves start only after all deps in
//! earlier waves complete.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

/// Coarse scheduling resource (path-level refinement lives in runtime bridge).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ScheduleResource {
    /// Whole-workspace scan reads (grep/glob/list without isolated path).
    WorkspaceScan,
    /// Canonical workspace-relative path (normalized lowercase).
    Path(String),
    /// Fallback write lock when path cannot be inferred.
    WorkspaceWrite,
}

/// Lightweight plan view for scheduling (no execution state).
#[derive(Debug, Clone)]
pub struct DagPlanView {
    pub index: usize,
    pub parallel_eligible: bool,
    pub reads: HashSet<ScheduleResource>,
    pub writes: HashSet<ScheduleResource>,
}

impl DagPlanView {
    #[must_use]
    pub fn depends_on(&self, prior: &Self) -> bool {
        resource_conflict(&prior.writes, &self.reads)
            || resource_conflict(&prior.writes, &self.writes)
            || resource_conflict(&prior.reads, &self.writes)
            || coarse_workspace_write_before_scan(&prior.writes, &self.reads)
    }
}

fn coarse_workspace_write_before_scan(
    writes: &HashSet<ScheduleResource>,
    reads: &HashSet<ScheduleResource>,
) -> bool {
    reads.contains(&ScheduleResource::WorkspaceScan)
        && writes.contains(&ScheduleResource::WorkspaceWrite)
}

fn resource_conflict(left: &HashSet<ScheduleResource>, right: &HashSet<ScheduleResource>) -> bool {
    left.iter()
        .any(|l| right.iter().any(|r| l.conflicts_with(r)))
}

impl ScheduleResource {
    fn conflicts_with(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Path(a), Self::Path(b)) => a == b,
            (Self::WorkspaceWrite, Self::WorkspaceWrite) => true,
            (Self::WorkspaceWrite, Self::Path(_)) | (Self::Path(_), Self::WorkspaceWrite) => true,
            _ => false,
        }
    }
}

/// Build execution waves as plan index groups (proposal §8.2 DAG batching).
///
/// Returns one index per inner vec. Empty input yields empty vec.
#[must_use]
pub fn build_execution_waves(plans: &[DagPlanView]) -> Vec<Vec<usize>> {
    if plans.is_empty() {
        return Vec::new();
    }
    if plans.len() == 1 {
        return vec![vec![plans[0].index]];
    }

    let n = plans.len();
    let mut deps: Vec<HashSet<usize>> = vec![HashSet::new(); n];
    for (j, plan_j) in plans.iter().enumerate() {
        for (_i, plan_i) in plans.iter().enumerate().take(j) {
            if plan_j.depends_on(plan_i) {
                deps[j].insert(plan_i.index);
            }
        }
    }

    let mut remaining: HashSet<usize> = plans.iter().map(|p| p.index).collect();
    let mut satisfied: HashSet<usize> = HashSet::new();
    let mut waves: Vec<Vec<usize>> = Vec::new();

    while !remaining.is_empty() {
        let mut wave: Vec<usize> = remaining
            .iter()
            .copied()
            .filter(|idx| deps[*idx].iter().all(|dep| satisfied.contains(dep)))
            .collect();
        if wave.is_empty() {
            // Cycle or logic bug — fall back to serial program order for safety.
            wave = remaining.iter().copied().take(1).collect();
        }
        wave.sort_unstable();
        for idx in &wave {
            remaining.remove(idx);
            satisfied.insert(*idx);
        }
        waves.push(wave);
    }

    waves
}

static SCHEDULER_SHADOW_COMPARISONS: AtomicU64 = AtomicU64::new(0);
static SCHEDULER_SHADOW_DIFFS: AtomicU64 = AtomicU64::new(0);

/// Shadow-mode counters (`tools.scheduler = "shadow"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerShadowStats {
    pub comparisons: u64,
    pub diffs: u64,
}

/// Record a shadow comparison between legacy and DAG execution groups.
pub fn record_scheduler_shadow_diff(legacy: &[Vec<usize>], dag: &[Vec<usize>]) {
    SCHEDULER_SHADOW_COMPARISONS.fetch_add(1, Ordering::Relaxed);
    if legacy != dag {
        SCHEDULER_SHADOW_DIFFS.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(?legacy, ?dag, "scheduler shadow diff");
    }
}

#[must_use]
pub fn scheduler_shadow_stats() -> SchedulerShadowStats {
    SchedulerShadowStats {
        comparisons: SCHEDULER_SHADOW_COMPARISONS.load(Ordering::Relaxed),
        diffs: SCHEDULER_SHADOW_DIFFS.load(Ordering::Relaxed),
    }
}

/// Whether every plan in a wave may use the parallel executor path.
#[must_use]
pub fn wave_parallel_eligible(plans: &[DagPlanView], wave: &[usize]) -> bool {
    !wave.is_empty()
        && wave.iter().all(|idx| {
            plans
                .iter()
                .find(|p| p.index == *idx)
                .is_some_and(|p| p.parallel_eligible)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn path_read(index: usize, path: &str) -> DagPlanView {
        DagPlanView {
            index,
            parallel_eligible: true,
            reads: HashSet::from([ScheduleResource::Path(path.to_string())]),
            writes: HashSet::new(),
        }
    }

    fn path_write(index: usize, path: &str) -> DagPlanView {
        DagPlanView {
            index,
            parallel_eligible: false,
            reads: HashSet::new(),
            writes: HashSet::from([ScheduleResource::Path(path.to_string())]),
        }
    }

    fn workspace_scan(index: usize) -> DagPlanView {
        DagPlanView {
            index,
            parallel_eligible: true,
            reads: HashSet::from([ScheduleResource::WorkspaceScan]),
            writes: HashSet::new(),
        }
    }

    #[test]
    fn proposal_example_read_batch_then_write() {
        // batch = [read A, read B, edit A, grep C] → {read A, read B, grep C} ∥ then edit A
        let plans = vec![
            path_read(0, "a"),
            path_read(1, "b"),
            path_write(2, "a"),
            workspace_scan(3),
        ];
        let waves = build_execution_waves(&plans);
        assert_eq!(waves.len(), 2, "expected two waves, got {waves:?}");
        assert_eq!(waves[0], vec![0, 1, 3]);
        assert_eq!(waves[1], vec![2]);
        assert!(wave_parallel_eligible(&plans, &waves[0]));
        assert!(!wave_parallel_eligible(&plans, &waves[1]));
    }

    #[test]
    fn legacy_all_readonly_single_wave() {
        let plans = vec![path_read(0, "a"), path_read(1, "b"), path_read(2, "c")];
        let waves = build_execution_waves(&plans);
        assert_eq!(waves, vec![vec![0, 1, 2]]);
    }

    #[test]
    fn write_serializes_on_same_path() {
        let plans = vec![path_write(0, "x"), path_read(1, "x")];
        let waves = build_execution_waves(&plans);
        assert_eq!(waves, vec![vec![0], vec![1]]);
    }

    #[test]
    fn independent_paths_parallelize() {
        let plans = vec![path_write(0, "a"), path_read(1, "b")];
        let waves = build_execution_waves(&plans);
        assert_eq!(waves, vec![vec![0, 1]]);
    }

    #[test]
    fn empty_batch() {
        assert!(build_execution_waves(&[]).is_empty());
    }

    #[test]
    fn wave_order_is_deterministic() {
        let plans: Vec<DagPlanView> = (0..4).map(|i| path_read(i, &format!("f{i}"))).collect();
        let w1 = build_execution_waves(&plans);
        let w2 = build_execution_waves(&plans);
        assert_eq!(w1, w2);
        assert_eq!(w1, vec![vec![0, 1, 2, 3]]);
    }

    #[test]
    fn coarse_workspace_write_blocks_scan() {
        let plans = vec![
            DagPlanView {
                index: 0,
                parallel_eligible: false,
                reads: HashSet::new(),
                writes: HashSet::from([ScheduleResource::WorkspaceWrite]),
            },
            workspace_scan(1),
        ];
        let waves = build_execution_waves(&plans);
        assert_eq!(waves, vec![vec![0], vec![1]]);
    }

    #[test]
    fn deps_queue_matches_manual_topo() {
        let plans = vec![
            path_read(0, "src/lib.rs"),
            path_write(1, "src/lib.rs"),
            path_read(2, "README.md"),
        ];
        let mut q: VecDeque<usize> = build_execution_waves(&plans)
            .into_iter()
            .flat_map(|w| w.into_iter())
            .collect();
        assert_eq!(q.pop_front(), Some(0));
        assert_eq!(q.pop_front(), Some(2));
        assert_eq!(q.pop_front(), Some(1));
    }
}
