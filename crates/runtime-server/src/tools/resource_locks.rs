//! Per-resource async locks for DAG tool execution (kernel-v2 M4).

use std::collections::HashSet;
use std::sync::Arc;

use tokio::sync::{Mutex, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock};
use zagens_tools::{ResourceLockMode, ScheduleResource, resource_lock_targets};

/// Holds lock guards for the duration of one tool execution.
pub struct ResourceLockGuardSet {
    _guards: Vec<ResourceLockGuard>,
}

enum ResourceLockGuard {
    GlobalRead(OwnedRwLockReadGuard<()>),
    GlobalWrite(OwnedRwLockWriteGuard<()>),
    SlotRead(OwnedRwLockReadGuard<()>),
    SlotWrite(OwnedRwLockWriteGuard<()>),
}

/// Lazily allocated per [`ScheduleResource`] lock slots.
#[derive(Default)]
pub struct ResourceLockRegistry {
    slots: Mutex<std::collections::HashMap<ScheduleResource, Arc<RwLock<()>>>>,
}

impl ResourceLockRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    async fn slot(&self, resource: &ScheduleResource) -> Arc<RwLock<()>> {
        let mut map = self.slots.lock().await;
        map.entry(resource.clone())
            .or_insert_with(|| Arc::new(RwLock::new(())))
            .clone()
    }

    /// Acquire locks for one tool. Falls back to the global batch lock when disabled.
    pub async fn acquire(
        &self,
        global: &Arc<RwLock<()>>,
        reads: &HashSet<ScheduleResource>,
        writes: &HashSet<ScheduleResource>,
        fine_grained: bool,
        supports_parallel: bool,
    ) -> ResourceLockGuardSet {
        if !fine_grained {
            return Self::acquire_global(global, supports_parallel, !writes.is_empty()).await;
        }

        let targets = resource_lock_targets(reads, writes);
        if targets.is_empty() {
            return Self::acquire_global(global, supports_parallel, false).await;
        }

        let mut guards = Vec::with_capacity(targets.len());
        for (resource, mode) in targets {
            let slot = self.slot(&resource).await;
            match mode {
                ResourceLockMode::Shared => {
                    guards.push(ResourceLockGuard::SlotRead(slot.read_owned().await));
                }
                ResourceLockMode::Exclusive => {
                    guards.push(ResourceLockGuard::SlotWrite(slot.write_owned().await));
                }
            }
        }
        ResourceLockGuardSet { _guards: guards }
    }

    /// Global batch lock (legacy path).
    pub async fn acquire_global(
        global: &Arc<RwLock<()>>,
        supports_parallel: bool,
        force_write: bool,
    ) -> ResourceLockGuardSet {
        if supports_parallel && !force_write {
            ResourceLockGuardSet {
                _guards: vec![ResourceLockGuard::GlobalRead(
                    global.clone().read_owned().await,
                )],
            }
        } else {
            ResourceLockGuardSet {
                _guards: vec![ResourceLockGuard::GlobalWrite(
                    global.clone().write_owned().await,
                )],
            }
        }
    }
}

/// Optional fine-grained lock plan for one tool execution.
#[derive(Clone)]
pub struct FineGrainedLockContext {
    pub registry: Arc<ResourceLockRegistry>,
    pub reads: HashSet<ScheduleResource>,
    pub writes: HashSet<ScheduleResource>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn distinct_path_reads_run_concurrently_under_fine_grained_locks() {
        let registry = Arc::new(ResourceLockRegistry::new());
        let global = Arc::new(RwLock::new(()));
        let reads_a = HashSet::from([ScheduleResource::Path("a".to_string())]);
        let reads_b = HashSet::from([ScheduleResource::Path("b".to_string())]);

        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let global_a = global.clone();
        let global_b = global.clone();

        let task_a = tokio::spawn(async move {
            let _guard = reg_a
                .acquire(&global_a, &reads_a, &HashSet::new(), true, true)
                .await;
            tokio::time::sleep(Duration::from_millis(80)).await;
        });
        let task_b = tokio::spawn(async move {
            let _guard = reg_b
                .acquire(&global_b, &reads_b, &HashSet::new(), true, true)
                .await;
            tokio::time::sleep(Duration::from_millis(80)).await;
        });

        let start = Instant::now();
        let (a, b) = tokio::join!(task_a, task_b);
        a.unwrap();
        b.unwrap();
        assert!(
            start.elapsed() < Duration::from_millis(140),
            "expected concurrent path reads, took {:?}",
            start.elapsed()
        );
    }

    #[tokio::test]
    async fn global_fallback_serializes_when_parallel_not_supported() {
        let registry = Arc::new(ResourceLockRegistry::new());
        let global = Arc::new(RwLock::new(()));
        let reads = HashSet::from([ScheduleResource::Path("a".to_string())]);
        let reads_b = HashSet::from([ScheduleResource::Path("b".to_string())]);

        let reg_a = registry.clone();
        let reg_b = registry.clone();
        let global_a = global.clone();
        let global_b = global.clone();

        let task_a = tokio::spawn(async move {
            let _guard = reg_a
                .acquire(&global_a, &reads, &HashSet::new(), false, false)
                .await;
            tokio::time::sleep(Duration::from_millis(80)).await;
        });
        let task_b = tokio::spawn(async move {
            let _guard = reg_b
                .acquire(&global_b, &reads_b, &HashSet::new(), false, false)
                .await;
            tokio::time::sleep(Duration::from_millis(80)).await;
        });

        let start = Instant::now();
        let (a, b) = tokio::join!(task_a, task_b);
        a.unwrap();
        b.unwrap();
        assert!(
            start.elapsed() >= Duration::from_millis(140),
            "global write lock should serialize, took {:?}",
            start.elapsed()
        );
    }
}
