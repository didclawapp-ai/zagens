//! Daily Agent Explore / Edit / Verify / Ship phase catalog policy.
//!
//! Soft deferral: tools remain discoverable via tool_search, but the eager set
//! tracks the inferred phase so the model sees fewer irrelevant tools each turn.

use std::collections::HashSet;

use crate::chat::Tool;
use crate::engine::tool_catalog::is_tool_search_tool;
use crate::turn::TurnLoopMode;

/// Coarse agent work phase inferred from recent successful tool calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentToolPhase {
    #[default]
    Explore,
    Edit,
    Verify,
    Ship,
}

impl AgentToolPhase {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Explore => "explore",
            Self::Edit => "edit",
            Self::Verify => "verify",
            Self::Ship => "ship",
        }
    }

    /// Advance phase based on a successful tool name.
    #[must_use]
    pub fn advance(self, tool_name: &str) -> Self {
        if matches!(
            tool_name,
            "edit_file"
                | "write_file"
                | "apply_patch"
                | "batch_edit"
                | "edit_and_check"
                | "change_and_verify"
                | "fim_edit"
                | "refactor_imports"
        ) {
            return Self::Edit;
        }
        if matches!(
            tool_name,
            "run_tests"
                | "task_gate_run"
                | "assert_tests_pass"
                | "assert_file_count"
                | "assert_output_matches"
                | "diagnostics"
                | "exec_shell"
        ) {
            // Shell may be exploratory; only bump Explore→Verify when already editing.
            return match self {
                Self::Explore => Self::Explore,
                Self::Edit | Self::Verify | Self::Ship => Self::Verify,
            };
        }
        if matches!(
            tool_name,
            "github_comment"
                | "github_close_issue"
                | "pr_attempt_record"
                | "pr_attempt_preflight"
                | "update_plan"
        ) {
            return Self::Ship;
        }
        if matches!(
            tool_name,
            "read_file"
                | "grep_files"
                | "glob_files"
                | "file_search"
                | "explore_codebase"
                | "investigate"
                | "answer_from_repo"
                | "list_dir"
                | "project_map"
                | "git_status"
                | "git_diff"
                | "git_log"
                | "git_show"
                | "git_blame"
        ) {
            return match self {
                Self::Verify | Self::Ship => self,
                _ => Self::Explore,
            };
        }
        self
    }
}

/// Tools that should be eager in the given phase (in addition to always-eager UX tools).
#[must_use]
pub fn phase_bonus_eager_tools(phase: AgentToolPhase) -> &'static [&'static str] {
    match phase {
        AgentToolPhase::Explore => &[
            "investigate",
            "answer_from_repo",
            "explore_codebase",
            "grep_files",
            "glob_files",
            "read_file",
            "file_search",
            "project_map",
            "git_status",
            "git_diff",
            "list_dir",
            "file_info",
        ],
        AgentToolPhase::Edit => &[
            "edit_file",
            "write_file",
            "apply_patch",
            "change_and_verify",
            "edit_and_check",
            "read_file",
            "grep_files",
            "investigate",
            "promote_to_context",
        ],
        AgentToolPhase::Verify => &[
            "run_tests",
            "task_gate_run",
            "diagnostics",
            "exec_shell",
            "exec_shell_wait",
            "change_and_verify",
            "assert_tests_pass",
            "promote_to_context",
            "read_file",
            "grep_files",
        ],
        AgentToolPhase::Ship => &[
            "update_plan",
            "checklist_write",
            "todo_write",
            "github_pr_context",
            "github_issue_context",
            "pr_attempt_list",
            "pr_attempt_read",
            "read_file",
            "git_status",
            "git_diff",
        ],
    }
}

/// UX / discovery tools that stay on the native deferral policy (not phase-shrunk).
fn phase_always_eager(name: &str) -> bool {
    is_tool_search_tool(name)
        || matches!(
            name,
            "request_user_input"
                | "load_skill"
                | "multi_tool_use.parallel"
                | "note"
                | "remember"
                | "describe_image"
                | "scratchpad_status"
                | "scratchpad_init"
                | "scratchpad_append"
                | "scratchpad_set_area"
                | "scratchpad_list_notes"
                | "rlm"
                | "recall_archive"
        )
}

/// Union of all phase-bonus names plus closely related primitives that should
/// follow phase policy even if omitted from a specific bonus list.
fn phase_managed_tool_names() -> HashSet<&'static str> {
    let mut set = HashSet::new();
    for phase in [
        AgentToolPhase::Explore,
        AgentToolPhase::Edit,
        AgentToolPhase::Verify,
        AgentToolPhase::Ship,
    ] {
        set.extend(phase_bonus_eager_tools(phase).iter().copied());
    }
    for extra in [
        "write_file",
        "edit_file",
        "apply_patch",
        "batch_edit",
        "fim_edit",
        "refactor_imports",
        "exec_shell",
        "exec_shell_wait",
        "exec_shell_interact",
        "exec_wait",
        "exec_interact",
        "run_tests",
        "diagnostics",
        "assert_tests_pass",
        "assert_file_count",
        "assert_output_matches",
        "task_gate_run",
        "github_comment",
        "github_close_issue",
        "pr_attempt_record",
        "pr_attempt_preflight",
        "explore_codebase",
        "investigate",
        "answer_from_repo",
        "edit_and_check",
        "change_and_verify",
        "promote_to_context",
        "project_map",
        "git_status",
        "git_diff",
        "git_log",
        "git_show",
        "git_blame",
        "grep_files",
        "glob_files",
        "file_search",
        "read_file",
        "list_dir",
        "file_info",
        "update_plan",
        "checklist_write",
        "todo_write",
        "github_pr_context",
        "github_issue_context",
        "pr_attempt_list",
        "pr_attempt_read",
    ] {
        set.insert(extra);
    }
    set
}

/// Failure-driven catalog hot-start (complements phase inference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailureHotStart {
    #[default]
    None,
    /// Compile / typecheck / cargo errors → diagnostics + verify tools.
    CompileDiagnostics,
    /// Zero matches / missing symbol / not found → explore/search tools.
    SymbolMissing,
}

impl FailureHotStart {
    /// Infer hot-start from a tool name + result body (success or failure).
    #[must_use]
    pub fn infer(tool_name: &str, result: &str, success: bool) -> Self {
        let lower = result.to_ascii_lowercase();
        // Prefer structured LSP diagnostics (`ERROR [12:8] msg`) over rustc-only
        // `error[E…]` patterns so non-Rust languages still hot-start verify tools.
        let lsp_error = result.contains("ERROR [")
            || result.contains("\nERROR [")
            || (lower.contains("<diagnostics") && lower.contains("error"));
        let compile_ish = lsp_error
            || lower.contains("error[e")
            || lower.contains("cannot find")
            || lower.contains("unresolved import")
            || lower.contains("type mismatch")
            || lower.contains("undefined reference")
            || lower.contains("compilation failed")
            || lower.contains("cargo check")
            || lower.contains("tsc:")
            || (lower.contains("error:")
                && (tool_name.contains("shell")
                    || tool_name == "run_tests"
                    || tool_name == "diagnostics"
                    || tool_name == "change_and_verify"
                    || tool_name == "edit_and_check"));
        if compile_ish {
            return Self::CompileDiagnostics;
        }
        let missing = !success
            || lower.contains("not found")
            || lower.contains("matched 0")
            || lower.contains("0 matches")
            || lower.contains("no files matched")
            || lower.contains("uncertainty=not_found")
            || (lower.contains("symbol `") && lower.contains("not found"));
        if missing
            && matches!(
                tool_name,
                "grep_files"
                    | "glob_files"
                    | "file_search"
                    | "read_file"
                    | "investigate"
                    | "explore_codebase"
                    | "project_map"
            )
        {
            return Self::SymbolMissing;
        }
        Self::None
    }

    #[must_use]
    pub fn bonus_tools(self) -> &'static [&'static str] {
        match self {
            Self::None => &[],
            Self::CompileDiagnostics => &[
                "diagnostics",
                "exec_shell",
                "run_tests",
                "change_and_verify",
                "edit_and_check",
                "read_file",
                "grep_files",
            ],
            Self::SymbolMissing => &[
                "investigate",
                "answer_from_repo",
                "explore_codebase",
                "grep_files",
                "glob_files",
                "file_search",
                "read_file",
                "project_map",
            ],
        }
    }
}

/// Apply soft phase policy: eager phase-bonus tools; defer other phase-managed tools.
///
/// Does not remove tools; only flips `defer_loading` so tool_search can still activate them.
/// Always-eager UX tools keep whatever native deferral already set.
#[must_use]
pub fn apply_agent_phase_catalog(
    tools: Vec<Tool>,
    phase: AgentToolPhase,
    mode: TurnLoopMode,
) -> Vec<Tool> {
    apply_agent_phase_catalog_with_hot_start(tools, phase, mode, FailureHotStart::None)
}

/// Like [`apply_agent_phase_catalog`] but unions failure hot-start tools into the eager set.
#[must_use]
pub fn apply_agent_phase_catalog_with_hot_start(
    mut tools: Vec<Tool>,
    phase: AgentToolPhase,
    mode: TurnLoopMode,
    hot_start: FailureHotStart,
) -> Vec<Tool> {
    if mode != TurnLoopMode::Agent {
        return tools;
    }
    let mut bonus: HashSet<&str> = phase_bonus_eager_tools(phase).iter().copied().collect();
    bonus.extend(hot_start.bonus_tools().iter().copied());
    let managed = phase_managed_tool_names();
    for tool in &mut tools {
        let name = tool.name.as_str();
        if phase_always_eager(name) {
            continue;
        }
        if bonus.contains(name) {
            tool.defer_loading = Some(false);
        } else if managed.contains(name) {
            tool.defer_loading = Some(true);
        }
    }
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::Tool;

    fn tool(name: &str, defer: bool) -> Tool {
        Tool {
            tool_type: None,
            name: name.to_string(),
            description: String::new(),
            input_schema: serde_json::json!({}),
            allowed_callers: None,
            defer_loading: Some(defer),
            input_examples: None,
            strict: None,
            cache_control: None,
        }
    }

    #[test]
    fn edit_advances_to_edit_phase() {
        assert_eq!(
            AgentToolPhase::Explore.advance("edit_file"),
            AgentToolPhase::Edit
        );
    }

    #[test]
    fn verify_after_edit() {
        let phase = AgentToolPhase::Edit.advance("run_tests");
        assert_eq!(phase, AgentToolPhase::Verify);
    }

    #[test]
    fn shell_does_not_force_verify_from_explore() {
        assert_eq!(
            AgentToolPhase::Explore.advance("exec_shell"),
            AgentToolPhase::Explore
        );
    }

    #[test]
    fn explore_phase_defers_edit_tools() {
        let catalog = vec![
            tool("investigate", true),
            tool("edit_file", false),
            tool("request_user_input", false),
        ];
        let out = apply_agent_phase_catalog(catalog, AgentToolPhase::Explore, TurnLoopMode::Agent);
        assert_eq!(
            out.iter()
                .find(|t| t.name == "investigate")
                .unwrap()
                .defer_loading,
            Some(false)
        );
        assert_eq!(
            out.iter()
                .find(|t| t.name == "edit_file")
                .unwrap()
                .defer_loading,
            Some(true)
        );
        assert_eq!(
            out.iter()
                .find(|t| t.name == "request_user_input")
                .unwrap()
                .defer_loading,
            Some(false)
        );
    }

    #[test]
    fn yolo_mode_skips_phase_policy() {
        let catalog = vec![tool("edit_file", false)];
        let out = apply_agent_phase_catalog(catalog, AgentToolPhase::Explore, TurnLoopMode::Yolo);
        assert_eq!(out[0].defer_loading, Some(false));
    }

    #[test]
    fn ship_advances_on_preflight() {
        assert_eq!(
            AgentToolPhase::Verify.advance("pr_attempt_preflight"),
            AgentToolPhase::Ship
        );
    }

    #[test]
    fn hot_start_prefers_lsp_error_block() {
        let hint = FailureHotStart::infer(
            "edit_file",
            "Applied edit.\nERROR [12:8] expected `;`, found `}`",
            true,
        );
        assert_eq!(hint, FailureHotStart::CompileDiagnostics);
    }

    #[test]
    fn hot_start_eager_diagnostics_on_compile_fail() {
        let hint = FailureHotStart::infer(
            "exec_shell",
            "error[E0425]: cannot find value `foo` in this scope",
            false,
        );
        assert_eq!(hint, FailureHotStart::CompileDiagnostics);
        let catalog = vec![
            tool("diagnostics", true),
            tool("investigate", true),
            tool("edit_file", false),
        ];
        let out = apply_agent_phase_catalog_with_hot_start(
            catalog,
            AgentToolPhase::Explore,
            TurnLoopMode::Agent,
            hint,
        );
        assert_eq!(
            out.iter()
                .find(|t| t.name == "diagnostics")
                .unwrap()
                .defer_loading,
            Some(false)
        );
    }
}
