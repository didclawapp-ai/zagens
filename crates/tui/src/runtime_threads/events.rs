//! Event timeline helpers for agent card rebind (R-003 A4.6).

use super::types::RuntimeEventRecord;

/// One sub-agent rebind hint extracted from a thread's persisted event
/// timeline (issue #128). When the TUI resumes a session that was
/// mid-fanout, the in-transcript card stack is empty — these hints let the
/// UI know which agent_ids were live (or recently terminal) so it can
/// reconstruct the matching `DelegateCard` / `FanoutCard` placeholders
/// before fresh mailbox envelopes arrive on a re-attached engine.
///
/// The helper is the testable contract here — actual TUI wire-up to the
/// resume flow is a follow-up; the runtime API consumer (`runtime_api.rs`)
/// can already call `resume_thread_with_agent_rebind` to drive it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // consumed by #128 follow-up TUI resume wiring; tested here.
pub struct AgentRebindHint {
    pub agent_id: String,
    pub status: AgentRebindStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AgentRebindStatus {
    Spawned,
    InProgress,
    Completed,
}

/// Collapse a chronologically ordered slice of `RuntimeEventRecord` into
/// the latest known status per `agent_id`. Drops entries that aren't in
/// the `agent.*` family. Cards built from these hints are immediately
/// open to mutation by subsequent live mailbox envelopes (each envelope's
/// `agent_id` matches one already in the rebind map).
#[must_use]
#[allow(dead_code)]
pub fn collect_agent_rebind_hints(events: &[RuntimeEventRecord]) -> Vec<AgentRebindHint> {
    use std::collections::BTreeMap;
    let mut latest: BTreeMap<String, AgentRebindStatus> = BTreeMap::new();
    for event in events {
        let id = match event.payload.get("agent_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let next_status = match event.event.as_str() {
            "agent.spawned" => Some(AgentRebindStatus::Spawned),
            "agent.progress" => Some(AgentRebindStatus::InProgress),
            "agent.completed" => Some(AgentRebindStatus::Completed),
            _ => None,
        };
        if let Some(status) = next_status {
            // Don't downgrade Completed → InProgress on out-of-order events.
            let entry = latest.entry(id).or_insert(status);
            if !matches!(*entry, AgentRebindStatus::Completed) {
                *entry = status;
            }
        }
    }
    latest
        .into_iter()
        .map(|(agent_id, status)| AgentRebindHint { agent_id, status })
        .collect()
}

