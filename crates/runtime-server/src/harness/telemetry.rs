//! Offline aggregation of tool telemetry from `kernel_events` (Phase 0.3 / T1 / Phase 3.1).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use rusqlite::Connection;
use serde::Serialize;
use zagens_core::engine::kernel_event::{KernelEvent, ToolOutcome};
use zagens_runtime_adapters::persist::kernel_event_log::{
    KernelEventLog, ensure_kernel_events_table,
};
use zagens_runtime_adapters::persist::session_manager::default_sessions_dir;

use super::hints;
use super::tool_sequences::{ToolSequenceReport, mine_tool_sequences};

const TELEMETRY_KINDS: &[&str] = &[
    "tool_call_finished",
    "loop_guard_triggered",
    "harness_verify",
    "stage_gate_blocked",
];

/// Per-tool counters derived from `tool_call_finished` events.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolStat {
    pub name: String,
    pub calls: u64,
    pub failures: u64,
    pub blocked: u64,
    pub timeouts: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_rate: Option<f64>,
}

/// T3 hint audit row for a high-failure tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolHintAuditEntry {
    pub name: String,
    pub failures: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_rate: Option<f64>,
    pub hint_covered: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint_summary: Option<String>,
}

/// Aggregate report for T1 (`zagens doctor --tools` / desktop Agent 体检).
#[derive(Debug, Clone, Serialize)]
pub struct ToolTelemetryReport {
    pub sessions_db: String,
    pub present: bool,
    pub kernel_event_rows: u64,
    pub tool_calls: u64,
    pub tool_failures: u64,
    pub tool_failure_rate: Option<f64>,
    pub loop_guard_events: u64,
    pub loop_guard_retry_rate: Option<f64>,
    pub harness_verify_events: u64,
    pub harness_verify_passes: u64,
    pub harness_verify_self_heal_rate: Option<f64>,
    pub stage_gate_blocked_events: u64,
    pub turns_with_tools: u64,
    pub top_by_calls: Vec<ToolStat>,
    pub top_by_failure_rate: Vec<ToolStat>,
    pub hint_coverage_top_failures: Vec<ToolHintAuditEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint_coverage_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_sequences: Option<ToolSequenceReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ToolTelemetryReport {
    #[must_use]
    pub fn empty(db_path: &Path, note: impl Into<String>) -> Self {
        Self {
            sessions_db: db_path.display().to_string(),
            present: db_path.exists(),
            kernel_event_rows: 0,
            tool_calls: 0,
            tool_failures: 0,
            tool_failure_rate: None,
            loop_guard_events: 0,
            loop_guard_retry_rate: None,
            harness_verify_events: 0,
            harness_verify_passes: 0,
            harness_verify_self_heal_rate: None,
            stage_gate_blocked_events: 0,
            turns_with_tools: 0,
            top_by_calls: Vec::new(),
            top_by_failure_rate: Vec::new(),
            hint_coverage_top_failures: Vec::new(),
            hint_coverage_rate: None,
            tool_sequences: None,
            note: Some(note.into()),
        }
    }
}

/// Resolve default `~/.zagens/sessions/sessions.db`.
#[must_use]
pub fn default_sessions_db_path() -> PathBuf {
    default_sessions_dir()
        .map(|d| d.join("sessions.db"))
        .unwrap_or_else(|_| PathBuf::from(".zagens/sessions/sessions.db"))
}

/// HL-2: append `HarnessVerify` records to the shared sessions kernel_events log.
///
/// Best-effort — silently no-ops when the sessions DB cannot be opened (CI / headless).
pub fn append_harness_verify_records(
    turn_id: &str,
    records: &[crate::long_horizon::harness_verify_loop::HarnessVerifyRecord],
) {
    if records.is_empty() {
        return;
    }
    let db_path = default_sessions_db_path();
    if let Some(parent) = db_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(conn) = Connection::open(&db_path) else {
        return;
    };
    if ensure_kernel_events_table(&conn).is_err() {
        return;
    }
    let mut log = KernelEventLog::new(&conn);
    for record in records {
        let event = crate::long_horizon::harness_verify_loop::record_to_kernel_event(
            turn_id.to_string(),
            record,
        );
        let _ = log.append(event);
    }
}

/// Build a telemetry report from on-disk `sessions.db` (read-only).
pub fn build_tool_telemetry_report(db_path: &Path) -> anyhow::Result<ToolTelemetryReport> {
    if !db_path.exists() {
        return Ok(ToolTelemetryReport::empty(
            db_path,
            "sessions.db not found — run agent sessions locally to populate kernel_events",
        ));
    }

    let conn = Connection::open(db_path)
        .with_context(|| format!("open sessions db {}", db_path.display()))?;
    ensure_kernel_events_table(&conn).context("ensure kernel_events table")?;

    let kernel_event_rows: u64 = conn
        .query_row("SELECT COUNT(*) FROM kernel_events", [], |row| row.get(0))
        .unwrap_or(0);

    let log = KernelEventLog::new(&conn);
    let envelopes = log
        .load_events_by_kinds(TELEMETRY_KINDS)
        .context("load telemetry kinds")?;

    let mut stats: HashMap<String, ToolStat> = HashMap::new();
    let mut loop_guard_events = 0u64;
    let mut harness_verify_events = 0u64;
    let mut harness_verify_passes = 0u64;
    let mut harness_verify_retries = 0u64;
    let mut harness_verify_retries_passed = 0u64;
    let mut stage_gate_blocked_events = 0u64;
    let mut turns_with_tools = HashMap::<String, ()>::new();

    for envelope in &envelopes {
        match &envelope.event {
            KernelEvent::ToolCallFinished {
                turn_id,
                tool_name,
                outcome,
                ..
            } => {
                turns_with_tools.insert(turn_id.clone(), ());
                let tool_name = if tool_name.trim().is_empty() {
                    "<unknown>".to_string()
                } else {
                    tool_name.clone()
                };
                let entry = stats.entry(tool_name.clone()).or_insert_with(|| ToolStat {
                    name: tool_name,
                    ..ToolStat::default()
                });
                entry.calls = entry.calls.saturating_add(1);
                match outcome {
                    ToolOutcome::Success => {}
                    ToolOutcome::Blocked { .. } => {
                        entry.blocked = entry.blocked.saturating_add(1);
                        entry.failures = entry.failures.saturating_add(1);
                    }
                    ToolOutcome::Timeout => {
                        entry.timeouts = entry.timeouts.saturating_add(1);
                        entry.failures = entry.failures.saturating_add(1);
                    }
                    ToolOutcome::ToolError { .. } | ToolOutcome::GuardHalt { .. } => {
                        entry.failures = entry.failures.saturating_add(1);
                    }
                    _ => {
                        entry.failures = entry.failures.saturating_add(1);
                    }
                }
            }
            KernelEvent::LoopGuardTriggered { .. } => {
                loop_guard_events = loop_guard_events.saturating_add(1);
            }
            KernelEvent::HarnessVerify { pass, retry_no, .. } => {
                harness_verify_events = harness_verify_events.saturating_add(1);
                if *pass {
                    harness_verify_passes = harness_verify_passes.saturating_add(1);
                }
                if *retry_no > 0 {
                    harness_verify_retries = harness_verify_retries.saturating_add(1);
                    if *pass {
                        harness_verify_retries_passed =
                            harness_verify_retries_passed.saturating_add(1);
                    }
                }
            }
            KernelEvent::StageGateBlocked { .. } => {
                stage_gate_blocked_events = stage_gate_blocked_events.saturating_add(1);
            }
            _ => {}
        }
    }

    let tool_calls: u64 = stats.values().map(|s| s.calls).sum();
    let tool_failures: u64 = stats.values().map(|s| s.failures).sum();

    let mut all_tools: Vec<ToolStat> = stats.into_values().collect();
    for tool in &mut all_tools {
        if tool.calls > 0 {
            tool.failure_rate =
                Some((tool.failures as f64 / tool.calls as f64 * 100.0 * 100.0).round() / 100.0);
        }
    }

    let mut top_by_calls = all_tools.clone();
    top_by_calls.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.name.cmp(&b.name)));
    top_by_calls.truncate(15);

    let mut top_by_failure_rate: Vec<ToolStat> = all_tools
        .iter()
        .filter(|t| t.calls >= 3 && t.failures > 0)
        .cloned()
        .collect();
    top_by_failure_rate.sort_by(|a, b| {
        b.failure_rate
            .partial_cmp(&a.failure_rate)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.calls.cmp(&a.calls))
    });
    top_by_failure_rate.truncate(10);

    let (hint_coverage_top_failures, hint_coverage_rate) =
        build_hint_coverage(&top_by_failure_rate);

    let tool_call_envelopes: Vec<_> = envelopes
        .iter()
        .filter(|e| matches!(e.event, KernelEvent::ToolCallFinished { .. }))
        .cloned()
        .collect();
    let tool_sequences = if tool_call_envelopes.is_empty() {
        None
    } else {
        Some(mine_tool_sequences(&tool_call_envelopes))
    };

    let tool_failure_rate = if tool_calls > 0 {
        Some((tool_failures as f64 / tool_calls as f64 * 100.0 * 100.0).round() / 100.0)
    } else {
        None
    };

    let loop_guard_retry_rate = if tool_calls > 0 {
        Some((loop_guard_events as f64 / tool_calls as f64 * 100.0 * 100.0).round() / 100.0)
    } else {
        None
    };

    let harness_verify_self_heal_rate = if harness_verify_retries > 0 {
        Some(
            (harness_verify_retries_passed as f64 / harness_verify_retries as f64 * 100.0 * 100.0)
                .round()
                / 100.0,
        )
    } else {
        None
    };

    Ok(ToolTelemetryReport {
        sessions_db: db_path.display().to_string(),
        present: true,
        kernel_event_rows,
        tool_calls,
        tool_failures,
        tool_failure_rate,
        loop_guard_events,
        loop_guard_retry_rate,
        harness_verify_events,
        harness_verify_passes,
        harness_verify_self_heal_rate,
        stage_gate_blocked_events,
        turns_with_tools: turns_with_tools.len() as u64,
        top_by_calls,
        top_by_failure_rate,
        hint_coverage_top_failures,
        hint_coverage_rate,
        tool_sequences,
        note: if tool_calls == 0 {
            Some("No tool_call_finished events yet".into())
        } else {
            None
        },
    })
}

fn build_hint_coverage(top_by_failure_rate: &[ToolStat]) -> (Vec<ToolHintAuditEntry>, Option<f64>) {
    if top_by_failure_rate.is_empty() {
        return (Vec::new(), None);
    }

    let mut entries = Vec::with_capacity(top_by_failure_rate.len());
    let mut covered = 0u64;
    for tool in top_by_failure_rate {
        let audit = hints::audit_tool(&tool.name);
        if audit.covered {
            covered = covered.saturating_add(1);
        }
        entries.push(ToolHintAuditEntry {
            name: tool.name.clone(),
            failures: tool.failures,
            failure_rate: tool.failure_rate,
            hint_covered: audit.covered,
            hint_summary: audit.summary,
        });
    }
    let rate = Some((covered as f64 / entries.len() as f64 * 100.0 * 100.0).round() / 100.0);
    (entries, rate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use zagens_core::engine::kernel_event::{KernelEvent, TurnOutcome};
    use zagens_core::turn::TurnLoopMode;

    fn write_fixture_db(dir: &Path) -> PathBuf {
        let db_path = dir.join("sessions.db");
        let conn = Connection::open(&db_path).expect("open");
        ensure_kernel_events_table(&conn).expect("migrate");
        let mut log = KernelEventLog::new(&conn);
        let tid = "turn-fixture".to_string();

        log.append(KernelEvent::TurnStarted {
            turn_id: tid.clone(),
            mode: TurnLoopMode::Agent,
            input_text: "x".into(),
            max_steps: 5,
        })
        .expect("turn_started");

        for (name, outcome) in [
            ("read_file", ToolOutcome::Success),
            (
                "read_file",
                ToolOutcome::ToolError {
                    message: "not found".into(),
                },
            ),
            (
                "grep",
                ToolOutcome::Blocked {
                    reason: "approval".into(),
                },
            ),
            ("grep", ToolOutcome::Success),
            ("grep", ToolOutcome::Success),
        ] {
            log.append(KernelEvent::ToolCallFinished {
                turn_id: tid.clone(),
                call_id: format!("call-{name}-{}", outcome.kind_label()),
                tool_name: name.to_string(),
                outcome,
                duration_ms: 1,
                wrote_state: false,
                result_preview: String::new(),
                session_content: String::new(),
            })
            .expect("tool finished");
        }

        log.append(KernelEvent::LoopGuardTriggered {
            turn_id: tid.clone(),
            call_id: "call-dup".into(),
            reason: "identical_call".into(),
        })
        .expect("loop guard");

        log.append(KernelEvent::HarnessVerify {
            turn_id: tid.clone(),
            stage: "build".into(),
            predicate: "exit_code".into(),
            pass: true,
            retry_no: 1,
            rollback_triggered: false,
            duration_ms: 8,
            code: None,
            suggestion: None,
        })
        .expect("harness verify self-heal pass");

        log.append(KernelEvent::StageGateBlocked {
            turn_id: tid.clone(),
            step_idx: 2,
            skill: "office-weekly-report".into(),
            stage: "prepare".into(),
            tool_name: "write_office".into(),
            code: "stage_gate_blocked".into(),
            suggestion: "Complete prepare first.".into(),
        })
        .expect("stage gate blocked");

        log.append(KernelEvent::TurnEnded {
            turn_id: tid,
            outcome: TurnOutcome::Completed,
            total_steps: 3,
        })
        .expect("turn ended");

        db_path
    }

    trait OutcomeLabel {
        fn kind_label(&self) -> &'static str;
    }

    impl OutcomeLabel for ToolOutcome {
        fn kind_label(&self) -> &'static str {
            match self {
                ToolOutcome::Success => "ok",
                ToolOutcome::Blocked { .. } => "blocked",
                ToolOutcome::GuardHalt { .. } => "halt",
                ToolOutcome::Timeout => "timeout",
                ToolOutcome::ToolError { .. } => "err",
                _ => "other",
            }
        }
    }

    #[test]
    fn aggregates_failure_and_loop_guard_rates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = write_fixture_db(dir.path());
        let report = build_tool_telemetry_report(&db_path).expect("report");

        assert_eq!(report.tool_calls, 5);
        assert_eq!(report.tool_failures, 2);
        assert_eq!(report.loop_guard_events, 1);
        assert!(report.tool_failure_rate.unwrap() > 39.0);
        assert_eq!(report.top_by_failure_rate.len(), 1);
        assert_eq!(report.harness_verify_events, 1);
        assert_eq!(report.harness_verify_passes, 1);
        assert_eq!(report.harness_verify_self_heal_rate, Some(100.0));
        assert_eq!(report.stage_gate_blocked_events, 1);
        assert_eq!(report.top_by_failure_rate[0].name, "grep");
        assert_eq!(report.hint_coverage_top_failures.len(), 1);
        assert!(report.hint_coverage_top_failures[0].hint_covered);
        assert_eq!(report.hint_coverage_rate, Some(100.0));
    }
}
