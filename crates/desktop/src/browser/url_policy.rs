//! Browser URL allowlist (§6.1 / §6.2 of BUILTIN_BROWSER_PLAN).

use std::path::Path;

use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavActor {
    /// Address bar / in-app open — wider https allow.
    Human,
    /// Agent `browser_navigate` — loopback free; external https needs ask/allowlist.
    Agent,
}

#[derive(Debug, Clone, Default)]
pub struct NavOpts<'a> {
    /// Session / config host allowlist (lowercase hostnames) for agent external https.
    pub allowlist: &'a [String],
    /// When true, allow private LAN hosts (default false per §6.2).
    pub allow_private_lan: bool,
    /// Workspace root for `file://` canonicalize (required for file navigation).
    pub workspace_root: Option<&'a Path>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UrlPolicyError {
    pub code: String,
    pub message: String,
}

impl UrlPolicyError {
    fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// Hosts allowed for loopback http(s) without further prompt (P1 table, written hard).
pub fn is_loopback_host(host: &str) -> bool {
    let h = normalize_host(host);
    matches!(h.as_str(), "127.0.0.1" | "localhost" | "::1")
}

pub fn normalize_host(host: &str) -> String {
    host.trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase()
}

/// Security badge for UI: `blank` | `loopback` | `external` | `file` | `unknown`.
pub fn security_kind(url_str: &str) -> &'static str {
    let trimmed = url_str.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("about:blank") {
        return "blank";
    }
    match Url::parse(trimmed) {
        Ok(u) => match u.scheme() {
            "file" => "file",
            "about" => "blank",
            _ => {
                let host = u.host_str().unwrap_or("");
                if host.is_empty() {
                    "blank"
                } else if is_loopback_host(host) {
                    "loopback"
                } else {
                    "external"
                }
            }
        },
        Err(_) => "unknown",
    }
}

/// Thin wrapper (kept for call-site clarity / tests).
#[allow(dead_code)]
pub fn validate_navigation(raw: &str, actor: NavActor) -> Result<Url, UrlPolicyError> {
    validate_navigation_with(raw, actor, &NavOpts::default())
}

/// Convenience used by call sites that already built opts elsewhere.
#[inline]
pub fn validate_human_url(raw: &str, opts: &NavOpts<'_>) -> Result<Url, UrlPolicyError> {
    validate_navigation_with(raw, NavActor::Human, opts)
}

pub fn validate_navigation_with(
    raw: &str,
    actor: NavActor,
    opts: &NavOpts<'_>,
) -> Result<Url, UrlPolicyError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(UrlPolicyError::new("empty_url", "URL 为空"));
    }
    let url = Url::parse(trimmed)
        .map_err(|e| UrlPolicyError::new("invalid_url", format!("无法解析 URL: {e}")))?;
    let scheme = url.scheme().to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" => {}
        "about" if url.as_str().eq_ignore_ascii_case("about:blank") => return Ok(url),
        "file" => return validate_file_url(&url, opts),
        "javascript" | "data" | "vbscript" => {
            return Err(UrlPolicyError::new(
                "scheme_forbidden",
                format!("不允许的 URL scheme: {scheme}"),
            ));
        }
        other => {
            return Err(UrlPolicyError::new(
                "scheme_unknown",
                format!("未知 URL scheme: {other}"),
            ));
        }
    }

    let host = url.host_str().map(normalize_host).unwrap_or_default();
    if host.is_empty() && scheme != "about" {
        return Err(UrlPolicyError::new("missing_host", "URL 缺少主机名"));
    }

    if is_loopback_host(&host) {
        return Ok(url);
    }

    // Explicit rejects from §6.2
    if host == "0.0.0.0" {
        return Err(UrlPolicyError::new("host_forbidden", "不允许主机 0.0.0.0"));
    }

    // Private LAN / link-local
    if is_private_or_link_local_host(&host) {
        if !opts.allow_private_lan {
            return Err(UrlPolicyError::new(
                "lan_forbidden",
                format!("默认拒绝非回环局域网主机: {host}"),
            ));
        }
        // Explicitly opened: allow http(s) on private LAN.
        return Ok(url);
    }

    // External https/http
    if scheme == "http" {
        return Err(UrlPolicyError::new(
            "cleartext_forbidden",
            "外站仅允许 https（回环除外）",
        ));
    }

    match actor {
        NavActor::Human => Ok(url),
        NavActor::Agent => {
            if opts.allowlist.iter().any(|h| normalize_host(h) == host) {
                return Ok(url);
            }
            Err(UrlPolicyError::new(
                "agent_external_needs_ask",
                format!(
                    "Agent 打开外站需审批或加入会话 allowlist: {host}（在 Browser 面板「允许当前域名」）"
                ),
            ))
        }
    }
}

fn validate_file_url(url: &Url, opts: &NavOpts<'_>) -> Result<Url, UrlPolicyError> {
    let Some(ws) = opts.workspace_root else {
        return Err(UrlPolicyError::new(
            "file_needs_workspace",
            "file:// 需要已知 workspace 根目录",
        ));
    };
    let path = url
        .to_file_path()
        .map_err(|_| UrlPolicyError::new("file_invalid", "无法将 file:// 转为本地路径"))?;
    let file_canon = std::fs::canonicalize(&path).map_err(|e| {
        UrlPolicyError::new(
            "file_missing",
            format!("文件不存在或无法解析 {}: {e}", path.display()),
        )
    })?;
    let ws_canon = std::fs::canonicalize(ws).map_err(|e| {
        UrlPolicyError::new(
            "workspace_invalid",
            format!("无法解析 workspace {}: {e}", ws.display()),
        )
    })?;
    if !path_within(&file_canon, &ws_canon) {
        return Err(UrlPolicyError::new(
            "file_escape",
            format!(
                "file:// 路径超出 workspace: {} (root={})",
                file_canon.display(),
                ws_canon.display()
            ),
        ));
    }
    Url::from_file_path(&file_canon).map_err(|_| {
        UrlPolicyError::new(
            "file_invalid",
            format!("无法重新编码 file:// {}", file_canon.display()),
        )
    })
}

fn path_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn is_private_or_link_local_host(host: &str) -> bool {
    if host.starts_with("10.")
        || host.starts_with("192.168.")
        || host.starts_with("169.254.")
        || host.starts_with("fc")
        || host.starts_with("fd")
        || host.starts_with("fe80:")
    {
        return true;
    }
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some((second, _)) = rest.split_once('.') {
            if let Ok(n) = second.parse::<u8>() {
                return (16..=31).contains(&n);
            }
        }
    }
    false
}

/// Build a `file://` URL for a workspace-relative path (UI helper).
#[allow(dead_code)]
pub fn workspace_file_url(workspace: &Path, rel: &str) -> Result<Url, UrlPolicyError> {
    let joined = workspace.join(rel.trim_start_matches(['/', '\\']));
    let opts = NavOpts {
        workspace_root: Some(workspace),
        ..Default::default()
    };
    let as_url = Url::from_file_path(&joined).map_err(|_| {
        UrlPolicyError::new("file_invalid", format!("无法编码路径 {}", joined.display()))
    })?;
    validate_file_url(&as_url, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn loopback_http_ok() {
        for u in [
            "http://127.0.0.1:5173/",
            "http://localhost:3000",
            "http://[::1]:8080/",
        ] {
            assert!(validate_navigation(u, NavActor::Human).is_ok(), "{u}");
            assert!(validate_navigation(u, NavActor::Agent).is_ok(), "{u}");
        }
    }

    #[test]
    fn rejects_javascript_and_zero() {
        assert_eq!(
            validate_navigation("javascript:alert(1)", NavActor::Human)
                .unwrap_err()
                .code,
            "scheme_forbidden"
        );
        assert_eq!(
            validate_navigation("http://0.0.0.0/", NavActor::Human)
                .unwrap_err()
                .code,
            "host_forbidden"
        );
    }

    #[test]
    fn rejects_lan_by_default() {
        assert_eq!(
            validate_navigation("http://192.168.1.1/", NavActor::Human)
                .unwrap_err()
                .code,
            "lan_forbidden"
        );
    }

    #[test]
    fn lan_ok_when_flag() {
        let opts = NavOpts {
            allow_private_lan: true,
            ..Default::default()
        };
        assert!(validate_navigation_with("http://192.168.1.1/", NavActor::Human, &opts).is_ok());
    }

    #[test]
    fn human_https_ok_agent_needs_ask() {
        let u = "https://example.com/docs";
        assert!(validate_navigation(u, NavActor::Human).is_ok());
        assert_eq!(
            validate_navigation(u, NavActor::Agent).unwrap_err().code,
            "agent_external_needs_ask"
        );
    }

    #[test]
    fn agent_external_ok_with_allowlist() {
        let allow = vec!["example.com".into()];
        let opts = NavOpts {
            allowlist: &allow,
            ..Default::default()
        };
        assert!(
            validate_navigation_with("https://example.com/docs", NavActor::Agent, &opts).is_ok()
        );
    }

    #[test]
    fn about_blank_ok() {
        assert!(validate_navigation("about:blank", NavActor::Human).is_ok());
    }

    #[test]
    fn security_kinds() {
        assert_eq!(security_kind("about:blank"), "blank");
        assert_eq!(security_kind("http://127.0.0.1:5173/"), "loopback");
        assert_eq!(security_kind("https://example.com"), "external");
        assert_eq!(security_kind("file:///C:/tmp/a.html"), "file");
    }

    #[test]
    fn file_within_workspace_ok() {
        let dir =
            std::env::temp_dir().join(format!("zagens-browser-file-ok-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let file = dir.join("page.html");
        fs::write(&file, "<html></html>").unwrap();
        let url = Url::from_file_path(&file).unwrap();
        let opts = NavOpts {
            workspace_root: Some(dir.as_path()),
            ..Default::default()
        };
        assert!(validate_navigation_with(url.as_str(), NavActor::Human, &opts).is_ok());
        assert!(validate_navigation_with(url.as_str(), NavActor::Agent, &opts).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn file_escape_rejected() {
        let dir =
            std::env::temp_dir().join(format!("zagens-browser-file-ws-{}", std::process::id()));
        let outside =
            std::env::temp_dir().join(format!("zagens-browser-file-escape-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let _ = fs::create_dir_all(&outside);
        let file = outside.join("secret.html");
        fs::write(&file, "x").unwrap();
        let url = Url::from_file_path(&file).unwrap();
        let opts = NavOpts {
            workspace_root: Some(dir.as_path()),
            ..Default::default()
        };
        assert_eq!(
            validate_navigation_with(url.as_str(), NavActor::Human, &opts)
                .unwrap_err()
                .code,
            "file_escape"
        );
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn file_without_workspace_rejected() {
        assert_eq!(
            validate_navigation("file:///C:/Windows/notepad.exe", NavActor::Human)
                .unwrap_err()
                .code,
            "file_needs_workspace"
        );
    }
}
