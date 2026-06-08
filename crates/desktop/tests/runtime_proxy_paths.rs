//! A+.5 — `runtime_proxy` path allowlist regression (integration crate tests).
//!
//! Unit tests on the real `validate_runtime_path` live in
//! `src/runtime_proxy.rs` (`cargo test -p zagens-desktop runtime_proxy`).
//! This file keeps a mirror contract so `tests/` always runs under CI.

fn validate_like_runtime_proxy(path: &str) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() || !p.starts_with('/') {
        return Err("path 必须以 / 开头".to_string());
    }
    if p.contains("..") {
        return Err("path 不能包含 ..".to_string());
    }
    if !(p.starts_with("/v1/") || p == "/health") {
        return Err("仅允许 /health 与 /v1/* 路径".to_string());
    }
    Ok(())
}

#[test]
fn desktop_runtime_proxy_allowlist_matches_v1_and_health() {
    for ok in [
        "/health",
        "/v1/sessions",
        "/v1/stream",
        "/v1/threads/x/events",
    ] {
        validate_like_runtime_proxy(ok).expect(ok);
    }
    for bad in ["/v0/sessions", "/internal", "/v1/../x"] {
        assert!(
            validate_like_runtime_proxy(bad).is_err(),
            "must reject {bad}"
        );
    }
}
