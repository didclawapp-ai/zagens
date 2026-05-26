#!/usr/bin/env python3
"""Split config/load.rs into config/load/ submodule (D1)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/tui/src/config/load.rs"
FALLBACK = ROOT / "crates/tui/src/_load_restore.rs"
OUT = ROOT / "crates/tui/src/config/load"

# Line ranges from fe7ad5b `config/load.rs` (1-based, inclusive).
RANGES: dict[str, list[tuple[int, int]]] = {
    "impl_config.rs": [(34, 695)],
    "paths.rs": [(697, 944), (1603, 1677)],
    "env_overrides.rs": [(945, 1252)],
    "model.rs": [(1254, 1370)],
    "merge.rs": [(1372, 1601)],
    "credentials.rs": [(1678, 2127)],
}

HEADERS = {
    "impl_config.rs": """use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use toml;

use crate::audit::log_sensitive_event;
use crate::features::{Features, FeaturesToml, is_known_feature_key};
use crate::hooks::HooksConfig;

use super::super::providers::{normalize_model_name, ApiProvider};
use super::super::types::*;
use super::super::{
    API_KEYRING_SENTINEL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEKCN_BASE_URL,
    DEFAULT_FIREWORKS_BASE_URL, DEFAULT_FIREWORKS_MODEL, DEFAULT_MAX_SUBAGENTS,
    DEFAULT_NVIDIA_NIM_BASE_URL, DEFAULT_NVIDIA_NIM_MODEL, DEFAULT_NOVITA_BASE_URL,
    DEFAULT_NOVITA_MODEL, DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL,
    DEFAULT_OPENROUTER_BASE_URL, DEFAULT_OPENROUTER_MODEL, DEFAULT_SGLANG_BASE_URL,
    DEFAULT_SGLANG_MODEL, DEFAULT_TEXT_MODEL, DEFAULT_VLLM_BASE_URL, DEFAULT_VLLM_MODEL,
    MAX_SUBAGENTS,
};
use super::env_overrides::apply_env_overrides;
use super::merge::{apply_managed_overrides, apply_profile, apply_requirements};
use super::model::{
    model_for_provider, normalize_base_url, normalize_model_config, normalize_model_for_provider,
};
use super::paths::{
    default_memory_path, default_mcp_config_path, default_notes_path, default_skills_dir,
    expand_path, resolve_load_config_path,
};

""",
    "paths.rs": """use std::fmt::Write;
use std::fs;
#[cfg(unix)]
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use super::super::types::Config;
use super::super::DEFAULT_TEXT_MODEL;

""",
    "env_overrides.rs": """use std::collections::HashMap;

use super::super::providers::ApiProvider;
use super::super::types::{CapacityConfig, Config, MemoryConfig, ProvidersConfig};
use super::super::{
    API_KEYRING_SENTINEL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEKCN_BASE_URL,
    DEFAULT_FIREWORKS_BASE_URL, DEFAULT_FIREWORKS_MODEL, DEFAULT_MAX_SUBAGENTS,
    DEFAULT_NVIDIA_NIM_BASE_URL, DEFAULT_NVIDIA_NIM_FLASH_MODEL, DEFAULT_NVIDIA_NIM_MODEL,
    DEFAULT_NOVITA_BASE_URL, DEFAULT_NOVITA_FLASH_MODEL, DEFAULT_NOVITA_MODEL,
    DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL, DEFAULT_OPENROUTER_BASE_URL,
    DEFAULT_OPENROUTER_FLASH_MODEL, DEFAULT_OPENROUTER_MODEL, DEFAULT_SGLANG_BASE_URL,
    DEFAULT_SGLANG_FLASH_MODEL, DEFAULT_SGLANG_MODEL, DEFAULT_TEXT_MODEL, DEFAULT_VLLM_BASE_URL,
    DEFAULT_VLLM_FLASH_MODEL, DEFAULT_VLLM_MODEL, MAX_SUBAGENTS,
};
use super::model::{normalize_model_config, parse_http_headers};

""",
    "model.rs": """use std::collections::HashMap;

use anyhow::Result;

use super::super::providers::{canonical_model_name, normalize_model_name, ApiProvider};
use super::super::types::Config;
use super::super::{
    DEFAULT_NVIDIA_NIM_FLASH_MODEL, DEFAULT_NVIDIA_NIM_MODEL, DEFAULT_NOVITA_FLASH_MODEL,
    DEFAULT_NOVITA_MODEL, DEFAULT_OPENROUTER_FLASH_MODEL, DEFAULT_OPENROUTER_MODEL,
    DEFAULT_SGLANG_FLASH_MODEL, DEFAULT_SGLANG_MODEL, DEFAULT_VLLM_FLASH_MODEL, DEFAULT_VLLM_MODEL,
    DEFAULT_FIREWORKS_MODEL,
};

""",
    "merge.rs": """use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml;

use crate::features::{Features, FeaturesToml};

use super::super::providers::ApiProvider;
use super::super::types::{Config, ConfigFile, RequirementsFile, *};
use super::paths::{default_managed_config_path, default_requirements_path, expand_path};

""",
    "credentials.rs": """use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use crate::audit::log_sensitive_event;

use super::super::providers::ApiProvider;
use super::super::types::Config;
use super::super::{API_KEYRING_SENTINEL, DEFAULT_TEXT_MODEL};
use super::paths::{
    default_config_path, ensure_parent_dir, expand_path, home_config_path,
    write_config_file_secure,
};

""",
}


def slice_lines(lines: list[str], start: int, end: int) -> list[str]:
    return lines[start - 1 : end]


def privatize(body: list[str]) -> list[str]:
    out = []
    for line in body:
        if line.startswith("fn ") and not line.startswith("fn is_api_key"):
            out.append("pub(crate) " + line)
        else:
            out.append(line)
    return out


def extract_body(lines: list[str], ranges: list[tuple[int, int]]) -> list[str]:
    body: list[str] = []
    for start, end in ranges:
        body.extend(slice_lines(lines, start, end))
    return privatize(body)


def main() -> None:
    src = SRC if SRC.exists() else FALLBACK
    if not src.exists():
        raise SystemExit(f"missing source: {SRC} or {FALLBACK}")
    lines = src.read_text(encoding="utf-8-sig", errors="replace").splitlines(keepends=True)
    OUT.mkdir(parents=True, exist_ok=True)

    for name, ranges in RANGES.items():
        body = extract_body(lines, ranges)
        hdr = HEADERS.get(name, "")
        (OUT / name).write_text(hdr + "".join(body), encoding="utf-8")

    mod_rs = """//! Config load/merge/credentials (split from legacy `load.rs`).

mod credentials;
mod env_overrides;
mod impl_config;
mod merge;
mod model;
mod paths;

pub use credentials::*;
pub use env_overrides::*;
pub use merge::*;
pub use model::*;
pub use paths::*;

#[cfg(test)]
include!("tests.inc.rs");
"""
    (OUT / "mod.rs").write_text(mod_rs, encoding="utf-8")
    if SRC.exists():
        SRC.unlink()
    print("ok", OUT)


if __name__ == "__main__":
    main()
