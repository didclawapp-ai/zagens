#!/usr/bin/env python3
"""Split client.rs into client/ submodule (D1)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/tui/src/client.rs"
OUT = ROOT / "crates/tui/src/client"

RANGES: dict[str, list[tuple[int, int]]] = {
    "tool_names.rs": [(23, 110)],
    "types.rs": [(112, 303)],
    "http.rs": [(304, 446), (519, 544)],
    "client_impl.rs": [(448, 517), (546, 678), (684, 699)],
    "llm.rs": [(701, 743)],
    "api_parse.rs": [(745, 926)],
    "fim.rs": [(928, 961)],
}

HEADERS = {
    "tool_names.rs": """use regex::Regex;
use std::sync::OnceLock;

""",
    "types.rs": """use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{ApiProvider, RetryPolicy};
use tokio::sync::Mutex as AsyncMutex;

""",
    "http.rs": """use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};

use crate::logging;

pub(super) const ALLOW_INSECURE_HTTP_ENV: &str = "DEEPSEEK_ALLOW_INSECURE_HTTP";

""",
    "client_impl.rs": """use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::header::HeaderMap;

use crate::config::Config;
use crate::llm_client::{
    LlmError, RetryConfig as LlmRetryConfig, extract_retry_after, with_retry,
};
use crate::logging;

use super::api_parse::parse_models_response;
use super::http::{
    ERROR_BODY_MAX_BYTES, add_extra_root_certs, api_url, bounded_error_text, build_default_headers,
    force_http1_from_env, validate_base_url_security,
};
use super::types::{
    AvailableModel, ConnectionHealth, DeepSeekClient, TokenBucket,
    apply_request_failure, apply_request_success, mark_recovery_probe_if_due,
};

""",
    "llm.rs": """use anyhow::Result;

use crate::llm_client::LlmClient;
use crate::models::MessageRequest;

use super::http::api_url;
use super::types::DeepSeekClient;

""",
    "api_parse.rs": """use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::config::ApiProvider;
use crate::models::{ServerToolUsage, SystemPrompt, Usage};

use super::types::AvailableModel;

""",
    "fim.rs": """use anyhow::{Context, Result};
use serde_json::json;

use super::http::{ERROR_BODY_MAX_BYTES, api_url, bounded_error_text};
use super::types::DeepSeekClient;

""",
}


def slice_lines(lines: list[str], start: int, end: int) -> list[str]:
    return lines[start - 1 : end]


def extract_body(lines: list[str], ranges: list[tuple[int, int]]) -> list[str]:
    body: list[str] = []
    for start, end in ranges:
        body.extend(slice_lines(lines, start, end))
    return body


def main() -> None:
    if not SRC.exists():
        raise SystemExit(f"missing {SRC}")
    lines = SRC.read_text(encoding="utf-8-sig", errors="replace").splitlines(keepends=True)
    OUT.mkdir(parents=True, exist_ok=True)

    for name, ranges in RANGES.items():
        body = extract_body(lines, ranges)
        hdr = HEADERS.get(name, "")
        (OUT / name).write_text(hdr + "".join(body), encoding="utf-8")

    test_start = next(i for i, l in enumerate(lines) if l.strip() == "mod tests {") + 1
    test_end = len(lines) - 1
    while test_end > test_start and lines[test_end].strip() != "}":
        test_end -= 1
    tests_body = lines[test_start:test_end]
    while tests_body and tests_body[0].strip() in {"use super::*;", ""}:
        tests_body.pop(0)

    tests_hdr = """use super::*;
use crate::client::chat::{
    build_chat_messages, build_chat_messages_for_request, count_reasoning_replay_chars,
    parse_chat_message, parse_sse_chunk, sanitize_thinking_mode_messages, tool_to_chat,
};
use crate::models::{
    ContentBlock, ContentBlockStart, Delta, Message, MessageRequest, StreamEvent, Tool,
};
use serde_json::json;

"""
    (OUT / "tests.inc.rs").write_text(tests_hdr + "".join(tests_body), encoding="utf-8")

    mod_rs = """//! HTTP client for DeepSeek's OpenAI-compatible Chat Completions API.
//!
//! DeepSeek documents `/chat/completions` as the primary endpoint, and this
//! client now routes all normal traffic through that surface.

mod api_parse;
mod chat;
mod client_impl;
mod fim;
mod http;
mod llm;
mod tool_names;
mod types;

pub use types::{AvailableModel, DeepSeekClient};

#[cfg(test)]
include!("tests.inc.rs");
"""
    (OUT / "mod.rs").write_text(mod_rs, encoding="utf-8")
    SRC.unlink()
    print("ok", OUT)


if __name__ == "__main__":
    main()
