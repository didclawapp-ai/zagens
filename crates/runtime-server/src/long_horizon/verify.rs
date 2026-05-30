//! `[verify: cmd]` checklist prefix (LHT Phase 2).

/// Normalize a shell command for fuzzy comparison.
#[must_use]
pub fn normalize_cmd(command: &str) -> String {
    command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// True when a recent exec command satisfies the checklist `[verify: …]` expectation.
#[must_use]
pub fn verification_satisfied(expected: &str, recent_execs: &[String]) -> bool {
    let expected_norm = normalize_cmd(expected);
    if expected_norm.is_empty() {
        return true;
    }
    recent_execs.iter().any(|ran| commands_match(&expected_norm, ran))
}

fn commands_match(expected_norm: &str, ran_norm: &str) -> bool {
    if expected_norm == ran_norm {
        return true;
    }
    ran_norm.contains(expected_norm) || expected_norm.contains(ran_norm)
}

/// Warning appended to checklist_update result when verify prefix was not satisfied.
#[must_use]
pub fn verify_mismatch_suffix(expected: &str, lang: &str) -> String {
    if lang.starts_with("zh") {
        format!(
            "\n\n[LHT] 警告：已将此项标为 completed，但近期未见匹配的验证命令 `{expected}`。请先运行该命令或撤销 completed。"
        )
    } else {
        format!(
            "\n\n[LHT] Warning: marked completed but no recent run matched verify command `{expected}`. Run it first or revert the status."
        )
    }
}

/// Strip optional `[verify: …]` prefix for display / objective text.
#[must_use]
pub fn strip_verify_prefix(content: &str) -> String {
    let trimmed = content.trim();
    let Some(rest) = trimmed.strip_prefix("[verify:") else {
        return trimmed.to_string();
    };
    rest.split_once(']')
        .map(|(_, after)| after.trim().to_string())
        .unwrap_or_else(|| rest.trim().to_string())
}

/// Parse verification command from checklist content, if present.
#[must_use]
pub fn parse_verify_command(content: &str) -> Option<String> {
    let trimmed = content.trim();
    let rest = trimmed.strip_prefix("[verify:")?;
    let (cmd, _) = rest.split_once(']')?;
    let cmd = cmd.trim();
    if cmd.is_empty() {
        None
    } else {
        Some(cmd.to_string())
    }
}

/// English phrases that mark a checklist item as a *runnable* acceptance —
/// "it builds / tests pass / examples run / lints clean". Deliberately narrow:
/// execution/verification intent, NOT generic "implement X". Used to catch the
/// "false green" smell (item created/claimed done but never actually run).
const EN_ACCEPTANCE_HINTS: &[&str] = &[
    "tests pass",
    "test passes",
    "all tests",
    "build passes",
    "builds clean",
    "compiles",
    "lint clean",
    "lints clean",
    "run the example",
    "run examples",
    "examples run",
    "runs green",
    "go build",
    "go test",
    "go vet",
    "cargo test",
    "cargo build",
    "cargo check",
    "cargo clippy",
    "npm test",
    "pnpm test",
    "yarn test",
    "pytest",
];

/// Chinese counterparts (matched case-sensitively against the raw content).
const ZH_ACCEPTANCE_HINTS: &[&str] = &[
    "跑通",
    "全绿",
    "编译通过",
    "构建通过",
    "测试通过",
    "全部通过",
    "验证通过",
    "运行示例",
    "示例脚本",
];

/// Advisory suffix when a *completed* checklist item reads like a runnable
/// acceptance (build / tests pass / examples run) yet carries **no**
/// `[verify: cmd]` prefix — the "false green" smell where the step was created
/// or self-declared done but never actually executed. Returns `None` for plain
/// implementation items and for items already tagged `[verify:]` (those go
/// through the matched-exec gate in [`verify_mismatch_suffix`] instead).
#[must_use]
pub fn unverified_acceptance_suffix(content: &str, lang: &str) -> Option<String> {
    if parse_verify_command(content).is_some() {
        return None;
    }
    let lc = content.to_ascii_lowercase();
    let looks_runnable = EN_ACCEPTANCE_HINTS.iter().any(|k| lc.contains(k))
        || ZH_ACCEPTANCE_HINTS.iter().any(|k| content.contains(k));
    if !looks_runnable {
        return None;
    }
    Some(if lang.starts_with("zh") {
        "\n\n[LHT] 提示：此项看起来是“可运行的验收”（构建 / 测试 / 跑示例），但没有 `[verify: <命令>]` 前缀，无法据此核验你是否真的运行过。请改写为 `[verify: <命令>] <描述>` 并**实际运行该命令、看到通过输出后**再标 completed——创建文件不等于验证通过。".to_string()
    } else {
        "\n\n[LHT] Note: this looks like a runnable acceptance (build / tests pass / run examples) but has no `[verify: <command>]` prefix, so there's no way to confirm you actually ran it. Rewrite it as `[verify: <command>] <label>` and **run that command and see it pass** before marking it completed — creating a file is not the same as verifying it runs.".to_string()
    })
}

/// Compute the verify-gate verdict for a checklist item that was just marked
/// **completed**. Pure (no I/O) so it can back both the per-item
/// `checklist_update`/`todo_update` path and the bulk `checklist_write` path,
/// and be unit-tested directly.
///
/// Returns a stable verdict label (for the `long_horizon.verify_gate` node /
/// `sidecar.log` probe) and an optional advisory suffix to append to the tool
/// result:
/// - tagged `[verify: cmd]` with a recent matching exec → `("verified", None)`
/// - tagged `[verify: cmd]` with no matching exec → `("mismatch", Some(warn))`
/// - untagged but reads like a runnable acceptance → `("unverified_acceptance", Some(note))`
/// - plain implementation item → `("untagged_ok", None)`
#[must_use]
pub fn verify_gate_verdict(
    content: &str,
    recent_execs: &[String],
    lang: &str,
) -> (&'static str, Option<String>) {
    match parse_verify_command(content) {
        Some(expected) => {
            if verification_satisfied(&expected, recent_execs) {
                ("verified", None)
            } else {
                ("mismatch", Some(verify_mismatch_suffix(&expected, lang)))
            }
        }
        None => match unverified_acceptance_suffix(content, lang) {
            Some(s) => ("unverified_acceptance", Some(s)),
            None => ("untagged_ok", None),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_and_parse_verify() {
        let s = "[verify: cargo test -p auth] Run tests";
        assert_eq!(
            parse_verify_command(s).as_deref(),
            Some("cargo test -p auth")
        );
        assert_eq!(strip_verify_prefix(s), "Run tests");
    }

    #[test]
    fn verification_match_substring() {
        let recent = vec![normalize_cmd("cargo test -p auth --no-run")];
        assert!(verification_satisfied("cargo test -p auth", &recent));
        assert!(!verification_satisfied("cargo clippy", &recent));
    }

    #[test]
    fn unverified_acceptance_flags_runnable_items() {
        // Runnable acceptance with no [verify:] prefix → flagged.
        assert!(unverified_acceptance_suffix("Run examples and confirm they pass", "en").is_some());
        assert!(unverified_acceptance_suffix("go test ./... all green", "en").is_some());
        assert!(unverified_acceptance_suffix("跑通全部示例脚本", "zh").is_some());
        assert!(unverified_acceptance_suffix("编译通过且测试通过", "zh").is_some());
    }

    #[test]
    fn unverified_acceptance_ignores_plain_and_tagged_items() {
        // Plain implementation item → not flagged.
        assert!(unverified_acceptance_suffix("Create token/token.go", "en").is_none());
        assert!(unverified_acceptance_suffix("创建 lexer 包", "zh").is_none());
        // Already a [verify:] item → handled by the matched-exec gate, not here.
        assert!(unverified_acceptance_suffix("[verify: go test ./...] tests pass", "en").is_none());
    }

    #[test]
    fn verdict_covers_all_four_cases() {
        let recent = vec![normalize_cmd("go test ./...")];
        // Tagged + matching recent exec → verified, no suffix.
        let (v, s) = verify_gate_verdict("[verify: go test ./...] all green", &recent, "en");
        assert_eq!(v, "verified");
        assert!(s.is_none());
        // Tagged + no matching exec → mismatch, with suffix.
        let (v, s) = verify_gate_verdict("[verify: go vet ./...] no warnings", &recent, "en");
        assert_eq!(v, "mismatch");
        assert!(s.is_some());
        // Untagged but runnable acceptance → unverified_acceptance, with suffix.
        let (v, s) = verify_gate_verdict("go build passes cleanly", &[], "en");
        assert_eq!(v, "unverified_acceptance");
        assert!(s.is_some());
        // Plain implementation item → untagged_ok, no suffix.
        let (v, s) = verify_gate_verdict("Create token/token.go", &[], "en");
        assert_eq!(v, "untagged_ok");
        assert!(s.is_none());
    }
}
