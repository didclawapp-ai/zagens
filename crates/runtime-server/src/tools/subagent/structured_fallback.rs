use std::path::Path;

use zagens_core::subagent::SubAgentResult;

use super::blackboard::{
    read_structured_findings_from_blackboard, read_structured_verdict_from_blackboard,
};
use super::prompts::{
    findings_to_verdict, parse_structured_findings_result, parse_structured_verdict,
};
use super::runtime::SubAgent;

/// Enrich a snapshot with structured output using memory → blackboard → prose fallback.
pub(crate) fn enrich_subagent_result(
    snap: &mut SubAgentResult,
    agent: &SubAgent,
    workspace: &Path,
) {
    if snap.structured_findings.is_none()
        && let Some(ref task_id) = agent.blackboard_task_id
    {
        snap.structured_findings =
            read_structured_findings_from_blackboard(workspace, task_id, &snap.agent_type);
    }
    if snap.structured_verdict.is_none()
        && let Some(ref task_id) = agent.blackboard_task_id
    {
        snap.structured_verdict =
            read_structured_verdict_from_blackboard(workspace, task_id, &snap.agent_type);
    }
    if snap.structured_findings.is_none()
        && let Some(text) = snap.result.as_deref()
    {
        match parse_structured_findings_result(text) {
            Ok(findings) => {
                snap.structured_findings = Some(findings);
                snap.structured_findings_parse_failure = None;
            }
            Err(reason) => {
                snap.structured_findings_parse_failure = Some(reason);
            }
        }
    }
    if snap.structured_verdict.is_none() {
        if let Some(text) = snap.result.as_deref() {
            snap.structured_verdict = parse_structured_verdict(text);
        }
        if snap.structured_verdict.is_none() {
            snap.structured_verdict = snap.structured_findings.as_ref().map(findings_to_verdict);
        }
    }
}
