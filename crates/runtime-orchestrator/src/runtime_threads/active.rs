//! Active thread engine LRU and in-memory turn state (A4.6 extract).

use std::collections::{HashMap, HashSet, VecDeque};

use crate::engine::EngineHandle;

#[derive(Debug, Clone)]
pub struct ActiveTurnState {
    pub turn_id: String,
    pub interrupt_requested: bool,
    pub auto_approve: bool,
    pub trust_mode: bool,
}

#[derive(Clone)]
pub struct ActiveThreadState<P, R> {
    pub engine: EngineHandle<P, R>,
    pub active_turn: Option<ActiveTurnState>,
    /// Set when overlay changes while a turn is active; engine unloads after turn ends.
    pub pending_config_refresh: bool,
}

pub struct ActiveThreads<P, R> {
    pub engines: HashMap<String, ActiveThreadState<P, R>>,
    pub lru: VecDeque<String>,
    pub pending_approvals: HashMap<String, PendingApproval>,
}

impl<P, R> Default for ActiveThreads<P, R> {
    fn default() -> Self {
        Self {
            engines: HashMap::new(),
            lru: VecDeque::new(),
            pending_approvals: HashMap::new(),
        }
    }
}

#[allow(dead_code)]
pub struct PendingApproval {
    pub thread_id: String,
    pub turn_id: String,
    pub tool_call_id: String,
    pub approval_key: String,
    pub deadline: tokio::time::Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeApprovalDecision {
    ApproveTool,
    DenyTool,
    RetryWithFullAccess,
}

/// Browser tools that must still show an approval card even when the thread has
/// composer/`auto_approve` on. Policy already sets `approval_required`; without
/// this gate the monitor silently `approve_tool_call` and the UI never sees
/// `approval.required`.
///
/// When `browser_yolo` is on, policy does not emit `ApprovalRequired` for these
/// tools — so this helper is only reached for intentional human-ask cases.
#[must_use]
pub fn browser_tool_blocks_thread_auto_approve(tool_name: &str, description: &str) -> bool {
    matches!(
        tool_name,
        "browser_click" | "browser_type" | "browser_scroll" | "browser_start_preview"
    ) || (tool_name == "browser_navigate"
        && (description.contains("external site") || description.contains("session allowlist")))
}

pub fn touch_lru(lru: &mut VecDeque<String>, thread_id: &str) {
    if let Some(idx) = lru.iter().position(|id| id == thread_id) {
        lru.remove(idx);
    }
    lru.push_back(thread_id.to_string());
}

pub fn enforce_lru_capacity<P, R>(
    active: &mut ActiveThreads<P, R>,
    max_active_threads: usize,
) -> Vec<EngineHandle<P, R>>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_navigate_blocks_auto_approve() {
        assert!(browser_tool_blocks_thread_auto_approve(
            "browser_navigate",
            "Browser: open external site www.deepseek.com (session allowlist)"
        ));
        assert!(!browser_tool_blocks_thread_auto_approve(
            "browser_navigate",
            "Browser: navigate to http://127.0.0.1:8080/"
        ));
        assert!(browser_tool_blocks_thread_auto_approve(
            "browser_click",
            "Browser: click button \"go\" (#0, ref=button:go:0)"
        ));
        assert!(!browser_tool_blocks_thread_auto_approve(
            "read_file",
            "Read a file"
        ));
    }
}
