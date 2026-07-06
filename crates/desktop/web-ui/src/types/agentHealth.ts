/** Agent health report from GET /v1/agent-health (shared with `zagens doctor --tools`). */

export interface ToolStat {
  name: string;
  calls: number;
  failures: number;
  blocked: number;
  timeouts: number;
  failure_rate?: number | null;
}

export interface ToolHintAuditEntry {
  name: string;
  failures: number;
  failure_rate?: number | null;
  hint_covered: boolean;
  hint_summary?: string | null;
}

export interface AgentHealthReport {
  sessions_db: string;
  present: boolean;
  kernel_event_rows: number;
  tool_calls: number;
  tool_failures: number;
  tool_failure_rate?: number | null;
  loop_guard_events: number;
  loop_guard_retry_rate?: number | null;
  harness_verify_events: number;
  harness_verify_passes: number;
  harness_verify_self_heal_rate?: number | null;
  stage_gate_blocked_events: number;
  turns_with_tools: number;
  top_by_calls: ToolStat[];
  top_by_failure_rate: ToolStat[];
  hint_coverage_top_failures: ToolHintAuditEntry[];
  hint_coverage_rate?: number | null;
  note?: string | null;
}
