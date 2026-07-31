//! Offline aggregation CLI output for `zagens doctor --tools` (Phase 0.3 / T1).

pub use crate::harness::telemetry::{
    ToolHintAuditEntry, ToolStat, ToolTelemetryReport, build_tool_telemetry_report,
    default_sessions_db_path,
};

pub fn print_tool_telemetry_human(report: &ToolTelemetryReport) {
    use colored::Colorize;

    println!("{}", "Tool telemetry (kernel_events)".bold());
    println!("  db: {}", report.sessions_db);
    if !report.present {
        println!("  {}", "sessions.db missing".yellow());
        if let Some(note) = &report.note {
            println!("  {note}");
        }
        return;
    }

    println!("  kernel_event rows: {}", report.kernel_event_rows);
    println!("  turns with tools: {}", report.turns_with_tools);
    println!("  tool calls: {}", report.tool_calls);
    if let Some(rate) = report.tool_failure_rate {
        println!("  tool failure rate: {rate}%");
    }
    println!("  loop_guard events: {}", report.loop_guard_events);
    if let Some(rate) = report.loop_guard_retry_rate {
        println!("  loop_guard / tool call rate: {rate}%");
    }
    println!("  harness_verify events: {}", report.harness_verify_events);
    if report.harness_verify_events > 0 {
        let pass_rate = (report.harness_verify_passes as f64 / report.harness_verify_events as f64
            * 100.0
            * 100.0)
            .round()
            / 100.0;
        println!("  harness_verify pass rate: {pass_rate}%");
    }
    if let Some(rate) = report.harness_verify_self_heal_rate {
        println!("  harness_verify self-heal (retry>0 pass): {rate}%");
    }
    println!(
        "  stage_gate_blocked events: {}",
        report.stage_gate_blocked_events
    );
    if let Some(note) = &report.note {
        println!("  note: {note}");
    }

    if !report.top_by_calls.is_empty() {
        println!();
        println!("{}", "Top tools by calls".bold());
        for tool in &report.top_by_calls {
            let rate = tool
                .failure_rate
                .map(|r| format!(" fail {r}%"))
                .unwrap_or_default();
            println!(
                "  - {}: {} calls ({} fail, {} blocked){}",
                tool.name, tool.calls, tool.failures, tool.blocked, rate
            );
        }
    }

    if !report.top_by_failure_rate.is_empty() {
        println!();
        println!("{}", "Top misused tools (≥3 calls, by failure %)".bold());
        for tool in &report.top_by_failure_rate {
            let rate = tool.failure_rate.unwrap_or(0.0);
            println!(
                "  - {}: {rate}% ({} / {} calls)",
                tool.name, tool.failures, tool.calls
            );
        }
    }

    if !report.hint_coverage_top_failures.is_empty() {
        println!();
        println!("{}", "Failure hint coverage (T3)".bold());
        if let Some(rate) = report.hint_coverage_rate {
            println!("  top-failure tools covered: {rate}%");
        }
        for entry in &report.hint_coverage_top_failures {
            let mark = if entry.hint_covered { "✓" } else { "✗" };
            let summary = entry
                .hint_summary
                .as_deref()
                .unwrap_or("(no static hint yet)");
            println!("  {mark} {} — {}", entry.name, summary);
        }
    }

    if let Some(seq) = &report.tool_sequences {
        println!();
        println!("{}", "Tool sequences (T5 candidacy)".bold());
        println!(
            "  turns with tools: {} · threshold: {}%",
            seq.turns_with_tools, seq.threshold_pct
        );
        if seq.t5_candidates.is_empty() {
            println!("  no patterns ≥ threshold yet");
        } else {
            for stat in &seq.t5_candidates {
                println!(
                    "  ✓ {} — {} turns ({:.2}%)",
                    stat.pattern, stat.turn_hits, stat.turn_share_pct
                );
            }
        }
        if !seq.top_patterns.is_empty() {
            println!("  top patterns:");
            for stat in seq.top_patterns.iter().take(8) {
                let mark = if stat.t5_eligible { "✓" } else { "·" };
                println!(
                    "  {mark} {} — {} turns ({:.2}%)",
                    stat.pattern, stat.turn_hits, stat.turn_share_pct
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use zagens_core::engine::kernel_event::{KernelEvent, ToolOutcome, TurnOutcome};
    use zagens_core::turn::TurnLoopMode;
    use zagens_runtime_adapters::persist::kernel_event_log::{
        KernelEventLog, ensure_kernel_events_table,
    };

    fn write_fixture_db(dir: &std::path::Path) -> std::path::PathBuf {
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
                call_id: format!("call-{name}"),
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
            skill: "python-csv-pipeline".into(),
            stage: "inspect".into(),
            tool_name: "exec_shell".into(),
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

    #[test]
    fn doctor_tools_reexports_harness_telemetry() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = write_fixture_db(dir.path());
        let report = build_tool_telemetry_report(&db_path).expect("report");
        assert_eq!(report.tool_calls, 5);
        assert_eq!(report.harness_verify_events, 1);
        assert_eq!(report.stage_gate_blocked_events, 1);
    }
}
