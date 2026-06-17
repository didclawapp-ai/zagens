//! Tool progress copy and optional JSONL audit logging (P2 PR4 → `zagens-core`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

use serde_json::Value;

const PROGRESS_CMD_PREVIEW_BYTES: usize = 120;

/// Truncate shell command previews without splitting multibyte UTF-8 sequences.
fn truncate_progress_preview(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Append one JSON line to `DEEPSEEK_TOOL_AUDIT_LOG` when the env var is set.
pub fn emit_tool_audit(event: Value) {
    let Some(path) = std::env::var_os("DEEPSEEK_TOOL_AUDIT_LOG") else {
        return;
    };
    let line = match serde_json::to_string(&event) {
        Ok(line) => line,
        Err(_) => return,
    };
    let path = PathBuf::from(path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{line}");
    }
}

/// First progress line shown when a long-running tool starts.
#[must_use]
pub fn tool_progress_opening_line(tool_name: &str, input: &Value) -> String {
    match tool_name {
        "write_file" | "edit_file" | "read_file" => match input.get("path").and_then(Value::as_str)
        {
            Some(p) if !p.is_empty() => format!("{tool_name} → {p}"),
            _ => format!("{tool_name} …"),
        },
        "apply_patch" => "apply_patch → unified diff".to_string(),
        "write_office" => {
            let fmt = input.get("format").and_then(Value::as_str).unwrap_or("");
            match input.get("path").and_then(Value::as_str) {
                Some(p) if !p.is_empty() && !fmt.is_empty() => {
                    format!("write_office ({fmt}) → {p}")
                }
                Some(p) if !p.is_empty() => format!("write_office → {p}"),
                _ => "write_office …".to_string(),
            }
        }
        "exec_shell" => match input.get("command").and_then(Value::as_str) {
            Some(cmd) if cmd.len() > PROGRESS_CMD_PREVIEW_BYTES => {
                format!(
                    "exec_shell: {}…",
                    truncate_progress_preview(cmd, PROGRESS_CMD_PREVIEW_BYTES)
                )
            }
            Some(cmd) if !cmd.is_empty() => format!("exec_shell: {cmd}"),
            _ => "exec_shell …".to_string(),
        },
        "task_shell_start" => match input.get("command").and_then(Value::as_str) {
            Some(cmd) if cmd.len() > PROGRESS_CMD_PREVIEW_BYTES => {
                format!(
                    "task_shell_start: {}…",
                    truncate_progress_preview(cmd, PROGRESS_CMD_PREVIEW_BYTES)
                )
            }
            Some(cmd) if !cmd.is_empty() => format!("task_shell_start: {cmd}"),
            _ => "task_shell_start …".to_string(),
        },
        "task_shell_wait" => match input.get("task_id").and_then(Value::as_str) {
            Some(id) if !id.is_empty() => format!("task_shell_wait → {id}"),
            _ => "task_shell_wait …".to_string(),
        },
        other => format!("{other} …"),
    }
}

#[must_use]
pub fn tool_progress_phase_line(tool_name: &str) -> &'static str {
    match tool_name {
        "write_file" => "Writing file and building diff preview…",
        "edit_file" => "Reading target file and applying replacement…",
        "apply_patch" => "Applying patch hunks to workspace…",
        "read_file" => "Reading from disk…",
        "write_office" => "Generating Office document (may take a few seconds)…",
        "exec_shell" => "Running shell command…",
        "task_shell_start" => "Starting background shell task…",
        "task_shell_wait" => "Collecting task output…",
        _ => "Executing tool…",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    static AUDIT_TEST_GUARD: Mutex<()> = Mutex::new(());

    fn audit_test_guard() -> std::sync::MutexGuard<'static, ()> {
        AUDIT_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn emit_tool_audit_writes_jsonl_line_when_env_var_set() {
        let _g = audit_test_guard();
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("audit.log");
        unsafe {
            std::env::set_var("DEEPSEEK_TOOL_AUDIT_LOG", &path);
        }

        emit_tool_audit(json!({
            "event": "tool.spillover",
            "tool_id": "call-abc",
            "tool_name": "exec_shell",
            "path": "/tmp/foo.txt",
        }));
        emit_tool_audit(json!({
            "event": "tool.result",
            "tool_id": "call-xyz",
            "success": true,
        }));

        let body = std::fs::read_to_string(&path).expect("audit log written");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Value = serde_json::from_str(lines[0]).expect("first line is JSON");
        assert_eq!(
            first.get("event").and_then(|v| v.as_str()),
            Some("tool.spillover")
        );

        unsafe {
            std::env::remove_var("DEEPSEEK_TOOL_AUDIT_LOG");
        }
    }

    #[test]
    fn emit_tool_audit_is_noop_when_env_var_unset() {
        let _g = audit_test_guard();
        unsafe {
            std::env::remove_var("DEEPSEEK_TOOL_AUDIT_LOG");
        }
        emit_tool_audit(json!({"event": "noop", "x": 1}));
    }

    #[test]
    fn tool_progress_opening_line_exec_shell_truncates_at_char_boundary() {
        let prefix = "x".repeat(118);
        let cmd = format!("{prefix}中文");
        assert!(cmd.len() > PROGRESS_CMD_PREVIEW_BYTES);
        assert!(!cmd.is_char_boundary(PROGRESS_CMD_PREVIEW_BYTES));

        let line = tool_progress_opening_line("exec_shell", &json!({ "command": cmd }));
        assert!(line.starts_with("exec_shell: "));
        assert!(line.ends_with('…'));
    }

    #[test]
    fn tool_progress_opening_line_exec_shell_whiteboard_word_count_command() {
        let cmd = concat!(
            "node -e \"const s=require('fs').readFileSync('DESIGN.md','utf8');",
            "const w=s.match(/[\\u4e00-\\u9fff]+/g)||[];",
            "console.log('中文字符:',w.join('').length,'约',Math.round(w.join('').length/2),'字')\""
        );
        assert!(cmd.len() > PROGRESS_CMD_PREVIEW_BYTES);

        let line = tool_progress_opening_line("exec_shell", &json!({ "command": cmd }));
        assert!(line.starts_with("exec_shell: "));
        assert!(line.ends_with('…'));
    }

    #[test]
    fn tool_progress_opening_line_task_shell_start_truncates_at_char_boundary() {
        let prefix = "y".repeat(118);
        let cmd = format!("{prefix}中文");
        assert!(cmd.len() > PROGRESS_CMD_PREVIEW_BYTES);

        let line = tool_progress_opening_line("task_shell_start", &json!({ "command": cmd }));
        assert!(line.starts_with("task_shell_start: "));
        assert!(line.ends_with('…'));
    }
}
