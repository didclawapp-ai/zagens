//! Harness / briefing report generation (Phase 2b).

mod context;
mod office;
mod render;
mod templates;

pub use office::{ReportFormats, default_out_dir, write_report_bundle};
pub use render::render_markdown;
pub use templates::{from_night_queue, from_tool_telemetry};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::telemetry::ToolTelemetryReport;

    #[test]
    fn telemetry_markdown_contains_metrics() {
        let report = ToolTelemetryReport {
            sessions_db: "/tmp/sessions.db".into(),
            present: true,
            kernel_event_rows: 12,
            tool_calls: 5,
            tool_failures: 1,
            tool_failure_rate: Some(20.0),
            loop_guard_events: 1,
            loop_guard_retry_rate: Some(20.0),
            harness_verify_events: 2,
            harness_verify_passes: 2,
            harness_verify_self_heal_rate: Some(100.0),
            stage_gate_blocked_events: 3,
            turns_with_tools: 4,
            top_by_calls: vec![],
            top_by_failure_rate: vec![],
            hint_coverage_top_failures: vec![],
            hint_coverage_rate: Some(80.0),
            tool_sequences: None,
            note: None,
        };
        let ctx = from_tool_telemetry(&report);
        let md = render_markdown(&ctx);
        assert!(md.contains("Zagens Harness Report"));
        assert!(md.contains("Stage gate blocked events: 3"));
    }

    #[test]
    fn docx_payload_has_blocks() {
        let report = ToolTelemetryReport::empty(std::path::Path::new("/tmp/missing.db"), "note");
        let ctx = from_tool_telemetry(&report);
        let payload = super::render::build_docx_payload(&ctx);
        assert!(payload["blocks"].is_array());
        assert_eq!(payload["title"], "Zagens Harness Report");
    }
}
