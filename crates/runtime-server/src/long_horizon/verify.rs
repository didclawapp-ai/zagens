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
}
