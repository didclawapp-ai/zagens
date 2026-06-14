//! CRAFT B-L1 helpers: fix-loop hints, runtime events, sentinel parsing.

use std::path::Path;

use serde_json::{Map, Value, json};
use tokio::sync::mpsc;
use zagens_core::events::Event;
use zagens_core::subagent::{SubAgentResult, SubAgentType, VerdictLevel};

use super::blackboard::implementer_round_count;

/// Maximum Implementer fix attempts per CRAFT `task_id` before escalation.
pub const MAX_CRAFT_FIX_LOOPS_PER_TASK: u32 = 3;

/// Blackboard partition key written for a CRAFT role.
pub fn blackboard_partition_key(agent_type: &SubAgentType) -> Option<&'static str> {
    match agent_type {
        SubAgentType::Explore => Some("explorer"),
        SubAgentType::Implementer => Some("implementer"),
        SubAgentType::Review => Some("reviewer"),
        SubAgentType::Verifier => Some("verifier"),
        SubAgentType::Auditor => Some("auditor"),
        _ => None,
    }
}

/// Whether the structured verdict should trigger a Review/Test fix-loop.
pub fn fix_loop_required(verdict: &VerdictLevel) -> bool {
    matches!(verdict, VerdictLevel::Blocker | VerdictLevel::Fail)
}

/// Role the parent should re-spawn after Implementer fixes issues.
pub fn fix_loop_retry_role(source: &SubAgentType) -> Option<&'static str> {
    match source {
        SubAgentType::Review => Some("review"),
        SubAgentType::Verifier => Some("verifier"),
        _ => None,
    }
}

/// Programmatic fix-loop hint injected into the parent transcript (B1.4).
///
/// When `workspace` is provided and Implementer rounds for `task_id` have reached
/// [`MAX_CRAFT_FIX_LOOPS_PER_TASK`], returns an exhaustion sentinel instead of
/// another spawn hint.
pub fn craft_fix_loop_hint(
    res: &SubAgentResult,
    task_id: Option<&str>,
    workspace: Option<&Path>,
) -> Option<String> {
    let task_id = task_id?;
    let verdict = res.structured_verdict.as_ref()?;
    if !fix_loop_required(&verdict.verdict) {
        return None;
    }
    let retry_role = fix_loop_retry_role(&res.agent_type)?;

    if let Some(ws) = workspace
        && implementer_round_count(ws, task_id) >= MAX_CRAFT_FIX_LOOPS_PER_TASK
    {
        return Some(craft_fix_loop_exhausted_sentinel(
            res, task_id, retry_role, verdict,
        ));
    }

    Some(craft_fix_loop_spawn_sentinel(
        res, task_id, retry_role, verdict,
    ))
}

fn craft_fix_loop_spawn_sentinel(
    res: &SubAgentResult,
    task_id: &str,
    retry_role: &str,
    verdict: &zagens_core::subagent::StructuredVerdict,
) -> String {
    let mut payload = Map::new();
    payload.insert("action".into(), json!("spawn_implementer"));
    payload.insert("task_id".into(), json!(task_id));
    payload.insert("source_agent_type".into(), json!(res.agent_type.as_str()));
    payload.insert("retry_role".into(), json!(retry_role));
    if let Ok(v) = serde_json::to_value(&verdict.verdict) {
        payload.insert("verdict".into(), v);
    }
    if !verdict.items.is_empty()
        && let Ok(items) = serde_json::to_value(&verdict.items)
    {
        payload.insert("items".into(), items);
    }
    if let Some(summary) = verdict.summary.as_deref().filter(|s| !s.is_empty()) {
        payload.insert("summary".into(), json!(summary));
    }

    let payload = Value::Object(payload);
    format!("<deepseek:craft.fix_loop>{payload}</deepseek:craft.fix_loop>")
}

fn craft_fix_loop_exhausted_sentinel(
    res: &SubAgentResult,
    task_id: &str,
    retry_role: &str,
    verdict: &zagens_core::subagent::StructuredVerdict,
) -> String {
    let mut payload = Map::new();
    payload.insert("action".into(), json!("escalate_user"));
    payload.insert("reason".into(), json!("max_fix_loops"));
    payload.insert("task_id".into(), json!(task_id));
    payload.insert("source_agent_type".into(), json!(res.agent_type.as_str()));
    payload.insert("retry_role".into(), json!(retry_role));
    payload.insert("max_rounds".into(), json!(MAX_CRAFT_FIX_LOOPS_PER_TASK));
    if let Ok(v) = serde_json::to_value(&verdict.verdict) {
        payload.insert("verdict".into(), v);
    }
    if !verdict.items.is_empty()
        && let Ok(items) = serde_json::to_value(&verdict.items)
    {
        payload.insert("items".into(), items);
    }
    if let Some(summary) = verdict.summary.as_deref().filter(|s| !s.is_empty()) {
        payload.insert("summary".into(), json!(summary));
    }

    let payload = Value::Object(payload);
    format!("<deepseek:craft.fix_loop_exhausted>{payload}</deepseek:craft.fix_loop_exhausted>")
}

/// Build the prompt for an auto-spawned Implementer fix-loop subagent.
///
/// Called from the executor post-hook (C1) when a Review or Verifier
/// finishes with BLOCKER/FAIL and `implementer_round_count` has not yet
/// reached [`MAX_CRAFT_FIX_LOOPS_PER_TASK`].
#[must_use]
pub fn build_fix_loop_implementer_prompt(
    verdict: &zagens_core::subagent::StructuredVerdict,
    source_role: &str,
    task_id: &str,
    fix_round: u32,
) -> String {
    let level = verdict_level_str(&verdict.verdict);
    let summary = verdict.summary.as_deref().unwrap_or("(no summary)");

    let mut items_text = String::new();
    for (i, item) in verdict.items.iter().enumerate() {
        let loc = if let Some(l) = item.line {
            format!("{}:{}", item.file, l)
        } else {
            item.file.clone()
        };
        let sev = item.severity.as_str();
        let desc = item.description.trim();
        let suggestion = item
            .suggestion
            .as_deref()
            .map(|s| format!(" Suggestion: {s}"))
            .unwrap_or_default();
        items_text.push_str(&format!(
            "{}. [{sev}] `{loc}` — {desc}{suggestion}\n",
            i + 1,
        ));
    }

    format!(
        "## CRAFT auto fix-loop — round {fix_round} (task `{task_id}`)\n\
         \n\
         The {source_role} identified **{level}** issues that block acceptance.\n\
         Summary: {summary}\n\
         \n\
         ### Issues to fix\n\
         {items_text}\n\
         Please fix all issues listed above. After fixing, run any relevant\n\
         `[verify: cmd]` commands mentioned in the issues to confirm they pass.\n\
         Do not modify unrelated code. After completing fixes, close your turn\n\
         so the next CRAFT review round can evaluate your changes."
    )
}

pub fn parse_subagent_done_sentinel(payload: &str) -> Option<Value> {
    let marker = "<deepseek:subagent.done>";
    let end = "</deepseek:subagent.done>";
    let start = payload.find(marker)? + marker.len();
    let rest = payload.get(start..)?;
    let end_idx = rest.find(end)?;
    serde_json::from_str(rest.get(..end_idx)?.trim()).ok()
}

/// Emit CRAFT runtime events for monitor/SSE (`craft.verdict`, `craft.board_updated`).
pub fn emit_craft_events(
    event_tx: &Option<mpsc::Sender<Event>>,
    agent_id: &str,
    res: &SubAgentResult,
    task_id: Option<&str>,
    partition: Option<&str>,
) {
    let Some(tx) = event_tx.as_ref() else {
        return;
    };

    if let (Some(tid), Some(part)) = (task_id, partition) {
        let _ = tx.try_send(Event::CraftBoardUpdated {
            task_id: tid.to_string(),
            partition: part.to_string(),
            agent_id: agent_id.to_string(),
        });
    }

    if let Some(v) = res.structured_verdict.as_ref() {
        let _ = tx.try_send(Event::CraftVerdict {
            agent_id: agent_id.to_string(),
            agent_type: res.agent_type.as_str().to_string(),
            task_id: task_id.map(str::to_string),
            verdict: verdict_level_str(&v.verdict).to_string(),
            summary: v.summary.clone(),
            items: serde_json::to_value(&v.items).unwrap_or_else(|_| json!([])),
        });
    }
}

pub fn verdict_level_str(level: &VerdictLevel) -> &'static str {
    match level {
        VerdictLevel::Pass => "PASS",
        VerdictLevel::Blocker => "BLOCKER",
        VerdictLevel::Major => "MAJOR",
        VerdictLevel::Fail => "FAIL",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::subagent::{
        StructuredVerdict, SubAgentAssignment, SubAgentStatus, VerdictItem,
    };

    fn sample_result(verdict: VerdictLevel, agent_type: SubAgentType) -> SubAgentResult {
        SubAgentResult {
            agent_id: "agent_1".into(),
            agent_type,
            assignment: SubAgentAssignment::new("task".into(), None),
            model: "deepseek-v4-flash".into(),
            nickname: None,
            status: SubAgentStatus::Completed,
            result: None,
            steps_taken: 1,
            duration_ms: 100,
            from_prior_session: false,
            structured_verdict: Some(StructuredVerdict {
                verdict,
                items: vec![VerdictItem {
                    severity: "BLOCKER".into(),
                    file: "src/lib.rs".into(),
                    line: Some(10),
                    description: "bad".into(),
                    rule: None,
                    suggestion: Some("fix it".into()),
                }],
                summary: Some("one issue".into()),
            }),
            structured_findings: None,
            completion_reason: None,
            max_steps: 100,
            step_timeout_ms: 600_000,
            structured_findings_parse_failure: None,
            scratchpad_run_id: None,
            parent_thread_id: None,
            progress_status: None,
            stuck_suspected: false,
            idle_ms: 0,
        }
    }

    #[test]
    fn fix_loop_hint_for_review_blocker() {
        let res = sample_result(VerdictLevel::Blocker, SubAgentType::Review);
        let hint = craft_fix_loop_hint(&res, Some("task-1"), None).expect("hint");
        assert!(hint.contains("<deepseek:craft.fix_loop>"));
        assert!(hint.contains("\"task_id\":\"task-1\""));
        assert!(hint.contains("\"retry_role\":\"review\""));
    }

    #[test]
    fn fix_loop_hint_skips_pass() {
        let res = sample_result(VerdictLevel::Pass, SubAgentType::Review);
        assert!(craft_fix_loop_hint(&res, Some("task-1"), None).is_none());
    }

    #[test]
    fn fix_loop_hint_requires_task_id() {
        let res = sample_result(VerdictLevel::Fail, SubAgentType::Verifier);
        assert!(craft_fix_loop_hint(&res, None, None).is_none());
    }

    #[test]
    fn fix_loop_hint_exhausted_after_max_rounds() {
        let dir = tempfile::tempdir().expect("tempdir");
        let ws = dir.path();
        let task_id = "exhaust-task";
        let board_path = ws.join(".zagens/blackboards/exhaust-task.json");
        std::fs::create_dir_all(board_path.parent().expect("parent")).expect("mkdir");
        let board = json!({
            "schema_version": 1,
            "task_id": task_id,
            "implementer": {
                "rounds": [
                    { "round": 1, "changes": [] },
                    { "round": 2, "changes": [] },
                    { "round": 3, "changes": [] }
                ]
            }
        });
        std::fs::write(
            &board_path,
            serde_json::to_string_pretty(&board).expect("json"),
        )
        .expect("write");

        let res = sample_result(VerdictLevel::Blocker, SubAgentType::Review);
        let hint = craft_fix_loop_hint(&res, Some(task_id), Some(ws)).expect("exhausted sentinel");
        assert!(hint.contains("<deepseek:craft.fix_loop_exhausted>"));
        assert!(hint.contains("\"action\":\"escalate_user\""));
        assert!(hint.contains("\"max_rounds\":3"));
    }
}
