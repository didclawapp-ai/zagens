/**
 * Intervals for runtime HTTP polls (channel B).
 * While streaming, panel state prefers channel C (`panel.*` on the live SSE stream).
 */

export const RUNTIME_PROBE_INTERVAL_STREAMING_MS = 18_000;
export const RUNTIME_PROBE_INTERVAL_IDLE_MS = 8_000;

/** Idle / non-streaming context refresh only (streaming uses `panel.context`). */
export const THREAD_CONTEXT_POLL_STREAMING_MS = 6_000;

/** Slow B-channel fallback if a `panel.scratchpad` event was missed. */
export const SCRATCHPAD_STATUS_POLL_STREAMING_MS = 60_000;
export const SCRATCHPAD_STATUS_POLL_IDLE_MS = 12_000;

/** Slow B-channel fallback if a `panel.checklist` event was missed. */
export const CHECKLIST_POLL_STREAMING_MS = 60_000;
export const CHECKLIST_POLL_IDLE_MS = 5_000;

/** LHT task graph (`harness.task_graph` SSE + GET fallback). */
export const TASK_GRAPH_POLL_STREAMING_MS = 8_000;
export const TASK_GRAPH_POLL_IDLE_MS = 8_000;

/** CRAFT blackboard task list in AgentPanel. */
export const CRAFT_BLACKBOARD_POLL_MS = 5_000;

/** Sub-agent disk snapshot while a turn is streaming (SSE fallback). */
export const SUBAGENT_STATE_POLL_STREAMING_MS = 3_000;

/** Best-effort session JSON checkpoint during long streams (tab hide still persists immediately). */
export const SESSION_CHECKPOINT_STREAMING_MS = 60_000;
