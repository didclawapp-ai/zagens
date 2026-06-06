//! Shared HTTP client builder helpers (proxy from environment).

use std::env;

use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder, NoProxy, Proxy};

/// Apply `HTTP(S)_PROXY` and `NO_PROXY` from the environment to a reqwest builder.
///
/// Workspace `reqwest` is built with `default-features = false`, so system proxy
/// is not inherited unless we configure it explicitly.
pub fn apply_env_proxy(mut builder: ClientBuilder) -> ClientBuilder {
    let proxy_url = env::var("HTTPS_PROXY")
        .or_else(|_| env::var("https_proxy"))
        .or_else(|_| env::var("HTTP_PROXY"))
        .or_else(|_| env::var("http_proxy"))
        .ok()
        .filter(|s| !s.trim().is_empty());

    let Some(proxy_url) = proxy_url else {
        return builder;
    };

    if let Ok(mut proxy) = Proxy::all(&proxy_url) {
        if let Some(no_proxy) = NoProxy::from_env() {
            proxy = proxy.no_proxy(Some(no_proxy));
        }
        builder = builder.proxy(proxy);
    }
    builder
}

/// Build a reqwest client with environment proxy settings applied.
pub fn build_http_client() -> Result<Client> {
    apply_env_proxy(Client::builder())
        .build()
        .context("failed to build HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_env_proxy_does_not_panic_without_env() {
        let _ = apply_env_proxy(Client::builder());
    }
}
