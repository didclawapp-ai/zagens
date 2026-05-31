//! Shared SSRF protection for outbound HTTP tools (`fetch_url`, `web_run`).
//!
//! The host of *every* redirect hop is validated — not just the initial URL.
//! We follow redirects manually with `Policy::none()` so each `Location` target
//! is re-resolved and checked against `is_restricted_ip` (cloud metadata,
//! loopback, link-local, private ranges). A bare `Policy::limited()` would
//! follow a public → `302` → `169.254.169.254` hop with no re-check (C3). The
//! validated IP is pinned per hop to close the DNS-rebinding TOCTOU window, and
//! DNS resolution failures fail closed rather than being handed to reqwest's
//! own resolver.

use std::time::Duration;

use deepseek_runtime_adapters::tools::{check_url_policy, is_http_url, is_restricted_ip};

use crate::tools::spec::{ToolContext, ToolError};

/// Maximum redirect hops we will follow before giving up.
pub(crate) const MAX_REDIRECTS: usize = 5;

/// Read a response body but stop buffering once `max_bytes` have been read, so
/// an unbounded / maliciously huge response can't OOM the runtime (C6). The
/// previous `resp.bytes().await` buffered the *entire* body into memory before
/// any truncation. Returns `(bytes, truncated)` where `truncated` is true if
/// the server had more data than `max_bytes`.
pub(crate) async fn read_body_capped(
    mut resp: reqwest::Response,
    max_bytes: usize,
) -> Result<(Vec<u8>, bool), ToolError> {
    let mut buf: Vec<u8> = Vec::new();
    let mut truncated = false;
    loop {
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| ToolError::execution_failed(format!("failed to read body: {e}")))?;
        let Some(chunk) = chunk else { break };
        let remaining = max_bytes.saturating_sub(buf.len());
        if chunk.len() > remaining {
            buf.extend_from_slice(&chunk[..remaining]);
            truncated = true;
            break;
        }
        buf.extend_from_slice(&chunk);
    }
    Ok((buf, truncated))
}

/// Validate a URL's host for SSRF: enforce network policy, reject `localhost`,
/// reject literal restricted IPs, and for hostnames resolve DNS and reject if
/// *any* resolved address is restricted. Returns `(host, pinned_ip)` so the
/// caller can pin the validated IP.
///
/// **Fails closed on DNS failure / zero addresses.**
pub(crate) async fn validate_url_ssrf(
    context: &ToolContext,
    tool_name: &str,
    url: &str,
) -> Result<Option<(String, std::net::IpAddr)>, ToolError> {
    let host = check_url_policy(context.network_policy.as_ref(), tool_name, url)
        .map_err(|e| ToolError::permission_denied(e.denial_message()))?;
    let Some(host) = host else {
        return Ok(None);
    };
    if host == "localhost" || host == "localhost.localdomain" {
        return Err(ToolError::permission_denied(
            "requests to localhost are not allowed",
        ));
    }
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_restricted_ip(&ip) {
            return Err(ToolError::permission_denied(format!(
                "IP {ip} is a restricted address (private/loopback/link-local)"
            )));
        }
        // Host is already a literal IP — no resolution / pinning needed.
        return Ok(None);
    }
    // Collect into an owned Vec immediately: the iterator returned by
    // `lookup_host` borrows `host`, so we must drop that borrow before moving
    // `host` into the return value below.
    let lookup: std::io::Result<Vec<std::net::SocketAddr>> =
        tokio::net::lookup_host((host.as_str(), 0u16))
            .await
            .map(|addrs| addrs.collect());
    match lookup {
        Ok(addrs) => {
            let mut first_valid: Option<std::net::IpAddr> = None;
            for addr in addrs {
                if is_restricted_ip(&addr.ip()) {
                    return Err(ToolError::permission_denied(format!(
                        "resolved IP {} is a restricted address (private/loopback/link-local)",
                        addr.ip()
                    )));
                }
                if first_valid.is_none() {
                    first_valid = Some(addr.ip());
                }
            }
            match first_valid {
                Some(ip) => Ok(Some((host, ip))),
                None => Err(ToolError::execution_failed(format!(
                    "could not resolve host `{host}` (no addresses)"
                ))),
            }
        }
        Err(e) => Err(ToolError::execution_failed(format!(
            "DNS resolution failed for `{host}`: {e}"
        ))),
    }
}

/// Issue a GET and follow redirects manually, re-validating every hop for SSRF
/// (see [`validate_url_ssrf`]). Each hop gets its own client pinned to the
/// validated IP. Returns the first non-redirect response, or errors after
/// [`MAX_REDIRECTS`].
pub(crate) async fn fetch_with_ssrf_guard(
    context: &ToolContext,
    tool_name: &str,
    initial_url: &str,
    user_agent: &str,
    accept: &str,
    timeout_ms: u64,
) -> Result<reqwest::Response, ToolError> {
    let mut current_url = initial_url.to_string();
    for _ in 0..=MAX_REDIRECTS {
        let pinning = validate_url_ssrf(context, tool_name, &current_url).await?;
        let mut builder = reqwest::Client::builder()
            .timeout(Duration::from_millis(timeout_ms))
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none());
        if let Some((hostname, ip)) = &pinning {
            builder = builder.resolve(hostname, std::net::SocketAddr::new(*ip, 0));
        }
        let client = builder.build().map_err(|e| {
            ToolError::execution_failed(format!("failed to build HTTP client: {e}"))
        })?;

        let resp = client
            .get(&current_url)
            .header("Accept", accept)
            .header("Accept-Language", "en-US,en;q=0.5")
            .send()
            .await
            .map_err(|e| ToolError::execution_failed(format!("request failed: {e}")))?;

        if resp.status().is_redirection() {
            let location = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            if let Some(location) = location {
                let base = reqwest::Url::parse(&current_url).map_err(|e| {
                    ToolError::execution_failed(format!("invalid current URL `{current_url}`: {e}"))
                })?;
                let next = base.join(&location).map_err(|e| {
                    ToolError::execution_failed(format!(
                        "invalid redirect location `{location}`: {e}"
                    ))
                })?;
                if !is_http_url(next.as_str()) {
                    return Err(ToolError::permission_denied(format!(
                        "redirect to non-http(s) scheme blocked: {next}"
                    )));
                }
                current_url = next.to_string();
                continue;
            }
            // 3xx without a usable Location — hand it back as-is.
        }
        return Ok(resp);
    }
    Err(ToolError::execution_failed(format!(
        "too many redirects (>{MAX_REDIRECTS})"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::spec::ToolContext;
    use std::path::PathBuf;

    fn ctx() -> ToolContext {
        ToolContext::new(PathBuf::from("."))
    }

    #[tokio::test]
    async fn rejects_cloud_metadata_ip() {
        // The classic SSRF target — link-local metadata endpoint. Also the hop
        // a malicious 302 would aim at.
        let err = validate_url_ssrf(&ctx(), "fetch_url", "http://169.254.169.254/latest/meta-data/")
            .await
            .expect_err("metadata IP must be rejected");
        assert!(format!("{err}").contains("restricted"));
    }

    #[tokio::test]
    async fn rejects_private_and_loopback_ips() {
        for url in [
            "http://127.0.0.1/",
            "http://10.0.0.5/",
            "http://192.168.1.1/admin",
            "http://[::1]/",
        ] {
            let err = validate_url_ssrf(&ctx(), "fetch_url", url)
                .await
                .unwrap_err();
            assert!(
                format!("{err}").contains("restricted"),
                "{url} should be rejected as restricted"
            );
        }
    }

    #[tokio::test]
    async fn rejects_localhost_hostname() {
        let err = validate_url_ssrf(&ctx(), "fetch_url", "http://localhost:8080/")
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("localhost"));
    }

    #[tokio::test]
    async fn dns_failure_fails_closed() {
        // `.invalid` is reserved (RFC 6761) to never legitimately resolve.
        // Previously an unresolvable host was let through to reqwest's resolver
        // — an SSRF bypass. It must now be a hard reject. (Some resolvers hijack
        // NXDOMAIN to a sinkhole IP, which `is_restricted_ip` then flags — also
        // a rejection, so we accept either path: the invariant is "blocked".)
        let err = validate_url_ssrf(&ctx(), "fetch_url", "http://no-such-host.invalid/")
            .await
            .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("DNS resolution failed")
                || msg.contains("could not resolve")
                || msg.contains("restricted"),
            "unresolvable host must be blocked, got: {msg}"
        );
    }

    #[tokio::test]
    async fn allows_public_literal_ip() {
        // A public literal IP needs no pinning (host already == ip) → Ok(None).
        let res = validate_url_ssrf(&ctx(), "fetch_url", "http://8.8.8.8/")
            .await
            .expect("public IP allowed");
        assert!(res.is_none());
    }
}
