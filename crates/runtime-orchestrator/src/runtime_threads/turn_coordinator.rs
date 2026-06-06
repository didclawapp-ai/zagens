//! Per-thread drain serialisation (OpenCode `SessionRunCoordinator` analogue).
//!
//! Guarantees at most one active drain chain per thread while coalescing `run` /
//! `wake` demands. `interrupt` establishes an ownership boundary: advisory wakes
//! recorded before the boundary are suppressed.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DemandTag {
    Run,
    Wake,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinatorAction {
    /// Begin or join an explicit drain generation.
    StartDrain,
    /// A drain is already active; join the in-flight generation.
    JoinDrain,
    /// Schedule one coalesced follow-up after the current drain completes.
    ScheduleFollowUp,
    /// Wake suppressed by an interrupt boundary.
    Suppressed,
    /// No eligible work (idle).
    NoOp,
}

#[derive(Debug, Clone)]
struct Entry {
    draining: bool,
    current: Option<DemandTag>,
    pending: Option<DemandTag>,
    interrupt_seq: u64,
    wake_seq: u64,
}

impl Default for Entry {
    fn default() -> Self {
        Self {
            draining: false,
            current: None,
            pending: None,
            interrupt_seq: 0,
            wake_seq: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct TurnCoordinator {
    entries: HashMap<String, Entry>,
}

fn coalesce(left: Option<DemandTag>, right: DemandTag) -> DemandTag {
    if left == Some(DemandTag::Run) || right == DemandTag::Run {
        DemandTag::Run
    } else {
        DemandTag::Wake
    }
}

impl TurnCoordinator {
    pub fn is_draining(&self, thread_id: &str) -> bool {
        self.entries.get(thread_id).is_some_and(|e| e.draining)
    }

    /// Mark the start of a drain generation (explicit `run` or promoted work).
    pub fn begin_drain(&mut self, thread_id: &str, explicit: bool) {
        let entry = self.entries.entry(thread_id.to_string()).or_default();
        entry.draining = true;
        entry.current = Some(if explicit {
            DemandTag::Run
        } else {
            DemandTag::Wake
        });
    }

    pub fn request_run(&mut self, thread_id: &str) -> CoordinatorAction {
        let entry = self.entries.entry(thread_id.to_string()).or_default();
        if entry.draining {
            entry.pending = Some(coalesce(entry.pending, DemandTag::Run));
            CoordinatorAction::JoinDrain
        } else {
            entry.draining = true;
            entry.current = Some(DemandTag::Run);
            CoordinatorAction::StartDrain
        }
    }

    pub fn request_wake(&mut self, thread_id: &str, inbox_seq: u64) -> CoordinatorAction {
        let entry = self.entries.entry(thread_id.to_string()).or_default();
        if inbox_seq <= entry.interrupt_seq {
            return CoordinatorAction::Suppressed;
        }
        entry.wake_seq = entry.wake_seq.max(inbox_seq);
        if entry.draining {
            entry.pending = Some(coalesce(entry.pending, DemandTag::Wake));
            CoordinatorAction::ScheduleFollowUp
        } else {
            entry.draining = true;
            entry.current = Some(DemandTag::Wake);
            CoordinatorAction::StartDrain
        }
    }

    /// End the current drain. Returns whether a coalesced follow-up should run.
    pub fn finish_drain(&mut self, thread_id: &str) -> Option<DemandTag> {
        let Some(entry) = self.entries.get_mut(thread_id) else {
            return None;
        };
        if !entry.draining {
            return None;
        }
        entry.draining = false;
        entry.current = None;
        let follow_up = entry.pending.take();
        if follow_up.is_none() {
            self.entries.remove(thread_id);
        } else {
            entry.draining = true;
            entry.current = follow_up;
        }
        follow_up
    }

    pub fn interrupt(&mut self, thread_id: &str, seq: u64) {
        let entry = self.entries.entry(thread_id.to_string()).or_default();
        entry.interrupt_seq = entry.interrupt_seq.max(seq);
        if let Some(DemandTag::Wake) = entry.pending {
            if entry.wake_seq <= entry.interrupt_seq {
                entry.pending = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_while_idle_starts_drain() {
        let mut c = TurnCoordinator::default();
        assert_eq!(c.request_run("t1"), CoordinatorAction::StartDrain);
        assert!(c.is_draining("t1"));
    }

    #[test]
    fn second_run_joins_and_schedules_follow_up() {
        let mut c = TurnCoordinator::default();
        assert_eq!(c.request_run("t1"), CoordinatorAction::StartDrain);
        assert_eq!(c.request_run("t1"), CoordinatorAction::JoinDrain);
        assert_eq!(c.finish_drain("t1"), Some(DemandTag::Run));
    }

    #[test]
    fn wake_coalesces_while_draining() {
        let mut c = TurnCoordinator::default();
        c.begin_drain("t1", true);
        assert_eq!(
            c.request_wake("t1", 10),
            CoordinatorAction::ScheduleFollowUp
        );
        assert_eq!(
            c.request_wake("t1", 11),
            CoordinatorAction::ScheduleFollowUp
        );
        assert_eq!(c.finish_drain("t1"), Some(DemandTag::Wake));
    }

    #[test]
    fn interrupt_suppresses_pending_wake() {
        let mut c = TurnCoordinator::default();
        c.begin_drain("t1", true);
        c.request_wake("t1", 5);
        c.interrupt("t1", 5);
        assert_eq!(c.request_wake("t1", 4), CoordinatorAction::Suppressed);
        assert_eq!(c.finish_drain("t1"), None);
    }

    #[test]
    fn wake_after_interrupt_boundary_starts_new_drain() {
        let mut c = TurnCoordinator::default();
        c.interrupt("t1", 3);
        assert_eq!(c.request_wake("t1", 4), CoordinatorAction::StartDrain);
    }
}
