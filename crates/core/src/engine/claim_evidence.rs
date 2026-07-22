//! Lightweight claim ↔ evidence reconciliation for prose-only turn endings.
//!
//! When the assistant asserts file-path facts without recent tool citations,
//! inject a short nudge so the next step gathers evidence instead of inventing.

use std::sync::LazyLock;

use regex::Regex;

use crate::chat::{ContentBlock, Message};

const RECENT_TOOL_RESULTS: usize = 16;
const MAX_CLAIMED_PATHS: usize = 8;

/// Build an optional user nudge when assistant prose makes strong path claims
/// that are not covered by recent tool-result evidence citations.
#[must_use]
pub fn maybe_unverified_path_claim_nudge(
    messages: &[Message],
    assistant_text: &str,
) -> Option<String> {
    let text = assistant_text.trim();
    if text.is_empty() || !has_strong_claim_signal(text) {
        return None;
    }

    let cited = collect_recent_cited_paths(messages, RECENT_TOOL_RESULTS);
    let claimed = extract_path_claims(text);
    if claimed.is_empty() {
        return None;
    }

    let missing: Vec<&str> = claimed
        .iter()
        .map(String::as_str)
        .filter(|p| !cited.iter().any(|c| paths_match(c, p)))
        .take(MAX_CLAIMED_PATHS)
        .collect();
    if missing.is_empty() {
        return None;
    }

    Some(format!(
        "[evidence check] The previous reply asserted path facts without matching \
         recent tool citations for: {}.\n\
         Treat those claims as unverified. Call investigate/read_file/grep_files \
         (or change_and_verify for edits) and cite paths before restating them as fact.",
        missing.join(", ")
    ))
}

fn has_strong_claim_signal(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("fixed")
        || lower.contains("updated")
        || lower.contains("created")
        || lower.contains("exists")
        || lower.contains("already in")
        || lower.contains("已修复")
        || lower.contains("已修改")
        || lower.contains("已更新")
        || lower.contains("已创建")
        || lower.contains("存在于")
        || lower.contains("写入了")
        || text.contains(":line ")
        || text.contains(" line ")
        || PATH_LINE.is_match(text)
}

/// Rough path:line matcher used as a strong-claim signal.
static PATH_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)[\w./\\-]+\.(rs|ts|tsx|js|jsx|py|go|md|toml|json|yaml|yml):\d+")
        .expect("path:line regex")
});

static PATH_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    // Delimiter class avoids nested quotes in the Rust string literal.
    Regex::new(
        "(?i)(?:^|[\\s`'\\(])((?:[\\w.-]+/)*[\\w.-]+\\.(?:rs|ts|tsx|js|jsx|py|go|md|toml|json|yaml|yml))",
    )
    .expect("path token regex")
});

fn extract_path_claims(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for cap in PATH_TOKEN.captures_iter(text) {
        let Some(m) = cap.get(1) else { continue };
        let path = normalize_path(m.as_str());
        if path.is_empty() || out.iter().any(|p| p == &path) {
            continue;
        }
        out.push(path);
        if out.len() >= MAX_CLAIMED_PATHS {
            break;
        }
    }
    out
}

fn collect_recent_cited_paths(messages: &[Message], limit: usize) -> Vec<String> {
    let mut cited = Vec::new();
    let mut seen_results = 0usize;
    for msg in messages.iter().rev() {
        for block in &msg.content {
            let ContentBlock::ToolResult { content, .. } = block else {
                continue;
            };
            seen_results += 1;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed
                    .strip_prefix("- cite:")
                    .or_else(|| trimmed.strip_prefix("cite:"))
                {
                    let path = normalize_path(&cite_path_from_display(rest.trim()));
                    if !path.is_empty() && !cited.iter().any(|c| c == &path) {
                        cited.push(path);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("- fact: path=") {
                    let path = normalize_path(rest.trim());
                    if !path.is_empty() && !cited.iter().any(|c| c == &path) {
                        cited.push(path);
                    }
                }
            }
            if seen_results >= limit {
                return cited;
            }
        }
    }
    cited
}

/// Parse `path`, `path:line`, or `path:start-end` without breaking `F:` drives.
fn cite_path_from_display(rest: &str) -> String {
    let rest = rest.trim();
    if let Some((path, lines)) = rest.rsplit_once(':') {
        let line_ok = lines
            .split('-')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
        if line_ok {
            return path.to_string();
        }
    }
    rest.to_string()
}

fn normalize_path(raw: &str) -> String {
    crate::engine::path_normalize::normalize_repo_path(raw)
}

fn paths_match(cited: &str, claimed: &str) -> bool {
    crate::engine::path_normalize::repo_paths_match(cited, claimed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_result(content: &str) -> Message {
        Message {
            role: "user".into(),
            content: vec![ContentBlock::ToolResult {
                tool_use_id: "t1".into(),
                content: content.into(),
                is_error: None,
                content_blocks: None,
            }],
        }
    }

    #[test]
    fn nudge_when_path_claim_lacks_citation() {
        let messages = vec![tool_result(
            "[evidence uncertainty=none]\n- fact: match_count=1\n- cite: src/other.rs:10",
        )];
        let nudge = maybe_unverified_path_claim_nudge(
            &messages,
            "I fixed `crates/core/src/lib.rs` and updated the helper.",
        );
        assert!(nudge.is_some());
        assert!(nudge.unwrap().contains("crates/core/src/lib.rs"));
    }

    #[test]
    fn no_nudge_when_citation_covers_claim() {
        let messages = vec![tool_result(
            "[evidence uncertainty=none]\n- cite: crates/core/src/lib.rs:1-20",
        )];
        let nudge = maybe_unverified_path_claim_nudge(
            &messages,
            "I fixed `crates/core/src/lib.rs` as shown above.",
        );
        assert!(nudge.is_none());
    }

    #[test]
    fn no_nudge_without_strong_signal() {
        let messages = vec![tool_result("- cite: src/a.rs:1")];
        let nudge =
            maybe_unverified_path_claim_nudge(&messages, "Looking at `src/b.rs` next might help.");
        assert!(nudge.is_none());
    }

    #[test]
    fn paths_match_requires_path_boundary() {
        assert!(paths_match("crates/core/src/lib.rs", "src/lib.rs"));
        assert!(!paths_match("src/ba.rs", "a.rs"));
    }

    #[test]
    fn no_nudge_when_verbatim_absolute_citation_covers_relative_claim() {
        let messages = vec![tool_result(
            "[evidence uncertainty=none]\n- cite: //?/F:/repo/crates/core/src/engine/citation_auditor.rs:1-20",
        )];
        let nudge = maybe_unverified_path_claim_nudge(
            &messages,
            "I updated `citation_auditor.rs` as shown above.",
        );
        assert!(nudge.is_none());
    }
}
