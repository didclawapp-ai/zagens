//! Read-only `gh` helpers for Diff thin-layer PR list (P4.5 Phase B).
//! Soft-fail oriented: callers get structured error codes instead of panics.
//! Uses `tokio::process` so slow `gh` does not occupy the blocking thread pool.

use std::path::Path;
use std::process::Stdio;

use serde::Deserialize;
use serde_json::Value;
use tokio::process::Command;

const DEFAULT_GH: &str = "gh";
const MAX_PULLS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhListError {
    Missing,
    NotGitRepo,
    Auth,
    Failed(String),
}

impl GhListError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing => "gh_missing",
            Self::NotGitRepo => "not_git_repo",
            Self::Auth => "gh_auth",
            Self::Failed(_) => "gh_failed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Missing => "gh CLI not found. Install GitHub CLI and run `gh auth login`.".into(),
            Self::NotGitRepo => "workspace is not a git repository".into(),
            Self::Auth => "gh is not authenticated. Run `gh auth login`.".into(),
            Self::Failed(m) => m.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PullSummary {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub is_draft: bool,
    pub updated_at: Option<String>,
    /// Aggregate check status: success | pending | failure | neutral | unknown
    pub checks: String,
}

fn gh_bin() -> String {
    std::env::var("DEEPSEEK_GH_BIN").unwrap_or_else(|_| DEFAULT_GH.to_string())
}

fn classify_gh_failure(args: &[&str], stderr: &str) -> GhListError {
    let lower = stderr.to_lowercase();
    if lower.contains("not logged into")
        || lower.contains("auth login")
        || lower.contains("authentication required")
        || lower.contains("http 401")
    {
        return GhListError::Auth;
    }
    if lower.contains("not a git repository") {
        return GhListError::NotGitRepo;
    }
    GhListError::Failed(if stderr.is_empty() {
        format!("gh {} failed", args.join(" "))
    } else {
        stderr.to_string()
    })
}

async fn run_gh_async(workspace: &Path, args: &[&str]) -> Result<String, GhListError> {
    let mut cmd = Command::new(gh_bin());
    cmd.args(args)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = cmd.output().await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            GhListError::Missing
        } else {
            GhListError::Failed(format!("failed to run gh: {e}"))
        }
    })?;

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !output.status.success() {
        return Err(classify_gh_failure(args, &stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List pull requests via `gh pr list --json …` (async, read-only).
pub async fn list_pulls(workspace: &Path, state: &str) -> Result<Vec<PullSummary>, GhListError> {
    let state = match state {
        "closed" | "merged" | "all" => state,
        _ => "open",
    };
    let limit = MAX_PULLS.to_string();
    let text = run_gh_async(
        workspace,
        &[
            "pr",
            "list",
            "--state",
            state,
            "--limit",
            &limit,
            "--json",
            "number,title,url,headRefName,baseRefName,isDraft,statusCheckRollup,updatedAt",
        ],
    )
    .await?;
    parse_pr_list_json(&text)
}

pub fn parse_pr_list_json(text: &str) -> Result<Vec<PullSummary>, GhListError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| GhListError::Failed(format!("gh json parse: {e}")))?;
    let arr = value
        .as_array()
        .ok_or_else(|| GhListError::Failed("gh pr list: expected JSON array".into()))?;
    let mut out = Vec::with_capacity(arr.len().min(MAX_PULLS));
    for item in arr.iter().take(MAX_PULLS) {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            number: u64,
            title: Option<String>,
            url: Option<String>,
            head_ref_name: Option<String>,
            base_ref_name: Option<String>,
            is_draft: Option<bool>,
            updated_at: Option<String>,
            status_check_rollup: Option<Value>,
        }
        let raw: Raw = serde_json::from_value(item.clone())
            .map_err(|e| GhListError::Failed(format!("pr entry: {e}")))?;
        out.push(PullSummary {
            number: raw.number,
            title: raw.title.unwrap_or_default(),
            url: raw.url.unwrap_or_default(),
            head_ref_name: raw.head_ref_name.unwrap_or_default(),
            base_ref_name: raw.base_ref_name.unwrap_or_default(),
            is_draft: raw.is_draft.unwrap_or(false),
            updated_at: raw.updated_at,
            checks: summarize_checks(raw.status_check_rollup.as_ref()),
        });
    }
    Ok(out)
}

/// Reduce `statusCheckRollup` to a single UI token.
pub fn summarize_checks(rollup: Option<&Value>) -> String {
    let Some(v) = rollup else {
        return "unknown".into();
    };
    let entries = match v {
        Value::Array(a) => a.as_slice(),
        Value::Null => return "neutral".into(),
        other => {
            if let Some(s) = other.get("state").and_then(|x| x.as_str()) {
                return normalize_check_state(s);
            }
            if let Some(s) = other.get("conclusion").and_then(|x| x.as_str()) {
                return normalize_check_state(s);
            }
            return "unknown".into();
        }
    };
    if entries.is_empty() {
        return "neutral".into();
    }
    let mut saw_pending = false;
    let mut saw_failure = false;
    let mut saw_success = false;
    for e in entries {
        let state = e
            .get("state")
            .and_then(|x| x.as_str())
            .or_else(|| e.get("conclusion").and_then(|x| x.as_str()))
            .unwrap_or("");
        match normalize_check_state(state).as_str() {
            "failure" => saw_failure = true,
            "pending" => saw_pending = true,
            "success" => saw_success = true,
            _ => {}
        }
    }
    if saw_failure {
        "failure".into()
    } else if saw_pending {
        "pending".into()
    } else if saw_success {
        "success".into()
    } else {
        "neutral".into()
    }
}

fn normalize_check_state(raw: &str) -> String {
    match raw.to_ascii_uppercase().as_str() {
        "SUCCESS" | "COMPLETED" | "PASS" | "PASSED" => "success".into(),
        "FAILURE" | "ERROR" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" => "failure".into(),
        "PENDING" | "QUEUED" | "IN_PROGRESS" | "EXPECTED" | "WAITING" => "pending".into(),
        "NEUTRAL" | "SKIPPED" | "STALE" => "neutral".into(),
        _ => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_pr_list_shapes_entries() {
        let text = r#"[
          {
            "number": 12,
            "title": "Fix diff panel",
            "url": "https://github.com/org/repo/pull/12",
            "headRefName": "feat/diff",
            "baseRefName": "main",
            "isDraft": false,
            "updatedAt": "2026-07-18T00:00:00Z",
            "statusCheckRollup": [
              { "state": "SUCCESS" },
              { "state": "PENDING" }
            ]
          }
        ]"#;
        let pulls = parse_pr_list_json(text).unwrap();
        assert_eq!(pulls.len(), 1);
        assert_eq!(pulls[0].number, 12);
        assert_eq!(pulls[0].checks, "pending");
        assert!(!pulls[0].is_draft);
    }

    #[test]
    fn summarize_checks_failure_wins() {
        let v = json!([{ "conclusion": "SUCCESS" }, { "conclusion": "FAILURE" }]);
        assert_eq!(summarize_checks(Some(&v)), "failure");
    }
}
