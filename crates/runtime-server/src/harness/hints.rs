//! Static E3-style failure hints for T1 top-failure tool audit (Phase 2b.3 / shared T3).

#[derive(Debug, Clone)]
pub struct ToolHintAudit {
    pub covered: bool,
    pub summary: Option<String>,
}

struct RegistryEntry {
    tool: &'static str,
    summary: &'static str,
}

const REGISTRY: &[RegistryEntry] = &[
    RegistryEntry {
        tool: "read_file",
        summary: "Confirm path is workspace-relative; widen line range if content looks truncated.",
    },
    RegistryEntry {
        tool: "write_file",
        summary: "Use workspace-relative paths; prefer edit_file/apply_patch for small diffs.",
    },
    RegistryEntry {
        tool: "edit_file",
        summary: "Match exact old_string including whitespace; read_file first to capture context.",
    },
    RegistryEntry {
        tool: "apply_patch",
        summary: "Unified diff must apply cleanly; verify paths and line context with read_file.",
    },
    RegistryEntry {
        tool: "grep_files",
        summary: "Escape regex metacharacters; narrow include filters; follow up with read_file.",
    },
    RegistryEntry {
        tool: "list_dir",
        summary: "Path must exist under workspace; use grep_files when you know a filename fragment.",
    },
    RegistryEntry {
        tool: "load_skill",
        summary: "Skill name must match bundled or user skill dir; check skills list in doctor output.",
    },
    RegistryEntry {
        tool: "assert_file_count",
        summary: "Glob is workspace-relative; counts dirs unless predicate excludes them.",
    },
    RegistryEntry {
        tool: "assert_output_matches",
        summary: "Run the referenced command first; pattern uses predicate::command_output_matches rules.",
    },
    RegistryEntry {
        tool: "assert_tests_pass",
        summary: "Provide a concrete test command; ensure deps installed in workspace.",
    },
    RegistryEntry {
        tool: "run_tests",
        summary: "Scope to affected crates/files; capture stderr for failing test names.",
    },
    RegistryEntry {
        tool: "grep",
        summary: "Legacy alias — prefer grep_files with explicit include filters.",
    },
];

/// Dynamic shell hints are applied at runtime via `failure_hints` metadata.
const DYNAMIC_HINT_TOOLS: &[&str] = &["exec_shell", "wait_for_process"];

pub fn audit_tool(tool_name: &str) -> ToolHintAudit {
    if DYNAMIC_HINT_TOOLS.contains(&tool_name) {
        return ToolHintAudit {
            covered: true,
            summary: Some("dynamic shell failure_hints on non-zero exit".into()),
        };
    }
    REGISTRY
        .iter()
        .find(|e| e.tool == tool_name)
        .map(|e| ToolHintAudit {
            covered: true,
            summary: Some(e.summary.into()),
        })
        .unwrap_or(ToolHintAudit {
            covered: false,
            summary: None,
        })
}

pub fn audit_tools<'a, I>(tools: I) -> (Vec<(String, ToolHintAudit)>, Option<f64>)
where
    I: IntoIterator<Item = &'a str>,
{
    let mut rows = Vec::new();
    for name in tools {
        rows.push((name.to_string(), audit_tool(name)));
    }
    let covered = rows.iter().filter(|(_, a)| a.covered).count();
    let rate = if rows.is_empty() {
        None
    } else {
        Some((covered as f64 / rows.len() as f64 * 100.0 * 100.0).round() / 100.0)
    };
    (rows, rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_shell_counts_as_covered() {
        let audit = audit_tool("exec_shell");
        assert!(audit.covered);
    }

    #[test]
    fn unknown_tool_not_covered() {
        let audit = audit_tool("totally_unknown_tool");
        assert!(!audit.covered);
    }

    #[test]
    fn registry_covers_common_tools() {
        for tool in ["read_file", "grep_files", "load_skill", "assert_file_count"] {
            assert!(audit_tool(tool).covered, "{tool} should have hints");
        }
    }
}
