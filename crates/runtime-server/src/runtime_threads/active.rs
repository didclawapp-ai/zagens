//! Active thread engine LRU and in-memory turn state (A4.6 extract).

use std::collections::{HashMap, HashSet, VecDeque};


use crate::core::engine::EngineHandle;


#[derive(Debug, Clone)]
pub(crate) struct ActiveTurnState {
    pub(crate) turn_id: String,
    pub(crate) interrupt_requested: bool,
    pub(crate) auto_approve: bool,
    pub(crate) trust_mode: bool,
}

#[derive(Clone)]
pub(crate) struct ActiveThreadState {
    pub(crate) engine: EngineHandle,
    pub(crate) active_turn: Option<ActiveTurnState>,
}

#[derive(Default)]
pub(crate) struct ActiveThreads {
    pub(crate) engines: HashMap<String, ActiveThreadState>,
    pub(crate) lru: VecDeque<String>,
    pub(crate) pending_approvals: HashMap<String, PendingApproval>,
}

#[allow(dead_code)]
pub(crate) struct PendingApproval {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) tool_call_id: String,
    pub(crate) deadline: tokio::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeApprovalDecision {
    ApproveTool,
    DenyTool,
    RetryWithFullAccess,
}

pub(crate) fn touch_lru(lru: &mut VecDeque<String>, thread_id: &str) {
    if let Some(idx) = lru.iter().position(|id| id == thread_id) {
        lru.remove(idx);
    }
    lru.push_back(thread_id.to_string());
}

pub(crate) fn enforce_lru_capacity(
    active: &mut ActiveThreads,
    max_active_threads: usize,
) -> Vec<EngineHandle> {
    let mut evicted = Vec::new();
    if max_active_threads == 0 || active.engines.len() < max_active_threads {
        return evicted;
    }
    let protected = active
        .engines
        .iter()
        .filter_map(|(thread_id, state)| {
            if state.active_turn.is_some() {
                Some(thread_id.clone())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();

    let scan_limit = active.lru.len();
    for _ in 0..scan_limit {
        let Some(candidate) = active.lru.pop_front() else {
            break;
        };
        if protected.contains(&candidate) {
            active.lru.push_back(candidate);
            continue;
        }
        if let Some(state) = active.engines.remove(&candidate) {
            evicted.push(state.engine);
        }
        break;
    }
    evicted
}
