//! Git diff summary for the workspace.

use std::path::Path;
use std::process::Command;

pub fn git_diff_stat(workspace: &Path) -> Vec<String> {
    let output = Command::new("git")
        .args(["diff", "--stat", "--no-color"])
        .current_dir(workspace)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let lines: Vec<String> = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(ToString::to_string)
                .collect();
            if lines.is_empty() {
                vec!["(clean working tree)".to_string()]
            } else {
                lines
            }
        }
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            vec![format!("git diff failed: {}", err.trim())]
        }
        Err(e) => vec![format!("git not available: {e}")],
    }
}
