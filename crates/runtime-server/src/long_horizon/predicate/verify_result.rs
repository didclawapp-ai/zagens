//! Layer-2 verify run DTO shared by manifest gate and the predicate library.

use serde::Serialize;

const STDOUT_TAIL_MAX: usize = 2_048;

/// Per-verify exit classification (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerifyExitClass {
    Ok,
    Assertion,
    Infra,
    Timeout,
    Cancelled,
}

/// One harness-active verify run.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyRunResult {
    pub id: String,
    pub command_display: String,
    pub exit_code: i32,
    pub exit_class: VerifyExitClass,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

impl VerifyRunResult {
    #[must_use]
    pub fn pass(&self) -> bool {
        self.exit_code == 0 && self.exit_class == VerifyExitClass::Ok
    }
}

#[must_use]
pub fn classify_exit(code: i32, stderr: &str, timed_out: bool) -> VerifyExitClass {
    if timed_out {
        return VerifyExitClass::Timeout;
    }
    if code == 0 {
        return VerifyExitClass::Ok;
    }
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("spawn eperm")
        || lower.contains("spawn eacces")
        || (lower.contains("error: spawn") && (lower.contains("eperm") || lower.contains("eacces")))
    {
        return VerifyExitClass::Infra;
    }
    if lower.contains("not found")
        || lower.contains("not recognized")
        || lower.contains("no such file")
        || lower.contains("segmentation fault")
        || lower.contains("access violation")
    {
        VerifyExitClass::Infra
    } else {
        VerifyExitClass::Assertion
    }
}

#[must_use]
pub fn tail(s: &str) -> String {
    if s.len() <= STDOUT_TAIL_MAX {
        s.to_string()
    } else {
        format!("...{}", &s[s.len().saturating_sub(STDOUT_TAIL_MAX)..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_zero_is_ok() {
        assert_eq!(classify_exit(0, "", false), VerifyExitClass::Ok);
    }

    #[test]
    fn classify_test_failure_as_assertion() {
        assert_eq!(
            classify_exit(1, "FAIL: TestFoo", false),
            VerifyExitClass::Assertion
        );
    }

    #[test]
    fn classify_command_not_found_as_infra() {
        assert_eq!(
            classify_exit(127, "bash: foo: command not found", false),
            VerifyExitClass::Infra
        );
    }
}
