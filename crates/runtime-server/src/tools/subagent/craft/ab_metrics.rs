//! C9: CRAFT A/B metrics collector.
//!
//! Records per-task execution telemetry to `.zagens/craft-ab-metrics.jsonl`
//! so single-agent runs can be compared against CRAFT-chain runs offline.
//!
//! ## What is recorded
//!
//! Each completed task appends one JSON line to the metrics file:
//!
//! ```json
//! {
//!   "schema": 1,
//!   "ts": "2026-06-14T14:00:00Z",
//!   "task_id": "fix-null-check",
//!   "mode": "craft",            // "craft" | "single_agent"
//!   "implementer_rounds": 2,
//!   "reviewer_rounds": 2,
//!   "verifier_rounds": 1,
//!   "terminal_verdict": "PASS", // last Verifier/Reviewer verdict
//!   "evidence_downgrades": 1,   // C3 BLOCKER→MAJOR downgrades seen
//!   "gate_fails": 0,            // C8 pre-review gate failures
//!   "duration_ms": 42000,
//!   "notes": ""
//! }
//! ```
//!
//! ## Analysis
//!
//! See `doc_Private/docs/harness/CRAFT_AB_RUNBOOK.md` for the full analysis
//! methodology, decision thresholds, and how to run headless A/B comparisons.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zagens_config::workspace_meta_file_write;

use crate::tools::subagent::blackboard::{
    implementer_round_count, reviewer_round_count, verifier_round_count,
};

/// JSONL record schema version.
const SCHEMA_VERSION: u32 = 1;

/// Relative path inside `.zagens/` for the metrics file.
const METRICS_REL: &str = "craft-ab-metrics.jsonl";

/// Task execution mode for the A/B comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CraftMode {
    /// Full CRAFT chain: Explorer → Implementer → Reviewer → Verifier.
    Craft,
    /// Bare single-agent run (no CRAFT orchestration).
    SingleAgent,
}

impl CraftMode {
    /// Infer mode from round counts: if implementer_rounds > 0 we assume CRAFT.
    pub fn infer(implementer_rounds: u32) -> Self {
        if implementer_rounds > 0 {
            Self::Craft
        } else {
            Self::SingleAgent
        }
    }
}

/// One CRAFT A/B task record written to the JSONL file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CraftAbRecord {
    pub schema: u32,
    /// ISO-8601 timestamp (UTC, seconds precision).
    pub ts: String,
    pub task_id: String,
    pub mode: CraftMode,
    pub implementer_rounds: u32,
    pub reviewer_rounds: u32,
    pub verifier_rounds: u32,
    /// Terminal verdict of the last Verifier or Reviewer that ran.
    pub terminal_verdict: String,
    /// Number of C3 BLOCKER→MAJOR evidence downgrades seen in this task.
    pub evidence_downgrades: u32,
    /// Number of C8 pre-review gate failures in this task.
    pub gate_fails: u32,
    /// Total wall-clock ms from first agent spawn to terminal state.
    pub duration_ms: u64,
    /// Free-form notes (empty by default; set by callers for manual runs).
    #[serde(skip_serializing_if = "String::is_empty", default)]
    pub notes: String,
}

impl CraftAbRecord {
    /// Build a record by reading round counts from the blackboard.
    pub fn from_blackboard(
        workspace: &Path,
        task_id: &str,
        terminal_verdict: &str,
        evidence_downgrades: u32,
        gate_fails: u32,
        duration_ms: u64,
        notes: &str,
    ) -> Self {
        let impl_rounds = implementer_round_count(workspace, task_id);
        let rev_rounds = reviewer_round_count(workspace, task_id);
        let ver_rounds = verifier_round_count(workspace, task_id);

        Self {
            schema: SCHEMA_VERSION,
            ts: unix_ts_str(),
            task_id: task_id.to_string(),
            mode: CraftMode::infer(impl_rounds),
            implementer_rounds: impl_rounds,
            reviewer_rounds: rev_rounds,
            verifier_rounds: ver_rounds,
            terminal_verdict: terminal_verdict.to_string(),
            evidence_downgrades,
            gate_fails,
            duration_ms,
            notes: notes.to_string(),
        }
    }

    /// Append this record as a JSON line to `.zagens/craft-ab-metrics.jsonl`.
    ///
    /// Creates the file (and parent directory) if they don't exist.
    /// Silently ignores write errors to avoid breaking the main path.
    pub fn append_to_file(&self, workspace: &Path) {
        let path = workspace_meta_file_write(workspace, METRICS_REL);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let Ok(mut line) = serde_json::to_string(self) else {
            return;
        };
        line.push('\n');
        // Use OpenOptions with write + create + append for cross-platform safety.
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// Return current UTC time as `YYYY-MM-DDTHH:MM:SSZ` (seconds precision).
fn unix_ts_str() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Manual ISO-8601 formatting to avoid pulling in chrono.
    let s = secs;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let mut days = s / 86400; // days since 1970-01-01
    // Gregorian calendar unrolled (good for years 1970–2100)
    let mut year = 1970u32;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let month_days: [u64; 12] = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut month = 1u32;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

/// Read all records from the metrics file (for analysis / testing).
/// Returns an empty vec if the file does not exist or is unreadable.
pub fn read_metrics(workspace: &Path) -> Vec<CraftAbRecord> {
    let path = workspace_meta_file_write(workspace, METRICS_REL);
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Simple aggregate over a slice of records (used in runbook analysis).
#[derive(Debug, Default)]
pub struct CraftAbSummary {
    pub total_tasks: usize,
    pub pass_on_first_impl_round: usize,
    pub avg_impl_rounds: f64,
    pub avg_reviewer_rounds: f64,
    pub evidence_downgrade_total: u32,
    pub gate_fail_total: u32,
}

impl CraftAbSummary {
    pub fn from_records(records: &[CraftAbRecord]) -> Self {
        if records.is_empty() {
            return Self::default();
        }
        let total = records.len();
        let first_round_pass = records
            .iter()
            .filter(|r| r.implementer_rounds <= 1 && r.terminal_verdict == "PASS")
            .count();
        let avg_impl = records
            .iter()
            .map(|r| r.implementer_rounds as f64)
            .sum::<f64>()
            / total as f64;
        let avg_rev = records
            .iter()
            .map(|r| r.reviewer_rounds as f64)
            .sum::<f64>()
            / total as f64;
        let evidence_downgrades = records.iter().map(|r| r.evidence_downgrades).sum();
        let gate_fails = records.iter().map(|r| r.gate_fails).sum();
        Self {
            total_tasks: total,
            pass_on_first_impl_round: first_round_pass,
            avg_impl_rounds: avg_impl,
            avg_reviewer_rounds: avg_rev,
            evidence_downgrade_total: evidence_downgrades,
            gate_fail_total: gate_fails,
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tmp() -> TempDir {
        tempfile::Builder::new()
            .prefix("craft-ab-test-")
            .tempdir()
            .expect("tempdir")
    }

    #[test]
    fn jsonl_append_and_read_roundtrip() {
        let dir = tmp();
        let ws = dir.path();
        // Explicitly create the .zagens directory
        let zagens_dir = ws.join(".zagens");
        std::fs::create_dir_all(&zagens_dir).expect("create .zagens dir");

        let metrics_path = zagens_dir.join("craft-ab-metrics.jsonl");

        let rec = CraftAbRecord {
            schema: SCHEMA_VERSION,
            ts: unix_ts_str(),
            task_id: "t-append".to_string(),
            mode: CraftMode::Craft,
            implementer_rounds: 2,
            reviewer_rounds: 2,
            verifier_rounds: 1,
            terminal_verdict: "PASS".to_string(),
            evidence_downgrades: 0,
            gate_fails: 1,
            duration_ms: 30_000,
            notes: String::new(),
        };
        rec.append_to_file(ws);
        rec.append_to_file(ws);

        let file_content =
            std::fs::read_to_string(&metrics_path).unwrap_or_else(|e| format!("<read error: {e}>"));
        assert!(
            !file_content.is_empty() && !file_content.starts_with("<read error"),
            "metrics file should be non-empty, got: {file_content:?}"
        );

        let loaded = read_metrics(ws);
        assert_eq!(
            loaded.len(),
            2,
            "expected 2 records in JSONL, raw content: {file_content:?}"
        );
        assert_eq!(loaded[0].task_id, "t-append");
        assert_eq!(loaded[1].reviewer_rounds, 2);
        assert_eq!(loaded[0].gate_fails, 1);
    }

    #[test]
    fn from_blackboard_reads_rounds() {
        use crate::tools::subagent::blackboard::write_blackboard_partition;
        use zagens_core::subagent::{
            StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus, SubAgentType,
            VerdictLevel,
        };

        let dir = tmp();
        let ws = dir.path();
        let task_id = "ab-task";

        fn make_result(agent_type: SubAgentType, verdict: VerdictLevel) -> SubAgentResult {
            SubAgentResult {
                agent_id: "x".into(),
                agent_type,
                assignment: SubAgentAssignment::new("".into(), None),
                model: "m".into(),
                nickname: None,
                status: SubAgentStatus::Completed,
                result: Some("done".into()),
                steps_taken: 1,
                tools_executed: 0,
                duration_ms: 100,
                from_prior_session: false,
                structured_verdict: Some(StructuredVerdict {
                    verdict,
                    items: vec![],
                    summary: None,
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

        write_blackboard_partition(
            ws,
            task_id,
            &SubAgentType::Implementer,
            &make_result(SubAgentType::Implementer, VerdictLevel::Pass),
        );
        write_blackboard_partition(
            ws,
            task_id,
            &SubAgentType::Implementer,
            &make_result(SubAgentType::Implementer, VerdictLevel::Pass),
        );
        write_blackboard_partition(
            ws,
            task_id,
            &SubAgentType::Review,
            &make_result(SubAgentType::Review, VerdictLevel::Pass),
        );
        write_blackboard_partition(
            ws,
            task_id,
            &SubAgentType::Verifier,
            &make_result(SubAgentType::Verifier, VerdictLevel::Pass),
        );

        let rec = CraftAbRecord::from_blackboard(ws, task_id, "PASS", 0, 0, 5_000, "note");
        assert_eq!(rec.implementer_rounds, 2);
        assert_eq!(rec.reviewer_rounds, 1);
        assert_eq!(rec.verifier_rounds, 1);
        assert_eq!(rec.mode, CraftMode::Craft);
        assert_eq!(rec.notes, "note");
    }

    #[test]
    fn summary_first_round_pass() {
        let records = vec![
            CraftAbRecord {
                schema: 1,
                ts: "2026-01-01T00:00:00Z".into(),
                task_id: "t1".into(),
                mode: CraftMode::Craft,
                implementer_rounds: 1,
                reviewer_rounds: 1,
                verifier_rounds: 1,
                terminal_verdict: "PASS".into(),
                evidence_downgrades: 0,
                gate_fails: 0,
                duration_ms: 10_000,
                notes: String::new(),
            },
            CraftAbRecord {
                schema: 1,
                ts: "2026-01-01T00:00:00Z".into(),
                task_id: "t2".into(),
                mode: CraftMode::Craft,
                implementer_rounds: 3,
                reviewer_rounds: 2,
                verifier_rounds: 1,
                terminal_verdict: "PASS".into(),
                evidence_downgrades: 1,
                gate_fails: 2,
                duration_ms: 90_000,
                notes: String::new(),
            },
        ];
        let summary = CraftAbSummary::from_records(&records);
        assert_eq!(summary.total_tasks, 2);
        assert_eq!(summary.pass_on_first_impl_round, 1);
        assert!((summary.avg_impl_rounds - 2.0).abs() < 0.01);
        assert_eq!(summary.evidence_downgrade_total, 1);
        assert_eq!(summary.gate_fail_total, 2);
    }

    #[test]
    fn infer_mode_from_rounds() {
        assert_eq!(CraftMode::infer(0), CraftMode::SingleAgent);
        assert_eq!(CraftMode::infer(1), CraftMode::Craft);
        assert_eq!(CraftMode::infer(3), CraftMode::Craft);
    }

    #[test]
    fn unix_ts_str_looks_like_iso8601() {
        let ts = unix_ts_str();
        assert!(ts.len() == 20, "expected 20 chars, got {ts}");
        assert!(ts.ends_with('Z'));
        assert!(ts.contains('T'));
    }
}
