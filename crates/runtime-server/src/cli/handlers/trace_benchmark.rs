//! `zagens trace benchmark` — Replay v0 corpus validate + baseline diff (Phase 4.4).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use zagens_core::engine::{build_replay_pack_from_fixture, validate_replay_pack};

use crate::cli::args::TraceBenchmarkArgs;
use crate::cli::context::CliContext;
use crate::cli::doctor_tools::{build_tool_telemetry_report, default_sessions_db_path};
use crate::harness::telemetry::ToolTelemetryReport;
use crate::trace_export::build_replay_pack_for_thread;

#[derive(Debug, Clone, Serialize)]
pub struct FixtureBenchRow {
    fixture: String,
    ok: bool,
    event_count: usize,
    golden_replay_compatible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadBenchRow {
    thread_id: String,
    ok: bool,
    event_count: usize,
    includes_session: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineDelta {
    metric: String,
    baseline: Option<f64>,
    current: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    delta_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TraceBenchmarkReport {
    pub schema: &'static str,
    pub generated_at_ms: u64,
    pub replay_dir: String,
    pub fixture_rows: Vec<FixtureBenchRow>,
    pub thread_rows: Vec<ThreadBenchRow>,
    pub fixtures_pass: usize,
    pub fixtures_total: usize,
    pub threads_pass: usize,
    pub threads_total: usize,
    pub tools_telemetry: ToolTelemetryReport,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub baseline_deltas: Vec<BaselineDelta>,
    pub ok: bool,
}

pub fn run(ctx: &CliContext, args: TraceBenchmarkArgs) -> Result<ExitCode> {
    let replay_dir = args.replay_dir.clone().unwrap_or_else(default_replay_dir);
    if !replay_dir.is_dir() {
        bail!("replay dir not found: {}", replay_dir.display());
    }

    let mut fixture_rows = Vec::new();
    for entry in
        fs::read_dir(&replay_dir).with_context(|| format!("read {}", replay_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(".session.json"))
        {
            continue;
        }
        fixture_rows.push(benchmark_fixture(&path));
    }
    fixture_rows.sort_by(|a, b| a.fixture.cmp(&b.fixture));

    let mut thread_rows = Vec::new();
    for thread_id in &args.thread {
        thread_rows.push(benchmark_thread(ctx, thread_id.trim(), args.no_redact)?);
    }

    let fixtures_pass = fixture_rows.iter().filter(|r| r.ok).count();
    let threads_pass = thread_rows.iter().filter(|r| r.ok).count();
    let tools_telemetry = build_tool_telemetry_report(&default_sessions_db_path())?;
    let baseline_deltas = args
        .baseline
        .as_ref()
        .map(|path| diff_baseline(path, &tools_telemetry))
        .transpose()?
        .unwrap_or_default();

    let ok = fixtures_pass == fixture_rows.len()
        && thread_rows.iter().all(|r| r.ok)
        && fixture_rows.iter().any(|r| r.ok);

    let report = TraceBenchmarkReport {
        schema: "zagens-trace-benchmark-v0",
        generated_at_ms: now_ms(),
        replay_dir: replay_dir.display().to_string(),
        fixtures_pass,
        fixtures_total: fixture_rows.len(),
        threads_pass,
        threads_total: thread_rows.len(),
        fixture_rows,
        thread_rows,
        tools_telemetry,
        baseline_deltas,
        ok,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }

    if let Some(out) = args.out {
        fs::write(&out, serde_json::to_string_pretty(&report)?)
            .with_context(|| format!("write {}", out.display()))?;
        eprintln!("Wrote benchmark report → {}", out.display());
    }

    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn default_replay_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/harness/kernel-v3-replay")
}

fn benchmark_fixture(path: &Path) -> FixtureBenchRow {
    let label = path.display().to_string();
    match build_replay_pack_from_fixture(path) {
        Ok(pack) => {
            let validation = validate_replay_pack(&pack);
            FixtureBenchRow {
                fixture: label,
                ok: validation.ok,
                event_count: validation.event_count,
                golden_replay_compatible: validation.golden_replay_compatible,
                error: validation.coherence_error,
            }
        }
        Err(e) => FixtureBenchRow {
            fixture: label,
            ok: false,
            event_count: 0,
            golden_replay_compatible: false,
            error: Some(e.to_string()),
        },
    }
}

fn benchmark_thread(ctx: &CliContext, thread_id: &str, no_redact: bool) -> Result<ThreadBenchRow> {
    if thread_id.is_empty() {
        bail!("empty --thread id");
    }
    match build_replay_pack_for_thread(
        thread_id,
        &ctx.config,
        &ctx.workspace,
        true,
        true,
        !no_redact,
    ) {
        Ok(pack) => {
            let validation = validate_replay_pack(&pack);
            Ok(ThreadBenchRow {
                thread_id: thread_id.to_string(),
                ok: validation.ok,
                event_count: validation.event_count,
                includes_session: validation.includes_session,
                error: validation.coherence_error,
            })
        }
        Err(e) => Ok(ThreadBenchRow {
            thread_id: thread_id.to_string(),
            ok: false,
            event_count: 0,
            includes_session: false,
            error: Some(e.to_string()),
        }),
    }
}

fn diff_baseline(path: &Path, current: &ToolTelemetryReport) -> Result<Vec<BaselineDelta>> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("read baseline {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&raw).context("parse baseline JSON")?;
    let baseline_tools = value.get("tools_telemetry");

    let mut deltas = Vec::new();
    push_rate_delta(
        &mut deltas,
        "tool_failure_rate",
        baseline_tools
            .and_then(|b| b.get("tool_failure_rate"))
            .and_then(|v| v.as_f64()),
        current.tool_failure_rate,
    );
    push_rate_delta(
        &mut deltas,
        "loop_guard_retry_rate",
        baseline_tools
            .and_then(|b| b.get("loop_guard_retry_rate"))
            .and_then(|v| v.as_f64()),
        current.loop_guard_retry_rate,
    );
    push_rate_delta(
        &mut deltas,
        "harness_verify_self_heal_rate",
        baseline_tools
            .and_then(|b| b.get("harness_verify_self_heal_rate"))
            .and_then(|v| v.as_f64()),
        current.harness_verify_self_heal_rate,
    );
    push_rate_delta(
        &mut deltas,
        "hint_coverage_rate",
        baseline_tools
            .and_then(|b| b.get("hint_coverage_rate"))
            .and_then(|v| v.as_f64()),
        current.hint_coverage_rate,
    );
    Ok(deltas)
}

fn push_rate_delta(
    out: &mut Vec<BaselineDelta>,
    metric: &str,
    baseline: Option<f64>,
    current: Option<f64>,
) {
    let delta_pct = match (baseline, current) {
        (Some(b), Some(c)) if b.abs() > f64::EPSILON => {
            Some(((c - b) / b * 100.0 * 100.0).round() / 100.0)
        }
        (Some(b), Some(c)) if b.abs() <= f64::EPSILON && c.abs() > f64::EPSILON => Some(100.0),
        _ => None,
    };
    out.push(BaselineDelta {
        metric: metric.to_string(),
        baseline,
        current,
        delta_pct,
    });
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn print_human(report: &TraceBenchmarkReport) {
    use colored::Colorize;

    println!("{}", "Trace benchmark (Replay v0)".bold());
    println!("  replay_dir: {}", report.replay_dir);
    println!(
        "  fixtures: {}/{} PASS",
        report.fixtures_pass, report.fixtures_total
    );
    if !report.thread_rows.is_empty() {
        println!(
            "  threads: {}/{} PASS",
            report.threads_pass, report.threads_total
        );
    }
    println!(
        "  tool telemetry: {} calls · failure {}%",
        report.tools_telemetry.tool_calls,
        report
            .tools_telemetry
            .tool_failure_rate
            .map(|r| r.to_string())
            .unwrap_or_else(|| "n/a".into())
    );
    if !report.baseline_deltas.is_empty() {
        println!("  baseline deltas:");
        for row in &report.baseline_deltas {
            let b = row
                .baseline
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".into());
            let c = row
                .current
                .map(|v| v.to_string())
                .unwrap_or_else(|| "n/a".into());
            let d = row
                .delta_pct
                .map(|v| format!(" ({v:+.2}% vs baseline)"))
                .unwrap_or_default();
            println!("    - {}: {b} → {c}{d}", row.metric);
        }
    }
    if let Some(seq) = &report.tools_telemetry.tool_sequences
        && !seq.t5_candidates.is_empty()
    {
        println!("  T5 eligible patterns:");
        for stat in &seq.t5_candidates {
            println!("    - {} ({:.2}%)", stat.pattern, stat.turn_share_pct);
        }
    }
    println!(
        "  result: {}",
        if report.ok {
            "PASS".green()
        } else {
            "FAIL".red()
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_replay_dir_has_passing_fixtures() {
        let dir = default_replay_dir();
        if !dir.is_dir() {
            return;
        }
        let rows: Vec<_> = fs::read_dir(&dir)
            .expect("read dir")
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| {
                p.extension().and_then(|e| e.to_str()) == Some("json")
                    && !p
                        .file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.ends_with(".session.json"))
            })
            .map(|p| benchmark_fixture(&p))
            .collect();
        assert!(!rows.is_empty(), "expected golden replay fixtures");
        assert!(
            rows.iter().all(|r| r.ok),
            "fixture failures: {:?}",
            rows.iter().filter(|r| !r.ok).collect::<Vec<_>>()
        );
    }
}
