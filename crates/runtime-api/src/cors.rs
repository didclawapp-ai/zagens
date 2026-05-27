//! Default CORS layer for the runtime HTTP API.

use axum::http::{HeaderValue, Method};
use tower_http::cors::{Any, CorsLayer};

const DEFAULT_CORS_ORIGINS: &[&str] = &[
    "http://localhost:3000",
    "http://127.0.0.1:3000",
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

pub fn cors_layer(extra_origins: &[String]) -> CorsLayer {
    let mut origins: Vec<HeaderValue> = DEFAULT_CORS_ORIGINS
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    for raw in extra_origins {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        match HeaderValue::from_str(trimmed) {
            Ok(value) if !origins.contains(&value) => origins.push(value),
            Ok(_) => {}
            Err(err) => tracing::warn!(
                "Ignoring invalid CORS origin '{trimmed}': {err}; expected scheme://host[:port]"
            ),
        }
    }
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}
