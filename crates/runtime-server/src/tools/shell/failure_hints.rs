//! Post-mortem hints for common shell failures (TS-05 / TS-06).
//!
//! Appended to failed `exec_shell` results when stderr/stdout match known
//! Windows + npm/jest friction patterns seen in long-horizon Node tasks.

use super::types::ShellStatus;
use crate::tools::spec::ToolResult;
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShellFailureHint {
    pub id: &'static str,
    pub message: &'static str,
}

fn combined_output(stdout: &str, stderr: &str) -> String {
    format!("{stderr}\n{stdout}")
}

fn mentions_npm(command: &str, output: &str) -> bool {
    let cmd = command.to_ascii_lowercase();
    let out = output.to_ascii_lowercase();
    cmd.contains("npm") || cmd.contains("npx") || out.contains("npm")
}

fn mentions_jest(command: &str, output: &str) -> bool {
    let cmd = command.to_ascii_lowercase();
    let out = output.to_ascii_lowercase();
    cmd.contains("jest") || out.contains("jest")
}

fn has_eperm_or_eacces(output: &str) -> bool {
    let upper = output.to_ascii_uppercase();
    upper.contains("EPERM") || upper.contains("EACCES")
}

fn has_spawn_failure(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("spawn eperm")
        || lower.contains("error: spawn")
        || lower.contains("errno: -4048")
        || lower.contains("errno -4048")
}

/// Detect actionable hints for a failed shell invocation.
#[must_use]
pub(crate) fn detect_shell_failure_hints(
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    status: &ShellStatus,
) -> Vec<ShellFailureHint> {
    if matches!(status, ShellStatus::Completed | ShellStatus::Running) {
        if exit_code.unwrap_or(0) == 0 {
            return Vec::new();
        }
    }

    let output = combined_output(stdout, stderr);
    let mut hints = Vec::new();

    if mentions_npm(command, &output) && has_eperm_or_eacces(&output) {
        hints.push(ShellFailureHint {
            id: "npm_cache_eperm",
            message: "npm hit EPERM/EACCES on the cache directory (common on Windows or \
                      sandboxed runs). Add a project `.npmrc` with `cache=./.npm-cache`, \
                      retry `npm install`, and avoid writing to the global `%AppData%\\npm-cache`. \
                      Template: `fixtures/harness/workspace-templates/nodejs-windows/.npmrc`.",
        });
    }

    if (mentions_jest(command, &output) || command.to_ascii_lowercase().contains("npm test"))
        && (has_eperm_or_eacces(&output) || has_spawn_failure(&output))
    {
        hints.push(ShellFailureHint {
            id: "jest_run_in_band",
            message: "Jest worker spawn failed (parallel test runners on Windows). Retry with \
                      `npm test -- --runInBand` or `npx jest --runInBand` to run tests in-band \
                      (single process, no worker pool).",
        });
    }

    let cmd_lower = command.to_ascii_lowercase();
    let out_lower = output.to_ascii_lowercase();
    let npm_install = cmd_lower.contains("npm install") || cmd_lower.contains("npm ci");
    if npm_install
        && (out_lower.contains("devdependencies")
            || out_lower.contains("omit=dev")
            || out_lower.contains("omit dev")
            || out_lower.contains("production=true")
            || (out_lower.contains("idealtree") && out_lower.contains("dev")))
    {
        hints.push(ShellFailureHint {
            id: "npm_dev_dependencies",
            message: "npm may have skipped devDependencies (npm v11+ defaults or `--omit=dev`). \
                      Retry with `npm install --include=dev`, or set `omit=` empty in `.npmrc`. \
                      For peer-resolution failures also try `npm install --legacy-peer-deps`.",
        });
    }

    hints
}

fn format_hints_block(hints: &[ShellFailureHint]) -> String {
    hints
        .iter()
        .map(|h| format!("[HINT:{}] {}", h.id, h.message))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Append hint block + metadata when hints were detected.
pub(crate) fn apply_shell_failure_hints(
    result: &mut ToolResult,
    command: &str,
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    status: &ShellStatus,
) {
    let hints = detect_shell_failure_hints(command, stdout, stderr, exit_code, status);
    if hints.is_empty() {
        return;
    }

    let block = format_hints_block(&hints);
    if result.content.is_empty() {
        result.content = block.clone();
    } else {
        result.content = format!("{}\n\n{block}", result.content);
    }

    let hint_ids: Vec<&str> = hints.iter().map(|h| h.id).collect();
    let hint_messages: Vec<&str> = hints.iter().map(|h| h.message).collect();
    match &mut result.metadata {
        Some(meta) => {
            if let Some(obj) = meta.as_object_mut() {
                obj.insert("failure_hints".to_string(), json!(hint_ids));
                obj.insert("failure_hint_messages".to_string(), json!(hint_messages));
            }
        }
        None => {
            result.metadata = Some(json!({
                "failure_hints": hint_ids,
                "failure_hint_messages": hint_messages,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn npm_eperm_emits_cache_hint() {
        let hints = detect_shell_failure_hints(
            "npm install",
            "",
            "npm ERR! code EPERM\nnpm ERR! syscall mkdir",
            Some(1),
            &ShellStatus::Failed,
        );
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].id, "npm_cache_eperm");
        assert!(hints[0].message.contains(".npm-cache"));
    }

    #[test]
    fn jest_spawn_emits_run_in_band_hint() {
        let hints = detect_shell_failure_hints(
            "npm test",
            "",
            "Error: spawn EPERM\n    at ChildProcess.spawn",
            Some(1),
            &ShellStatus::Failed,
        );
        assert!(hints.iter().any(|h| h.id == "jest_run_in_band"));
    }

    #[test]
    fn npm_dev_dependencies_hint_on_omit_dev_output() {
        let hints = detect_shell_failure_hints(
            "npm ci",
            "npm WARN config production Use `--omit=dev` instead.",
            "",
            Some(1),
            &ShellStatus::Failed,
        );
        assert!(hints.iter().any(|h| h.id == "npm_dev_dependencies"));
    }

    #[test]
    fn success_exit_skips_hints() {
        let hints = detect_shell_failure_hints(
            "npm install",
            "added 42 packages",
            "",
            Some(0),
            &ShellStatus::Completed,
        );
        assert!(hints.is_empty());
    }

    #[test]
    fn apply_shell_failure_hints_appends_to_content_and_metadata() {
        let mut result = ToolResult {
            content: "Command failed\n\nSTDERR:\nnpm ERR! EPERM".to_string(),
            success: false,
            metadata: Some(json!({"exit_code": 1})),
        };
        apply_shell_failure_hints(
            &mut result,
            "npm install",
            "",
            "npm ERR! EPERM",
            Some(1),
            &ShellStatus::Failed,
        );
        assert!(result.content.contains("[HINT:npm_cache_eperm]"));
        let meta = result.metadata.expect("metadata");
        assert_eq!(meta["failure_hints"].as_array().map(|a| a.len()), Some(1));
    }
}
