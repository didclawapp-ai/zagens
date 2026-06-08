use std::time::Duration;

use anyhow::anyhow;

pub(crate) const DEFAULT_MAX_STEPS: u32 = 100;
pub(crate) const TOOL_TIMEOUT: Duration = Duration::from_secs(30);
/// Per-step LLM API call timeout. Each `create_message` request must complete
/// within this window or the step is treated as timed out. Prevents a single
/// stuck API call from blocking the sub-agent indefinitely.
pub(crate) const STEP_API_TIMEOUT: Duration = Duration::from_secs(600);

pub(crate) fn step_api_timeout_error(secs: u64) -> anyhow::Error {
    anyhow!(
        "API call timed out after {secs}s (per-step cap). Child stopped — not proof the area is \
         fully reviewed. Parent: re-spawn with a smaller scope and explicit step_timeout_ms \
         (audit-repo tier: 600000–1800000 by inventory file count), raise \
         [subagents] step_timeout_secs in config/settings, or continue with parallel \
         read_file; do not mark scratchpad inventory done on timeout alone."
    )
}
pub(crate) const RESULT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(crate) const DEFAULT_RESULT_TIMEOUT_MS: u64 = 30_000;
pub(crate) const MIN_WAIT_TIMEOUT_MS: u64 = 10_000;
pub(crate) const MAX_RESULT_TIMEOUT_MS: u64 = 3_600_000;
pub(crate) const COMPLETED_AGENT_RETENTION: Duration = Duration::from_secs(60 * 60);
/// Background scan interval for zombie `Running` agents (P2-10).
pub(crate) const ZOMBIE_SCAN_INTERVAL: Duration = Duration::from_secs(30);
/// Extra idle time beyond a step's API timeout before flagging `stuck_suspected`.
pub(crate) const STUCK_IDLE_BUFFER: Duration = Duration::from_secs(60);
/// Default idle window before maintenance auto-cancels a running sub-agent.
pub(crate) const DEFAULT_SUBAGENT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) fn compute_stuck_suspected(
    status: &zagens_core::subagent::SubAgentStatus,
    step_timeout: Duration,
    idle: Duration,
) -> bool {
    use zagens_core::subagent::SubAgentStatus;
    if status != &SubAgentStatus::Running {
        return false;
    }
    idle > step_timeout.saturating_add(STUCK_IDLE_BUFFER)
}
pub(crate) const SUBAGENT_STATE_SCHEMA_VERSION: u32 = 1;
pub(crate) const SUBAGENT_STATE_FILE: &str = "subagents.v1.json";
pub(crate) const SUBAGENT_RESTART_REASON: &str = "Interrupted by process restart";
pub(crate) const STEP_TOOL_BUDGET_RATIO: f32 = 0.8;

pub(crate) fn step_tool_budget(step_timeout: Duration) -> Duration {
    Duration::from_secs_f64(step_timeout.as_secs_f64() * f64::from(STEP_TOOL_BUDGET_RATIO))
}

pub(crate) fn adaptive_wait_timeout_ms(
    step_timeout_ms: u64,
    max_steps: u32,
    steps_taken: u32,
) -> u64 {
    let remaining = max_steps.saturating_sub(steps_taken);
    step_timeout_ms
        .saturating_mul(u64::from(remaining))
        .clamp(MIN_WAIT_TIMEOUT_MS, MAX_RESULT_TIMEOUT_MS)
}

pub(crate) const VALID_SUBAGENT_TYPES: &str = "general, explore, plan, review, implementer, verifier, custom, \
     worker, explorer, awaiter, default, implement, builder, verify, validator, tester";
/// Removal version for deprecated tool aliases.
pub(crate) const DEPRECATION_REMOVAL_VERSION: &str = "0.8.0";
