//! Browser URL allowlist (§6.1 / §6.2 of BUILTIN_BROWSER_PLAN).

use serde::Serialize;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavActor {
    /// Address bar / in-app open — wider https allow.
    Human,
    /// Agent `browser_navigate` — loopback free; external https needs ask/allowlist (caller enforces ask).
    Agent,
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
    let h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    matches!(h.as_str(), "127.0.0.1" | "localhost" | "::1")
}

pub fn validate_navigation(raw: &str, actor: NavActor) -> Result<Url, UrlPolicyError> {
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
        "javascript" | "data" | "file" | "vbscript" => {
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

    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host.is_empty() && scheme != "about" {
        return Err(UrlPolicyError::new("missing_host", "URL 缺少主机名"));
    }

    if is_loopback_host(&host) {
        if scheme == "https" {
            // Allow https://localhost for local Vite/TLS; still loopback.
            return Ok(url);
        }
        return Ok(url);
    }

    // Explicit rejects from §6.2
    if host == "0.0.0.0" {
        return Err(UrlPolicyError::new("host_forbidden", "不允许主机 0.0.0.0"));
    }

    // Private LAN / link-local: default deny (allow_private_lan = false)
    if is_private_or_link_local_host(&host) {
        return Err(UrlPolicyError::new(
            "lan_forbidden",
            format!("默认拒绝非回环局域网主机: {host}"),
        ));
    }

    // External https/http
    if scheme == "http" && !is_loopback_host(&host) {
        return Err(UrlPolicyError::new(
            "cleartext_forbidden",
            "外站仅允许 https（回环除外）",
        ));
    }

    match actor {
        NavActor::Human => Ok(url),
        NavActor::Agent => {
            // Spike / P1: agent external = policy error code for UI to ask; do not auto-allow.
            Err(UrlPolicyError::new(
                "agent_external_needs_ask",
                format!("Agent 打开外站需审批: {host}"),
            ))
        }
    }
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
    // 172.16.0.0 – 172.31.255.255
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some((second, _)) = rest.split_once('.') {
            if let Ok(n) = second.parse::<u8>() {
                return (16..=31).contains(&n);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn human_https_ok_agent_needs_ask() {
        let u = "https://example.com/docs";
        assert!(validate_navigation(u, NavActor::Human).is_ok());
        assert_eq!(
            validate_navigation(u, NavActor::Agent).unwrap_err().code,
            "agent_external_needs_ask"
        );
    }

    #[test]
    fn about_blank_ok() {
        assert!(validate_navigation("about:blank", NavActor::Human).is_ok());
    }
}
