//! §6.7 Adversarial read-only auditor — agent-independent grounding signal.
//!
//! Implements the "gap enumerator" design from COMPOSABLE_HARNESS.md §6.7:
//!
//! - **No release/veto power**: output never directly gates `graph_complete`.
//! - **Read-only**: reviews checklist + git diff; never modifies artifacts.
//! - **Machine-testable output**: gap candidates carry `[verify: cmd]` or stub
//!   patterns, not prose LGTM/FAIL.
//! - **Agent-independent**: a separate model call with fresh context, not the
//!   same conversation as the builder — satisfies §0.1 "agent-independent"
//!   grounding signal class.
//!
//! After the auditor responds, **machine gates still adjudicate**: candidates
//! are surfaced as telemetry and optionally reinjected as new checklist items
//! with `[verify:]` markers so the normal Layer-2 oracle re-runs them.

use std::path::Path;

use serde::Serialize;
use zagens_core::chat::{ContentBlock, LlmClient, Message, MessageRequest};
use zagens_core::long_horizon::{AdversarialAuditConfig, CompletionGateMode};

use crate::runtime_api::workspace::run_git;
use crate::tools::todo::{TodoItem, TodoListSnapshot, TodoStatus};

use super::nudge::LongHorizonSessionState;

/// A single gap candidate reported by the adversarial auditor.
#[derive(Debug, Clone, Serialize)]
pub struct GapCandidate {
    /// Source file path (relative to workspace), or empty if not file-specific.
    pub file: String,
    /// Optional line number within `file`.
    pub line: Option<u32>,
    /// Human-readable description of the suspected gap.
    pub description: String,
    /// Optional shell command suggested to verify the gap.
    pub suggested_verify: Option<String>,
}

/// Result of one adversarial audit pass.
#[derive(Debug, Clone, Serialize)]
pub struct AdversarialAuditResult {
    pub gap_candidates: Vec<GapCandidate>,
    /// Which audit round within this session this is (1-indexed).
    pub audit_round: u32,
    /// `true` when the audit was skipped because `max_audit_rounds` was already used.
    pub was_bounded: bool,
}

/// Session-tracked counter for adversarial audit rounds (stored on
/// [`LongHorizonSessionState`]).
///
/// Returns `true` if the audit should run (rounds not yet exhausted), and
/// increments the counter.
pub fn should_run_audit(session: &mut LongHorizonSessionState, max: u32) -> bool {
    if session.adversarial_audit_rounds >= max {
        return false;
    }
    session.adversarial_audit_rounds += 1;
    true
}

/// Collect at most `max_lines` lines from `git diff --stat HEAD` for the
/// auditor context.  Returns an empty string when the workspace is not a
/// git repository or the diff is empty.
#[must_use]
fn collect_diff_stat(workspace: &Path, max_lines: usize) -> String {
    let raw = run_git(workspace, &["diff", "--stat", "HEAD"]).unwrap_or_default();
    let lines: Vec<&str> = raw.lines().take(max_lines).collect();
    lines.join("\n")
}

/// Build the system prompt for the adversarial auditor.
fn build_system_prompt() -> String {
    "You are a read-only adversarial code auditor.  \
     Your task is to enumerate implementation gaps that regex-based scanners \
     cannot catch: e.g. bodies that silently return an empty/default value, \
     functions that appear complete but are actually stubs without a `todo!()` marker, \
     or features that are planned in the checklist but absent from the diff.\n\
     \n\
     Rules:\n\
     1. Output ONLY suspected gaps.  If you find none, say \"No gaps found.\"\n\
     2. Each gap must use EXACTLY this format on its own paragraph:\n\
     [GAP] file:line description [verify: cmd] [/GAP]\n\
     Where `file:line` is the relative path and optional line number,\n\
     `description` is one sentence, and `[verify: cmd]` is a shell command\n\
     that would confirm the gap exists (omit the `[verify:]` part if you have no good command).\n\
     3. Do NOT write any code, do NOT suggest edits, do NOT judge pass/fail.\n\
     4. Maximum 8 gap candidates per response."
        .to_string()
}

/// Build the user message for the adversarial auditor.
fn build_user_message(checklist: &TodoListSnapshot, diff_stat: &str, lang: &str) -> String {
    let completed: Vec<&TodoItem> = checklist
        .items
        .iter()
        .filter(|i| i.status == TodoStatus::Completed)
        .collect();
    let pending: Vec<&TodoItem> = checklist
        .items
        .iter()
        .filter(|i| i.status == TodoStatus::Pending || i.status == TodoStatus::InProgress)
        .collect();

    let mut msg = String::new();
    if !lang.is_empty() && lang != "en" {
        msg.push_str("(Respond in English regardless of this locale hint.)\n\n");
    }
    msg.push_str("=== Completed checklist items ===\n");
    for item in &completed {
        msg.push_str(&format!("- {}\n", item.content.trim()));
    }
    if !pending.is_empty() {
        msg.push_str("\n=== Still-pending items ===\n");
        for item in &pending {
            msg.push_str(&format!("- {}\n", item.content.trim()));
        }
    }
    if !diff_stat.is_empty() {
        msg.push_str("\n=== Git diff --stat HEAD (summary) ===\n");
        msg.push_str(diff_stat);
        msg.push('\n');
    }
    msg.push_str(
        "\nEnumerate suspected implementation gaps using the [GAP]…[/GAP] format above. \
         If the diff covers all completed items adequately, say \"No gaps found.\"",
    );
    msg
}

/// Parse `[GAP] … [/GAP]` blocks from the auditor response.
#[must_use]
fn parse_gap_candidates(response: &str) -> Vec<GapCandidate> {
    let mut candidates = Vec::new();
    let mut rest = response;
    while let Some(start) = rest.find("[GAP]") {
        let after_open = &rest[start + 5..];
        let end = after_open.find("[/GAP]").unwrap_or(after_open.len());
        let block = after_open[..end].trim();
        if let Some(candidate) = parse_single_gap(block) {
            candidates.push(candidate);
        }
        rest = &after_open[end..];
        if rest.starts_with("[/GAP]") {
            rest = &rest[6..];
        }
    }
    candidates
}

fn parse_single_gap(block: &str) -> Option<GapCandidate> {
    if block.is_empty() {
        return None;
    }
    // Extract optional `[verify: cmd]`
    let (body, suggested_verify) = if let Some(v_start) = block.find("[verify:") {
        let v_after = &block[v_start + 8..];
        let v_end = v_after.find(']').unwrap_or(v_after.len());
        let cmd = v_after[..v_end].trim().to_string();
        let body = format!(
            "{} {}",
            &block[..v_start],
            &block[v_start + 8 + v_end + 1..]
        )
        .trim()
        .to_string();
        (body, if cmd.is_empty() { None } else { Some(cmd) })
    } else {
        (block.to_string(), None)
    };

    // First token is `file:line` or `file`
    let body = body.trim();
    let (file, line, description) = if let Some(space) = body.find(' ') {
        let location = body[..space].trim();
        let desc = body[space + 1..].trim();
        if let Some(colon) = location.rfind(':') {
            let maybe_line = &location[colon + 1..];
            if let Ok(l) = maybe_line.parse::<u32>() {
                (location[..colon].to_string(), Some(l), desc.to_string())
            } else {
                (location.to_string(), None, desc.to_string())
            }
        } else {
            (location.to_string(), None, desc.to_string())
        }
    } else {
        (body.to_string(), None, String::new())
    };

    if file.is_empty() && description.is_empty() {
        return None;
    }

    Some(GapCandidate {
        file,
        line,
        description,
        suggested_verify,
    })
}

/// Build a reinject nudge message listing gap candidates for the builder model.
#[must_use]
pub fn build_gap_reinject_message(gaps: &[GapCandidate], lang: &str) -> Message {
    let header = if lang.contains("zh") || lang.starts_with("zh") {
        "⚠️ 对抗性审计员发现以下可疑实现缺口（每项须通过机器验证后方可关闭）："
    } else {
        "⚠️ Adversarial audit found suspected implementation gaps (each must pass machine verification before closing):"
    };
    let mut text = header.to_string();
    for (i, g) in gaps.iter().enumerate() {
        let loc = if let Some(l) = g.line {
            format!("{}:{}", g.file, l)
        } else {
            g.file.clone()
        };
        let verify_part = g
            .suggested_verify
            .as_deref()
            .map(|cmd| format!("  `[verify: {cmd}]`"))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n{}. `{}` — {}{}",
            i + 1,
            loc,
            g.description,
            verify_part
        ));
    }
    text.push_str(
        "\n\nPlease address each gap above. \
         Add a `[verify: cmd]` command to the relevant checklist item and run it to confirm the fix.",
    );
    Message {
        role: "user".to_string(),
        content: vec![ContentBlock::Text {
            text,
            cache_control: None,
        }],
    }
}

/// Run the adversarial audit: single `create_message` call (no tools, read-only).
///
/// Returns:
/// - `None` when the audit is disabled, rounds exhausted, or no client.
/// - `Some(AdversarialAuditResult)` with the parsed gap candidates.
pub async fn run_adversarial_audit(
    config: &AdversarialAuditConfig,
    session: &mut LongHorizonSessionState,
    client: &dyn LlmClient,
    model: &str,
    workspace: &Path,
    checklist: &TodoListSnapshot,
    lang: &str,
) -> Option<AdversarialAuditResult> {
    if !config.enabled {
        return None;
    }
    if !should_run_audit(session, config.max_audit_rounds) {
        return Some(AdversarialAuditResult {
            gap_candidates: Vec::new(),
            audit_round: session.adversarial_audit_rounds,
            was_bounded: true,
        });
    }
    let audit_round = session.adversarial_audit_rounds;

    let diff_stat = tokio::task::spawn_blocking({
        let ws = workspace.to_path_buf();
        move || collect_diff_stat(&ws, 60)
    })
    .await
    .unwrap_or_default();

    let system_prompt = build_system_prompt();
    let user_msg = build_user_message(checklist, &diff_stat, lang);

    let request = MessageRequest {
        model: model.to_string(),
        system: Some(zagens_core::chat::SystemPrompt::Text(system_prompt)),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: user_msg,
                cache_control: None,
            }],
        }],
        max_tokens: config.max_tokens,
        stream: None,
        tools: None,
        thinking: None,
        tool_choice: None,
        metadata: None,
        reasoning_effort: None,
        temperature: None,
        top_p: None,
    };

    let response = match client.create_message(request).await {
        Ok(r) => r,
        Err(_) => {
            return Some(AdversarialAuditResult {
                gap_candidates: Vec::new(),
                audit_round,
                was_bounded: false,
            });
        }
    };

    let raw_text: String = response
        .content
        .iter()
        .filter_map(|b| {
            if let ContentBlock::Text { text, .. } = b {
                Some(text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let gap_candidates = parse_gap_candidates(&raw_text);

    Some(AdversarialAuditResult {
        gap_candidates,
        audit_round,
        was_bounded: false,
    })
}

/// Convert gap candidates to new pending checklist item contents (dedup).
#[must_use]
pub fn gaps_to_checklist_contents(
    gaps: &[GapCandidate],
    existing: &[TodoItem],
    mode: CompletionGateMode,
) -> Vec<String> {
    if mode != CompletionGateMode::Enforce {
        return Vec::new();
    }
    let existing_texts: std::collections::HashSet<String> = existing
        .iter()
        .map(|i| i.content.trim().to_string())
        .collect();

    gaps.iter()
        .filter_map(|g| {
            let verify_part = g
                .suggested_verify
                .as_deref()
                .map(|cmd| format!(" [verify: {cmd}]"))
                .unwrap_or_default();
            let loc = if let Some(l) = g.line {
                format!("{}:{}", g.file, l)
            } else {
                g.file.clone()
            };
            let content = format!("AUDIT gap [{loc}]: {}{}", g.description.trim(), verify_part);
            if existing_texts.contains(&content) {
                None
            } else {
                Some(content)
            }
        })
        .collect()
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gap_single_with_verify() {
        let raw = "[GAP] src/main.rs:42 handle_error branch not implemented [verify: cargo test -- error_handling] [/GAP]";
        let gaps = parse_gap_candidates(raw);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].file, "src/main.rs");
        assert_eq!(gaps[0].line, Some(42));
        assert!(gaps[0].description.contains("handle_error"));
        assert_eq!(
            gaps[0].suggested_verify.as_deref(),
            Some("cargo test -- error_handling")
        );
    }

    #[test]
    fn parse_gap_no_verify() {
        let raw = "[GAP] pkg/router.go all routes return 501 Not Implemented [/GAP]";
        let gaps = parse_gap_candidates(raw);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].file, "pkg/router.go");
        assert!(gaps[0].suggested_verify.is_none());
    }

    #[test]
    fn parse_multiple_gaps() {
        let raw = "[GAP] a.rs:1 gap one [/GAP]\n[GAP] b.rs:2 gap two [verify: cargo test] [/GAP]";
        let gaps = parse_gap_candidates(raw);
        assert_eq!(gaps.len(), 2);
    }

    #[test]
    fn parse_no_gaps_found() {
        let raw = "No gaps found.";
        let gaps = parse_gap_candidates(raw);
        assert!(gaps.is_empty());
    }

    #[test]
    fn gaps_to_checklist_deduplicates() {
        use crate::tools::todo::{TodoItem, TodoStatus};
        let gaps = vec![GapCandidate {
            file: "src/lib.rs".to_string(),
            line: Some(10),
            description: "stub body".to_string(),
            suggested_verify: None,
        }];
        let existing_content = "AUDIT gap [src/lib.rs:10]: stub body";
        let existing = vec![TodoItem {
            id: 1,
            content: existing_content.to_string(),
            status: TodoStatus::Pending,
        }];
        let contents = gaps_to_checklist_contents(&gaps, &existing, CompletionGateMode::Enforce);
        assert!(contents.is_empty(), "should dedup against existing items");
    }

    #[test]
    fn gaps_to_checklist_observe_returns_empty() {
        let gaps = vec![GapCandidate {
            file: "src/lib.rs".to_string(),
            line: None,
            description: "missing impl".to_string(),
            suggested_verify: Some("cargo test".to_string()),
        }];
        let contents = gaps_to_checklist_contents(&gaps, &[], CompletionGateMode::Observe);
        assert!(contents.is_empty());
    }

    #[test]
    fn should_run_audit_increments_and_bounds() {
        let mut session = LongHorizonSessionState::default();
        assert!(should_run_audit(&mut session, 1));
        assert_eq!(session.adversarial_audit_rounds, 1);
        assert!(!should_run_audit(&mut session, 1));
    }
}
