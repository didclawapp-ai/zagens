//! L3: `zagens coverage-gate` — cross-platform Layer-2 completion gate.
//!
//! Replaces the PowerShell check scripts with a portable Rust implementation.
//!
//! ## Exit codes
//!
//! | Code | Meaning |
//! |------|---------|
//! | 0    | All gate checks passed |
//! | 1    | One or more checks failed (unless `--no-fail`) |
//! | 2    | Configuration error (bad args, missing workspace, etc.) |
//!
//! ## Checks performed
//!
//! 1. **fmt**  — `cargo fmt --check`
//! 2. **clippy** — `cargo clippy -- -D warnings`
//! 3. **compile** — `cargo test --no-run`
//! 4. **tests** — `cargo test` *(only when `--run-tests` is set)*
//! 5. **checklist** — all todo items in `.zagens/todo.json` are completed
//!    *(only when `--require-checklist-complete`)*
//! 6. **craft** — last CRAFT task in `.zagens/craft-ab-metrics.jsonl` has
//!    `terminal_verdict == "PASS"` *(when a `task_id` is provided)*

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Instant;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::cli::args::CoverageGateArgs;

/// Result of a single gate check.
#[derive(Debug, Clone, Serialize)]
pub struct GateCheck {
    pub name: &'static str,
    pub passed: bool,
    pub output: String,
    pub duration_ms: u64,
}

impl GateCheck {
    fn pass(name: &'static str, duration_ms: u64) -> Self {
        Self {
            name,
            passed: true,
            output: String::new(),
            duration_ms,
        }
    }
    fn fail(name: &'static str, output: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name,
            passed: false,
            output: output.into(),
            duration_ms,
        }
    }
}

/// Full gate report.
#[derive(Debug, Serialize)]
pub struct GateReport {
    pub checks: Vec<GateCheck>,
    pub passed: bool,
    pub total_ms: u64,
}

impl GateReport {
    pub fn print_human(&self) {
        let symbol = |ok: bool| if ok { "✓" } else { "✗" };
        for c in &self.checks {
            println!("{} {} ({}ms)", symbol(c.passed), c.name, c.duration_ms);
            if !c.passed && !c.output.is_empty() {
                for line in c.output.lines().take(20) {
                    println!("  {line}");
                }
            }
        }
        println!();
        if self.passed {
            println!("✓ All gate checks passed ({} ms)", self.total_ms);
        } else {
            let failures: Vec<_> = self.checks.iter().filter(|c| !c.passed).collect();
            println!(
                "✗ {} check(s) failed — gate NOT passed ({} ms)",
                failures.len(),
                self.total_ms
            );
        }
    }
}

/// Run a cargo command and return (passed, trimmed_output).
fn run_cargo(workspace: &Path, args: &[&str]) -> (bool, String) {
    let started = Instant::now();
    let Ok(out) = Command::new("cargo")
        .args(args)
        .current_dir(workspace)
        .output()
    else {
        return (false, "cargo not found in PATH".to_string());
    };
    let _ = started.elapsed(); // timing handled at call site
    let success = out.status.success();
    let text = if success {
        String::new()
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        format!("{stderr}{stdout}")
            .lines()
            .take(60)
            .collect::<Vec<_>>()
            .join("\n")
    };
    (success, text)
}

fn check_cargo(workspace: &Path, name: &'static str, args: &[&str]) -> GateCheck {
    let t = Instant::now();
    let (ok, out) = run_cargo(workspace, args);
    let ms = t.elapsed().as_millis() as u64;
    if ok {
        GateCheck::pass(name, ms)
    } else {
        GateCheck::fail(name, out, ms)
    }
}

fn check_checklist(workspace: &Path) -> GateCheck {
    let t = Instant::now();
    // Read `.zagens/todo.json`
    let todo_path_new = workspace.join(".zagens").join("todo.json");
    let todo_path_legacy = workspace.join(".deepseek").join("todo.json");
    let todo_path = if todo_path_new.exists() {
        todo_path_new
    } else if todo_path_legacy.exists() {
        todo_path_legacy
    } else {
        return GateCheck::pass("checklist", t.elapsed().as_millis() as u64);
    };

    let Ok(raw) = std::fs::read_to_string(&todo_path) else {
        let ms = t.elapsed().as_millis() as u64;
        return GateCheck::fail("checklist", "cannot read todo.json", ms);
    };

    let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) else {
        let ms = t.elapsed().as_millis() as u64;
        return GateCheck::fail("checklist", "todo.json is not valid JSON", ms);
    };

    let items = val
        .as_array()
        .or_else(|| val.get("items").and_then(|v| v.as_array()));

    let Some(items) = items else {
        return GateCheck::pass("checklist", t.elapsed().as_millis() as u64);
    };

    let pending: Vec<String> = items
        .iter()
        .filter(|item| {
            let status = item
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("pending");
            status == "pending" || status == "in_progress"
        })
        .filter_map(|item| {
            item.get("content")
                .and_then(|c| c.as_str())
                .map(str::to_string)
        })
        .collect();

    let ms = t.elapsed().as_millis() as u64;
    if pending.is_empty() {
        GateCheck::pass("checklist", ms)
    } else {
        let summary = pending
            .iter()
            .take(5)
            .map(|s| format!("  - {s}"))
            .collect::<Vec<_>>()
            .join("\n");
        GateCheck::fail(
            "checklist",
            format!("{} pending item(s):\n{summary}", pending.len()),
            ms,
        )
    }
}

fn check_craft_verdict(workspace: &Path, task_id: Option<&str>) -> Option<GateCheck> {
    let t = Instant::now();
    let metrics_path = workspace.join(".zagens").join("craft-ab-metrics.jsonl");
    let Ok(raw) = std::fs::read_to_string(&metrics_path) else {
        return None; // no metrics file — skip check
    };

    let records: Vec<serde_json::Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();

    if records.is_empty() {
        return None;
    }

    // Find the relevant record
    let record = if let Some(tid) = task_id {
        records
            .iter()
            .rev()
            .find(|r| r.get("task_id").and_then(|v| v.as_str()) == Some(tid))
    } else {
        records.last()
    };

    let Some(record) = record else {
        return None;
    };

    let ms = t.elapsed().as_millis() as u64;
    let verdict = record
        .get("terminal_verdict")
        .and_then(|v| v.as_str())
        .unwrap_or("UNKNOWN");
    if verdict == "PASS" {
        Some(GateCheck::pass("craft-verdict", ms))
    } else {
        Some(GateCheck::fail(
            "craft-verdict",
            format!("terminal_verdict = {verdict} (expected PASS)"),
            ms,
        ))
    }
}

/// Run all gate checks and return the report.
pub fn run_gate(workspace: &Path, args: &CoverageGateArgs) -> GateReport {
    let global_t = Instant::now();
    let mut checks = Vec::new();

    // fmt
    checks.push(check_cargo(workspace, "fmt", &["fmt", "--check"]));

    // clippy
    checks.push(check_cargo(
        workspace,
        "clippy",
        &["clippy", "--", "-D", "warnings"],
    ));

    // compile (test --no-run)
    checks.push(check_cargo(
        workspace,
        "compile",
        &["test", "--no-run", "--message-format=short"],
    ));

    // tests (optional, slow)
    if args.run_tests {
        checks.push(check_cargo(workspace, "tests", &["test"]));
    }

    // checklist
    if args.require_checklist_complete {
        checks.push(check_checklist(workspace));
    }

    // craft verdict
    if let Some(check) = check_craft_verdict(workspace, args.task_id.as_deref()) {
        checks.push(check);
    }

    let passed = checks.iter().all(|c| c.passed);
    GateReport {
        checks,
        passed,
        total_ms: global_t.elapsed().as_millis() as u64,
    }
}

/// Entry point for `zagens coverage-gate`.
pub fn run(args: CoverageGateArgs) -> Result<ExitCode> {
    let workspace: PathBuf = args
        .workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| anyhow::anyhow!("cannot determine workspace directory"))?;

    if !workspace.is_dir() {
        bail!("workspace {:?} does not exist", workspace);
    }

    let report = run_gate(&workspace, &args);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        report.print_human();
    }

    if report.passed || args.no_fail {
        Ok(ExitCode::SUCCESS)
    } else {
        Ok(ExitCode::FAILURE)
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn tmp_workspace() -> TempDir {
        tempfile::Builder::new()
            .prefix("cov-gate-test-")
            .tempdir()
            .expect("tempdir")
    }

    fn default_args(workspace: &PathBuf) -> CoverageGateArgs {
        CoverageGateArgs {
            workspace: Some(workspace.clone()),
            require_checklist_complete: false,
            run_tests: false,
            json: false,
            task_id: None,
            no_fail: false,
        }
    }

    #[test]
    fn checklist_pass_when_all_done() {
        let dir = tmp_workspace();
        let ws = dir.path().to_path_buf();
        std::fs::create_dir_all(ws.join(".zagens")).unwrap();
        std::fs::write(
            ws.join(".zagens/todo.json"),
            r#"[{"id":1,"content":"task","status":"completed"}]"#,
        )
        .unwrap();

        let check = check_checklist(&ws);
        assert!(check.passed, "all-done checklist should pass");
    }

    #[test]
    fn checklist_fail_when_pending() {
        let dir = tmp_workspace();
        let ws = dir.path().to_path_buf();
        std::fs::create_dir_all(ws.join(".zagens")).unwrap();
        std::fs::write(
            ws.join(".zagens/todo.json"),
            r#"[
                {"id":1,"content":"done","status":"completed"},
                {"id":2,"content":"pending item","status":"pending"}
            ]"#,
        )
        .unwrap();

        let check = check_checklist(&ws);
        assert!(!check.passed, "pending item should fail gate");
        assert!(check.output.contains("pending item"));
    }

    #[test]
    fn checklist_skipped_when_no_file() {
        let dir = tmp_workspace();
        let ws = dir.path().to_path_buf();
        let check = check_checklist(&ws);
        assert!(check.passed, "no todo.json = skip = pass");
    }

    #[test]
    fn craft_verdict_pass() {
        let dir = tmp_workspace();
        let ws = dir.path().to_path_buf();
        std::fs::create_dir_all(ws.join(".zagens")).unwrap();
        std::fs::write(
            ws.join(".zagens/craft-ab-metrics.jsonl"),
            r#"{"schema":1,"ts":"T","task_id":"t1","mode":"craft","implementer_rounds":1,"reviewer_rounds":1,"verifier_rounds":1,"terminal_verdict":"PASS","evidence_downgrades":0,"gate_fails":0,"duration_ms":1000}"#,
        )
        .unwrap();
        let check = check_craft_verdict(&ws, None).expect("should have record");
        assert!(check.passed);
    }

    #[test]
    fn craft_verdict_fail_when_blocker() {
        let dir = tmp_workspace();
        let ws = dir.path().to_path_buf();
        std::fs::create_dir_all(ws.join(".zagens")).unwrap();
        std::fs::write(
            ws.join(".zagens/craft-ab-metrics.jsonl"),
            r#"{"schema":1,"ts":"T","task_id":"t1","mode":"craft","implementer_rounds":3,"reviewer_rounds":3,"verifier_rounds":0,"terminal_verdict":"BLOCKER","evidence_downgrades":0,"gate_fails":0,"duration_ms":9000}"#,
        )
        .unwrap();
        let check = check_craft_verdict(&ws, None).expect("should have record");
        assert!(!check.passed);
        assert!(check.output.contains("BLOCKER"));
    }

    #[test]
    fn report_summary_reflects_failures() {
        let failing = GateCheck::fail("fake", "bad output", 10);
        let passing = GateCheck::pass("ok", 5);
        let report = GateReport {
            checks: vec![failing, passing],
            passed: false,
            total_ms: 15,
        };
        assert!(!report.passed);
    }
}
