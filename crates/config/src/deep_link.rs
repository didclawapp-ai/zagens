//! `zagens://` deep link parsing and workspace validation.
//!
//! Supported form:
//! `zagens://open?workspace=<path>&prompt=<urlencoded>&task_type=code|office|auto&use_worktree=1`

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::paths::user_scoped_workspace;

pub const DEEP_LINK_SCHEME: &str = "zagens";
pub const DEEP_LINK_OPEN_HOST: &str = "open";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepLinkOpen {
    pub workspace: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_type: Option<String>,
    /// When true, the desktop should enable the new-session worktree toggle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_worktree: Option<bool>,
}

impl DeepLinkOpen {
    #[must_use]
    pub fn workspace_display(&self) -> String {
        self.workspace.display().to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeepLinkError {
    #[error("empty URL")]
    Empty,
    #[error("expected scheme `{DEEP_LINK_SCHEME}://`")]
    BadScheme,
    #[error("expected host `{DEEP_LINK_OPEN_HOST}`")]
    BadHost,
    #[error("missing required query parameter `workspace`")]
    MissingWorkspace,
    #[error("invalid workspace path: {0}")]
    InvalidWorkspace(String),
    #[error("invalid task_type `{0}` (expected code, office, or auto)")]
    InvalidTaskType(String),
    #[error("malformed URL: {0}")]
    Malformed(String),
}

/// Parse `zagens://open?...` and validate the workspace path.
pub fn parse_open_url(raw: &str) -> Result<DeepLinkOpen, DeepLinkError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DeepLinkError::Empty);
    }
    let rest = trimmed
        .strip_prefix(&format!("{DEEP_LINK_SCHEME}://"))
        .ok_or(DeepLinkError::BadScheme)?;
    let (authority_and_path, query) = match rest.split_once('?') {
        Some((head, q)) => (head, q),
        None => (rest, ""),
    };
    let host = authority_and_path
        .trim_end_matches('/')
        .split('/')
        .next()
        .unwrap_or("");
    if host != DEEP_LINK_OPEN_HOST {
        return Err(DeepLinkError::BadHost);
    }

    let params = parse_query(query);
    let workspace_raw = params
        .get("workspace")
        .ok_or(DeepLinkError::MissingWorkspace)?;
    let workspace = validate_workspace_path(workspace_raw)?;

    let prompt = params
        .get("prompt")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    let task_type = match params.get("task_type").map(String::as_str) {
        None => None,
        Some("") => None,
        Some("code" | "office" | "auto") => params.get("task_type").cloned(),
        Some(other) => return Err(DeepLinkError::InvalidTaskType(other.to_string())),
    };

    let use_worktree = params
        .get("use_worktree")
        .map(|s| matches!(s.as_str(), "1" | "true" | "yes" | "on"));

    Ok(DeepLinkOpen {
        workspace,
        prompt,
        task_type,
        use_worktree,
    })
}

/// Build a canonical open URL for the given workspace (prompt/task_type optional).
#[must_use]
pub fn build_open_url(
    workspace: &Path,
    prompt: Option<&str>,
    task_type: Option<&str>,
    use_worktree: bool,
) -> String {
    let mut url = format!(
        "{DEEP_LINK_SCHEME}://{DEEP_LINK_OPEN_HOST}?workspace={}",
        encode_query_component(&workspace.display().to_string())
    );
    if let Some(p) = prompt.filter(|s| !s.trim().is_empty()) {
        url.push_str("&prompt=");
        url.push_str(&encode_query_component(p));
    }
    if let Some(t) = task_type.filter(|s| matches!(*s, "code" | "office" | "auto")) {
        url.push_str("&task_type=");
        url.push_str(t);
    }
    if use_worktree {
        url.push_str("&use_worktree=1");
    }
    url
}

/// Scan argv for the first parseable `zagens://open?...` URL.
pub fn find_open_url_in_args<'a, I>(args: I) -> Option<DeepLinkOpen>
where
    I: IntoIterator<Item = &'a str>,
{
    for arg in args {
        let trimmed = arg.trim();
        if trimmed.starts_with(&format!("{DEEP_LINK_SCHEME}://"))
            && let Ok(link) = parse_open_url(trimmed)
        {
            return Some(link);
        }
    }
    None
}

fn validate_workspace_path(raw: &str) -> Result<PathBuf, DeepLinkError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DeepLinkError::MissingWorkspace);
    }
    let path = PathBuf::from(trimmed);
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(DeepLinkError::InvalidWorkspace(
            "path must not contain `..`".to_string(),
        ));
    }
    user_scoped_workspace(trimmed).map_err(DeepLinkError::InvalidWorkspace)
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        if key.is_empty() {
            continue;
        }
        out.insert(percent_decode(key), percent_decode(value));
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(v) =
                u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn encode_query_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_open_url() {
        let dir = test_workspace_under_home();
        let url = build_open_url(&dir, None, None, false);
        let parsed = parse_open_url(&url).unwrap();
        assert_eq!(
            parsed.workspace.canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_prompt_and_task_type() {
        let dir = test_workspace_under_home();
        let url = build_open_url(&dir, Some("fix CI"), Some("code"), false);
        let parsed = parse_open_url(&url).unwrap();
        assert_eq!(parsed.prompt.as_deref(), Some("fix CI"));
        assert_eq!(parsed.task_type.as_deref(), Some("code"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_parent_dir_escape() {
        let err = parse_open_url("zagens://open?workspace=/tmp/../etc").unwrap_err();
        assert!(matches!(err, DeepLinkError::InvalidWorkspace(_)));
    }

    #[test]
    fn parse_use_worktree_flag() {
        let dir = test_workspace_under_home();
        let url = build_open_url(&dir, None, None, true);
        let parsed = parse_open_url(&url).unwrap();
        assert_eq!(parsed.use_worktree, Some(true));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_bad_host() {
        let err = parse_open_url("zagens://run?workspace=/tmp").unwrap_err();
        assert!(matches!(err, DeepLinkError::BadHost));
    }

    #[test]
    fn find_in_args() {
        let dir = test_workspace_under_home();
        let url = build_open_url(&dir, Some("hi"), None, false);
        let found = find_open_url_in_args(["zagens.exe", url.as_str()]);
        assert!(found.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Linux CI temp dir is `/tmp`, outside [`user_scoped_workspace`] allowed roots.
    fn test_workspace_under_home() -> PathBuf {
        let home = dirs::home_dir().expect("home directory");
        let dir = home.join(format!("zagens-dl-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
