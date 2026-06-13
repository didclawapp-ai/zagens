//! Fine-grained resource lock targets for DAG scheduling (kernel-v2 M4).

use std::collections::HashSet;

use crate::dag_scheduler::ScheduleResource;

/// Shared vs exclusive lock for one scheduling resource slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceLockMode {
    Shared,
    Exclusive,
}

/// Stable sort key for deadlock-free lock acquisition order.
#[must_use]
pub fn resource_lock_order(resource: &ScheduleResource) -> (u8, String) {
    match resource {
        ScheduleResource::WorkspaceScan => (0, String::new()),
        ScheduleResource::Path(path) => (1, path.clone()),
        ScheduleResource::WorkspaceWrite => (2, String::new()),
    }
}

/// Resources to lock before executing one tool plan (writes → exclusive, reads → shared).
#[must_use]
pub fn resource_lock_targets(
    reads: &HashSet<ScheduleResource>,
    writes: &HashSet<ScheduleResource>,
) -> Vec<(ScheduleResource, ResourceLockMode)> {
    let mut out: Vec<(ScheduleResource, ResourceLockMode)> = writes
        .iter()
        .map(|resource| (resource.clone(), ResourceLockMode::Exclusive))
        .collect();
    for resource in reads {
        if !writes.contains(resource) {
            out.push((resource.clone(), ResourceLockMode::Shared));
        }
    }
    out.sort_by(|a, b| {
        resource_lock_order(&a.0)
            .cmp(&resource_lock_order(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_paths_are_exclusive_reads_shared() {
        let reads = HashSet::from([
            ScheduleResource::Path("a".to_string()),
            ScheduleResource::WorkspaceScan,
        ]);
        let writes = HashSet::from([ScheduleResource::Path("a".to_string())]);
        let targets = resource_lock_targets(&reads, &writes);
        assert_eq!(targets.len(), 2);
        assert!(
            targets
                .iter()
                .any(|(r, m)| r == &ScheduleResource::Path("a".to_string())
                    && *m == ResourceLockMode::Exclusive)
        );
        assert!(
            targets
                .iter()
                .any(|(r, m)| r == &ScheduleResource::WorkspaceScan
                    && *m == ResourceLockMode::Shared)
        );
    }

    #[test]
    fn distinct_read_paths_yield_two_shared_targets() {
        let reads = HashSet::from([
            ScheduleResource::Path("a".to_string()),
            ScheduleResource::Path("b".to_string()),
        ]);
        let targets = resource_lock_targets(&reads, &HashSet::new());
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|(_, m)| *m == ResourceLockMode::Shared));
    }
}
