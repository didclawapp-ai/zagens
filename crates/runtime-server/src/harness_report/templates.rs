//! Build [`ReportContext`] from T1 telemetry and night-queue documents.

use chrono::Utc;

use crate::cli::doctor_tools::ToolTelemetryReport;
use crate::night_queue::{NightQueueDocument, QueueTaskStatus};

use super::context::{ReportContext, ReportSection};

pub fn from_tool_telemetry(report: &ToolTelemetryReport) -> ReportContext {
    let now = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let mut summary = vec![
        format!("Sessions DB: {}", report.sessions_db),
        format!("Kernel event rows: {}", report.kernel_event_rows),
        format!("Turns with tools: {}", report.turns_with_tools),
        format!("Tool calls: {}", report.tool_calls),
    ];
    if let Some(rate) = report.tool_failure_rate {
        summary.push(format!("Tool failure rate: {rate}%"));
    }
    summary.push(format!("Loop-guard events: {}", report.loop_guard_events));
    summary.push(format!(
        "Harness verify events: {} (passes: {})",
        report.harness_verify_events, report.harness_verify_passes
    ));
    if let Some(rate) = report.harness_verify_self_heal_rate {
        summary.push(format!("Harness verify self-heal rate: {rate}%"));
    }
    summary.push(format!(
        "Stage gate blocked events: {}",
        report.stage_gate_blocked_events
    ));
    if let Some(rate) = report.hint_coverage_rate {
        summary.push(format!("Top failure-tool hint coverage: {rate}%"));
    }

    let mut sections = vec![ReportSection::Summary { items: summary }];

    if !report.top_by_calls.is_empty() {
        sections.push(ReportSection::Heading {
            level: 2,
            text: "Top tools by calls".into(),
        });
        sections.push(tool_stat_table(&report.top_by_calls));
    }

    if !report.top_by_failure_rate.is_empty() {
        sections.push(ReportSection::Heading {
            level: 2,
            text: "Top misused tools (≥3 calls)".into(),
        });
        sections.push(tool_stat_table(&report.top_by_failure_rate));
    }

    if !report.hint_coverage_top_failures.is_empty() {
        sections.push(ReportSection::Heading {
            level: 2,
            text: "Failure hints (T3 audit)".into(),
        });
        sections.push(hint_audit_table(&report.hint_coverage_top_failures));
    }

    ReportContext {
        title: "Zagens Harness Report".into(),
        subtitle: Some("Agent telemetry from kernel_events".into()),
        generated_at: now,
        sections,
    }
}

pub fn from_night_queue(doc: &NightQueueDocument) -> ReportContext {
    let now = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    let passed = doc
        .tasks
        .iter()
        .filter(|t| t.status == QueueTaskStatus::Passed)
        .count();
    let failed = doc
        .tasks
        .iter()
        .filter(|t| {
            matches!(
                t.status,
                QueueTaskStatus::Failed | QueueTaskStatus::RolledBack
            )
        })
        .count();
    let pending = doc
        .tasks
        .iter()
        .filter(|t| t.status == QueueTaskStatus::Pending)
        .count();

    let mut summary = vec![
        format!("Generated: {now}"),
        format!("Summary: {passed} passed · {failed} failed/rolled back · {pending} pending"),
    ];
    if let Some(ts) = doc.last_run_at {
        summary.push(format!("Last queue run: {ts}"));
    }

    let mut sections = vec![ReportSection::Summary { items: summary }];

    if doc.tasks.is_empty() {
        sections.push(ReportSection::Paragraph {
            text: "No tasks in queue.".into(),
        });
    } else {
        sections.push(ReportSection::Heading {
            level: 2,
            text: "Tasks".into(),
        });
        for task in &doc.tasks {
            let status = format!("{:?}", task.status).to_lowercase();
            sections.push(ReportSection::Heading {
                level: 3,
                text: format!("{} ({status})", task.id),
            });
            sections.push(ReportSection::Paragraph {
                text: crate::night_queue::preview(&task.prompt, 600),
            });
            if let Some(ref gate) = task.gate_summary {
                sections.push(ReportSection::Paragraph {
                    text: format!("Gate:\n{gate}"),
                });
            }
            if let Some(ref err) = task.error {
                sections.push(ReportSection::Paragraph {
                    text: format!("Error: {err}"),
                });
            }
        }
    }

    ReportContext {
        title: "Night Queue Briefing".into(),
        subtitle: None,
        generated_at: now,
        sections,
    }
}

fn tool_stat_table(tools: &[crate::cli::doctor_tools::ToolStat]) -> ReportSection {
    let mut rows = Vec::new();
    for tool in tools {
        let rate = tool
            .failure_rate
            .map(|r| format!("{r}%"))
            .unwrap_or_else(|| "—".into());
        rows.push(vec![
            tool.name.clone(),
            tool.calls.to_string(),
            tool.failures.to_string(),
            tool.blocked.to_string(),
            rate,
        ]);
    }
    ReportSection::Table {
        title: None,
        headers: vec![
            "Tool".into(),
            "Calls".into(),
            "Failures".into(),
            "Blocked".into(),
            "Fail %".into(),
        ],
        rows,
    }
}

fn hint_audit_table(entries: &[crate::cli::doctor_tools::ToolHintAuditEntry]) -> ReportSection {
    let mut rows = Vec::new();
    for entry in entries {
        rows.push(vec![
            entry.name.clone(),
            entry
                .failure_rate
                .map(|r| format!("{r}%"))
                .unwrap_or_else(|| "—".into()),
            if entry.hint_covered {
                "yes".into()
            } else {
                "no".into()
            },
            entry.hint_summary.clone().unwrap_or_else(|| "—".into()),
        ]);
    }
    ReportSection::Table {
        title: None,
        headers: vec![
            "Tool".into(),
            "Fail %".into(),
            "Hint".into(),
            "Summary".into(),
        ],
        rows,
    }
}
