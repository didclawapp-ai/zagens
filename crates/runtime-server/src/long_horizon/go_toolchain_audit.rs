//! Post-exec audits for Go toolchain verify commands.
//!
//! `go test ./...` exits 0 even when every package reports `[no test files]`, and
//! `go test -cover` exits 0 at 4% coverage — both are classic LHT false-greens.
//! This module re-classifies otherwise-successful runs as assertion failures.

/// Minimum package coverage (%) when `-cover` is present in the command or output.
pub const DEFAULT_MIN_COVERAGE_PCT: f64 = 60.0;

/// If a `go test` run reports only `[no test files]` and no `ok` lines, fail it.
#[must_use]
pub fn audit_go_test_output(command: &str, stdout: &str, stderr: &str) -> Option<String> {
    if !is_go_test_command(command) {
        return None;
    }
    let combined = format!("{stdout}\n{stderr}");
    let has_ok = combined
        .lines()
        .any(|line| line.trim_start().starts_with("ok "));
    let has_no_test_files = combined.contains("[no test files]");
    if !has_ok && has_no_test_files {
        return Some(
            "go test exited 0 but every package reported [no test files] — add *_test.go and real tests"
                .to_string(),
        );
    }
    if command.contains("-cover") || combined.contains("coverage:") {
        if let Some(min) = min_coverage_percent(&combined) {
            if min < DEFAULT_MIN_COVERAGE_PCT {
                let mut msg = format!(
                    "go test coverage {:.1}% is below minimum {:.0}%",
                    min, DEFAULT_MIN_COVERAGE_PCT
                );
                if combined.contains("cmd/") || combined.contains("examples/") {
                    msg.push_str(
                        " — extract cmd/examples logic into testable packages or add *_test.go under those paths",
                    );
                }
                return Some(msg);
            }
        }
    }
    None
}

fn is_go_test_command(command: &str) -> bool {
    let tokens: Vec<&str> = command.split_whitespace().collect();
    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i].eq_ignore_ascii_case("go") && tokens[i + 1].eq_ignore_ascii_case("test") {
            // `go test -c` compiles without running — do not audit as a test run.
            return !tokens.iter().any(|t| *t == "-c");
        }
    }
    false
}

/// Parse the lowest `coverage: X.X%` percentage from combined go test output.
fn min_coverage_percent(combined: &str) -> Option<f64> {
    use std::sync::LazyLock;

    static COVERAGE_RE: LazyLock<regex::Regex> =
        LazyLock::new(|| regex::Regex::new(r"coverage:\s*([\d.]+)").expect("COVERAGE_RE"));

    let mut min: Option<f64> = None;
    for line in combined.lines() {
        if line.contains("[no test files]") {
            continue;
        }
        for cap in COVERAGE_RE.captures_iter(line) {
            if let Ok(v) = cap[1].parse::<f64>() {
                min = Some(min.map_or(v, |m| m.min(v)));
            }
        }
    }
    min
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fails_when_only_no_test_files() {
        let out = "?   \tmicrostack-1/cmd/todo\t[no test files]\n?   \tmicrostack-1/ms\t[no test files]\n";
        let err = audit_go_test_output("go test ./...", out, "");
        assert!(err.is_some());
    }

    #[test]
    fn passes_when_ok_with_coverage() {
        let out = "ok  \tmicrostack-1/ms\t(cached)\tcoverage: 71.2% of statements\n";
        assert!(audit_go_test_output("go test -cover ./...", out, "").is_none());
    }

    #[test]
    fn fails_when_coverage_below_min() {
        let out = "ok  \tmicrostack/pkg\t0.12s\tcoverage: 45.0% of statements\n";
        let err = audit_go_test_output("go test -cover ./...", out, "");
        assert!(err.is_some());
        assert!(err.unwrap().contains("45.0%"));
    }

    #[test]
    fn ignores_non_go_test() {
        assert!(audit_go_test_output("go build ./...", "ok", "").is_none());
    }
}
