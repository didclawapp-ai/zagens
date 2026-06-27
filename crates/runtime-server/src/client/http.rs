use std::collections::HashMap;

use anyhow::Result;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use zagens_config::openai_compatible_api_url;

use crate::logging;

pub(super) const ALLOW_INSECURE_HTTP_ENV: &str = "DEEPSEEK_ALLOW_INSECURE_HTTP";

// === Helpers ===

/// Maximum bytes to read from an error response body (64 KB).
pub(super) const ERROR_BODY_MAX_BYTES: usize = 64 * 1024;

/// Read an error response body with a size limit to prevent unbounded allocation.
pub(super) async fn bounded_error_text(response: reqwest::Response, max_bytes: usize) -> String {
    use futures_util::StreamExt;
    let mut stream = response.bytes_stream();
    let mut buf = Vec::with_capacity(max_bytes.min(8192));
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else { break };
        let remaining = max_bytes.saturating_sub(buf.len());
        if remaining == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

pub(super) fn validate_base_url_security(base_url: &str) -> Result<()> {
    if base_url.starts_with("https://")
        || base_url.starts_with("http://localhost")
        || base_url.starts_with("http://127.0.0.1")
        || base_url.starts_with("http://[::1]")
    {
        return Ok(());
    }

    if base_url.starts_with("http://")
        && std::env::var(ALLOW_INSECURE_HTTP_ENV)
            .ok()
            .as_deref()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    {
        logging::warn(format!(
            "Using insecure HTTP base URL because {} is set",
            ALLOW_INSECURE_HTTP_ENV
        ));
        return Ok(());
    }

    if base_url.starts_with("http://") {
        anyhow::bail!(
            "Refusing insecure base URL '{}'. Use HTTPS or set {}=1 to override for trusted environments.",
            base_url,
            ALLOW_INSECURE_HTTP_ENV
        );
    }

    anyhow::bail!(
        "Refusing base URL '{}': only HTTPS (or explicitly allowed HTTP) URLs are supported.",
        base_url,
    )
}

pub(super) fn api_url(base_url: &str, path: &str) -> String {
    openai_compatible_api_url(base_url, path)
}

// === DeepSeekClient ===

/// Returns true when DEEPSEEK_FORCE_HTTP1 is set to a truthy value
/// (`1`, `true`, `yes`, `on`, case-insensitive). Used by `build_http_client`
/// to opt out of HTTP/2 entirely when DeepSeek's edge mishandles long-lived H2
/// streams (#103). Anything else (unset, `0`, `false`, ...) leaves HTTP/2 on.
pub(super) fn force_http1_from_env() -> bool {
    std::env::var("DEEPSEEK_FORCE_HTTP1")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .is_some_and(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
}

/// Read `SSL_CERT_FILE` and add its contents as extra root
/// certificates on the reqwest builder (#418). Tries the PEM-bundle
/// parser first (covers single-cert files too), then falls back to
/// DER. All failures log a warning and return the builder unchanged
/// so a malformed env var degrades gracefully.
pub(super) fn add_extra_root_certs(
    mut builder: reqwest::ClientBuilder,
    cert_path: &str,
) -> reqwest::ClientBuilder {
    let bytes = match std::fs::read(cert_path) {
        Ok(b) => b,
        Err(err) => {
            logging::warn(format!(
                "SSL_CERT_FILE={cert_path} could not be read: {err}"
            ));
            return builder;
        }
    };

    if let Ok(certs) = reqwest::Certificate::from_pem_bundle(&bytes) {
        let added = certs.len();
        for cert in certs {
            builder = builder.add_root_certificate(cert);
        }
        logging::info(format!(
            "SSL_CERT_FILE={cert_path} loaded ({added} cert(s))"
        ));
        return builder;
    }

    match reqwest::Certificate::from_der(&bytes) {
        Ok(cert) => {
            builder = builder.add_root_certificate(cert);
            logging::info(format!("SSL_CERT_FILE={cert_path} loaded (1 DER cert)"));
        }
        Err(err) => {
            logging::warn(format!(
                "SSL_CERT_FILE={cert_path} could not be parsed as PEM bundle or DER: {err}"
            ));
        }
    }
    builder
}

pub(super) fn build_default_headers(
    api_key: &str,
    extra_headers: &HashMap<String, String>,
) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    if !api_key.trim().is_empty() {
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {api_key}"))?,
        );
    }
    for (name, value) in extra_headers {
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() || value.is_empty() {
            continue;
        }
        let header_name = HeaderName::from_bytes(name.as_bytes())?;
        if header_name == AUTHORIZATION || header_name == CONTENT_TYPE {
            continue;
        }
        headers.insert(header_name, HeaderValue::from_str(value)?);
    }
    Ok(headers)
}
