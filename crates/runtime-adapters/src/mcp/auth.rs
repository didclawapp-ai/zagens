//! Remote MCP authentication: config → HTTP headers, with env substitution.
//!
//! Credentials should not live as plaintext in `mcp.json` when avoidable — use
//! `${ENV_VAR}` placeholders (resolved at connection time from the process
//! environment, including values injected by the desktop shell).

use std::collections::HashMap;

use anyhow::{Context, Result};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

use super::config::McpServerConfig;

/// Optional auth block on a remote MCP server (`sse` / `http` transports).
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct McpAuthConfig {
    /// `bearer` or `apiKey` (case-insensitive).
    #[serde(default, rename = "type")]
    pub auth_type: Option<String>,
    /// Bearer token, or full `Bearer <token>` value. Supports `${ENV}`.
    #[serde(default)]
    pub token: Option<String>,
    /// Header name for API-key auth (default `X-API-Key`).
    #[serde(default)]
    pub header: Option<String>,
    /// API key value. Supports `${ENV}`. Accepts `apiKey` alias in JSON.
    #[serde(default, alias = "apiKey")]
    pub api_key: Option<String>,
}

/// Header names treated as sensitive when exporting config to the UI/API.
const SENSITIVE_HEADER_NAMES: &[&str] = &[
    "authorization",
    "x-api-key",
    "api-key",
    "x-auth-token",
    "proxy-authorization",
    "cookie",
];

impl McpServerConfig {
    /// Resolve HTTP headers for remote transports: explicit `headers`, plus
    /// shorthand `auth`, with `${VAR}` / `$VAR` env substitution.
    pub fn resolve_http_headers(&self, server_name: &str) -> Result<HashMap<String, String>> {
        let mut out = HashMap::new();

        if let Some(auth) = &self.auth {
            auth.apply_to_map(&mut out, server_name)?;
        }

        for (name, value) in &self.headers {
            let resolved = resolve_env_placeholders(value)
                .with_context(|| format!("MCP server '{server_name}' header '{name}'"))?;
            out.insert(name.clone(), resolved);
        }

        Ok(out)
    }

    /// Return a copy safe to expose over HTTP APIs (secrets redacted).
    #[must_use]
    pub fn redacted_for_display(&self) -> Self {
        let mut copy = self.clone();
        copy.auth = copy.auth.as_ref().map(McpAuthConfig::redacted);
        copy.headers = redact_header_map(&copy.headers);
        copy
    }
}

impl McpAuthConfig {
    fn apply_to_map(&self, out: &mut HashMap<String, String>, server_name: &str) -> Result<()> {
        let kind = self
            .auth_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("MCP server '{server_name}' auth block requires a 'type' field")
            })?;

        match kind.to_ascii_lowercase().as_str() {
            "bearer" => {
                let token = self.token.as_deref().ok_or_else(|| {
                    anyhow::anyhow!(
                        "MCP server '{server_name}' bearer auth requires a 'token' field"
                    )
                })?;
                let resolved = resolve_env_placeholders(token)
                    .with_context(|| format!("MCP server '{server_name}' bearer token"))?;
                let value = normalize_bearer_value(&resolved);
                out.insert("Authorization".to_string(), value);
            }
            "apikey" | "api_key" | "api-key" => {
                let header = self
                    .header
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("X-API-Key");
                let key = self
                    .api_key
                    .as_deref()
                    .or(self.token.as_deref())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "MCP server '{server_name}' apiKey auth requires 'apiKey' or 'token'"
                        )
                    })?;
                let resolved = resolve_env_placeholders(key)
                    .with_context(|| format!("MCP server '{server_name}' apiKey value"))?;
                out.insert(header.to_string(), resolved);
            }
            other => anyhow::bail!(
                "MCP server '{server_name}' unknown auth type '{other}' (expected bearer or apiKey)"
            ),
        }
        Ok(())
    }

    #[must_use]
    fn redacted(&self) -> Self {
        Self {
            auth_type: self.auth_type.clone(),
            // Plaintext secrets are omitted so API consumers cannot echo them
            // back on PUT; [`merge_preserved_secrets`] restores them on save.
            token: self
                .token
                .as_ref()
                .filter(|t| looks_like_env_placeholder(t))
                .cloned(),
            header: self.header.clone(),
            api_key: self
                .api_key
                .as_ref()
                .filter(|t| looks_like_env_placeholder(t))
                .cloned(),
        }
    }
}

/// When the UI saves a server block after a redacted GET, restore secret
/// fields that were omitted (not re-entered by the user).
pub fn merge_preserved_secrets(new: &mut McpServerConfig, old: &McpServerConfig) {
    match (&mut new.auth, &old.auth) {
        (Some(new_auth), Some(old_auth)) => {
            if new_auth
                .token
                .as_deref()
                .is_none_or(|t| t.trim().is_empty())
            {
                new_auth.token = old_auth.token.clone();
            }
            if new_auth
                .api_key
                .as_deref()
                .is_none_or(|t| t.trim().is_empty())
            {
                new_auth.api_key = old_auth.api_key.clone();
            }
            if new_auth.auth_type.is_none() {
                new_auth.auth_type = old_auth.auth_type.clone();
            }
            if new_auth.header.is_none() {
                new_auth.header = old_auth.header.clone();
            }
        }
        (None, Some(old_auth)) if old_auth.token.is_some() || old_auth.api_key.is_some() => {
            new.auth = Some(old_auth.clone());
        }
        _ => {}
    }

    for (name, value) in &old.headers {
        if is_sensitive_header(name)
            && !new.headers.contains_key(name)
            && !looks_like_env_placeholder(value)
        {
            new.headers.insert(name.clone(), value.clone());
        }
    }
}

/// Substitute `${VAR}` and `$VAR` from the process environment.
pub fn resolve_env_placeholders(raw: &str) -> Result<String> {
    if !raw.contains('$') {
        return Ok(raw.to_string());
    }

    let mut out = String::with_capacity(raw.len());
    let bytes = raw.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'$' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }

        // ${VAR}
        if i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let start = i + 2;
            let mut j = start;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j >= bytes.len() {
                anyhow::bail!("unclosed '${{...}}' in value");
            }
            let name = std::str::from_utf8(&bytes[start..j])
                .context("invalid UTF-8 in env placeholder")?
                .trim();
            if name.is_empty() {
                anyhow::bail!("empty env placeholder '${{}}'");
            }
            let value = std::env::var(name)
                .with_context(|| format!("environment variable '{name}' is not set"))?;
            out.push_str(&value);
            i = j + 1;
            continue;
        }

        // $VAR (identifier: letter/underscore start, then alphanumeric/_)
        let start = i + 1;
        if start >= bytes.len() {
            out.push('$');
            break;
        }
        let first = bytes[start];
        if !(first.is_ascii_alphabetic() || first == b'_') {
            out.push('$');
            i += 1;
            continue;
        }
        let mut j = start + 1;
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_alphanumeric() || b == b'_' {
                j += 1;
            } else {
                break;
            }
        }
        let name =
            std::str::from_utf8(&bytes[start..j]).context("invalid UTF-8 in env placeholder")?;
        let value = std::env::var(name)
            .with_context(|| format!("environment variable '{name}' is not set"))?;
        out.push_str(&value);
        i = j;
    }
    Ok(out)
}

/// Apply resolved headers as reqwest default headers (remote MCP only).
pub fn apply_default_headers(
    builder: reqwest::ClientBuilder,
    headers: &HashMap<String, String>,
) -> Result<reqwest::ClientBuilder> {
    if headers.is_empty() {
        return Ok(builder);
    }
    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("invalid HTTP header name '{name}'"))?;
        let value = HeaderValue::from_str(value)
            .with_context(|| format!("invalid HTTP header value for '{name}'"))?;
        map.insert(name, value);
    }
    Ok(builder.default_headers(map))
}

fn normalize_bearer_value(token: &str) -> String {
    let trimmed = token.trim();
    if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("bearer ") {
        trimmed.to_string()
    } else {
        format!("Bearer {trimmed}")
    }
}

fn redact_header_map(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| {
            if is_sensitive_header(k) && !looks_like_env_placeholder(v) {
                None
            } else {
                Some((k.clone(), v.clone()))
            }
        })
        .collect()
}

fn is_sensitive_header(name: &str) -> bool {
    let lower = name.trim().to_ascii_lowercase();
    SENSITIVE_HEADER_NAMES.iter().any(|s| *s == lower)
}

fn looks_like_env_placeholder(value: &str) -> bool {
    value.contains("${") || value.starts_with('$')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_env_braced() {
        // SAFETY: test-only env mutation; single-threaded `cargo test` harness.
        unsafe {
            std::env::set_var("MCP_TEST_TOKEN", "secret-value");
        }
        assert_eq!(
            resolve_env_placeholders("Bearer ${MCP_TEST_TOKEN}").unwrap(),
            "Bearer secret-value"
        );
        unsafe {
            std::env::remove_var("MCP_TEST_TOKEN");
        }
    }

    #[test]
    fn bearer_auth_adds_prefix() {
        let cfg = McpServerConfig {
            command: None,
            args: vec![],
            env: HashMap::new(),
            url: Some("https://example.com/mcp".to_string()),
            transport: Some("http".to_string()),
            headers: HashMap::new(),
            auth: Some(McpAuthConfig {
                auth_type: Some("bearer".to_string()),
                token: Some("tok123".to_string()),
                header: None,
                api_key: None,
            }),
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: false,
            enabled: true,
            required: false,
            enabled_tools: vec![],
            disabled_tools: vec![],
        };
        let headers = cfg.resolve_http_headers("test").unwrap();
        assert_eq!(
            headers.get("Authorization").map(String::as_str),
            Some("Bearer tok123")
        );
    }

    #[test]
    fn custom_headers_override_auth() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer override".to_string());
        let cfg = McpServerConfig {
            command: None,
            args: vec![],
            env: HashMap::new(),
            url: Some("https://example.com/mcp".to_string()),
            transport: None,
            headers,
            auth: Some(McpAuthConfig {
                auth_type: Some("bearer".to_string()),
                token: Some("from-auth".to_string()),
                header: None,
                api_key: None,
            }),
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: false,
            enabled: true,
            required: false,
            enabled_tools: vec![],
            disabled_tools: vec![],
        };
        let resolved = cfg.resolve_http_headers("test").unwrap();
        assert_eq!(
            resolved.get("Authorization").map(String::as_str),
            Some("Bearer override")
        );
    }
}
