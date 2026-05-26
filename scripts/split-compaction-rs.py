#!/usr/bin/env python3
"""Split compaction.rs into compaction/ submodule (D1)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/tui/src/compaction.rs"
OUT = ROOT / "crates/tui/src/compaction"

# 1-based inclusive line ranges from compaction.rs @ bed5362.
RANGES: dict[str, list[tuple[int, int]]] = {
    "plan.rs": [(78, 501)],
    "tokens.rs": [(502, 668)],
    "prune.rs": [(670, 822)],
    "execute.rs": [(41, 77), (826, 1405)],
    "prompt.rs": [(1407, 1451)],
}

CONST_LINES = (24, 39)  # private consts -> mod.rs

HEADERS = {
    "plan.rs": """use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::logging;
use crate::models::{ContentBlock, Message};

use super::{MAX_WORKING_SET_PATHS, RECENT_WORKING_SET_WINDOW};

""",
    "tokens.rs": """use std::path::Path;

use crate::models::{ContentBlock, Message, SystemPrompt};
use deepseek_core::compaction::CompactionConfig;

use super::plan::plan_compaction;
use super::{KEEP_RECENT_MESSAGES, MIN_SUMMARIZE_MESSAGES};

""",
    "prune.rs": """use std::collections::HashMap;
use std::fmt::Write;

use crate::models::{ContentBlock, Message};

use super::SUMMARY_TOOL_RESULT_SNIPPET_CHARS;

""",
    "execute.rs": """use anyhow::Result;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::DEFAULT_TEXT_MODEL;
use crate::llm_client::LlmClient;
use crate::logging;
use crate::models::{
    CacheControl, ContentBlock, Message, MessageRequest, SystemBlock, SystemPrompt,
    context_window_for_model,
};
use deepseek_core::compaction::CompactionConfig;

use super::plan::plan_compaction;
use super::prune::prune_tool_results;
use super::tokens::{estimate_tokens, should_compact};
use super::prune::{tail_chars, truncate_chars};
use super::{
    CACHE_ALIGNED_SUMMARY_CONTEXT_BUDGET_PERCENT, KEEP_RECENT_MESSAGES,
    LARGE_CONTEXT_SUMMARY_INPUT_HEAD_CHARS, LARGE_CONTEXT_SUMMARY_INPUT_MAX_CHARS,
    LARGE_CONTEXT_SUMMARY_INPUT_TAIL_CHARS, LARGE_CONTEXT_SUMMARY_MAX_TOKENS,
    LARGE_CONTEXT_SUMMARY_TEXT_SNIPPET_CHARS, LARGE_CONTEXT_SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
    LARGE_CONTEXT_WINDOW_TOKENS, SUMMARY_INPUT_HEAD_CHARS, SUMMARY_INPUT_MAX_CHARS,
    SUMMARY_INPUT_TAIL_CHARS, SUMMARY_TEXT_SNIPPET_CHARS, SUMMARY_TOOL_RESULT_SNIPPET_CHARS,
};

""",
    "prompt.rs": """use crate::models::{SystemBlock, SystemPrompt};

""",
}


def slice_lines(lines: list[str], start: int, end: int) -> list[str]:
    return lines[start - 1 : end]


def privatize(body: list[str]) -> list[str]:
    out: list[str] = []
    for line in body:
        if line.startswith("pub "):
            out.append(line)
        elif line.startswith("fn ") or line.startswith("async fn "):
            out.append("pub(crate) " + line)
        elif line.startswith("struct "):
            if any(
                name in line
                for name in (
                    "SummaryInputLimits",
                    "ToolUseInfo",
                    "ToolResultPruneCandidate",
                )
            ):
                out.append(line)
            else:
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
    if not SRC.exists():
        raise SystemExit(f"missing {SRC}")
    lines = SRC.read_text(encoding="utf-8-sig", errors="replace").splitlines(keepends=True)
    OUT.mkdir(parents=True, exist_ok=True)

    const_body = slice_lines(lines, CONST_LINES[0], CONST_LINES[1])
    const_text = "".join(
        line.replace("const ", "pub(crate) const ", 1) for line in const_body
    )

    for name, ranges in RANGES.items():
        body = extract_body(lines, ranges)
        hdr = HEADERS.get(name, "")
        (OUT / name).write_text(hdr + "".join(body), encoding="utf-8")

    # tests: body of `mod tests { ... }`
    test_start = next(i for i, l in enumerate(lines) if l.strip() == "mod tests {") + 1
    test_end = len(lines) - 1  # closing `}` of mod tests
    while test_end > test_start and lines[test_end].strip() != "}":
        test_end -= 1
    tests_body = lines[test_start:test_end]
    tests_hdr = """use super::*;
use crate::models::{ContentBlock, Message, SystemBlock, SystemPrompt};
use serde_json::json;

"""
    # Drop duplicate inner `use` lines from the old `mod tests` wrapper.
    while tests_body and tests_body[0].strip() in {
        "use super::*;",
        "use serde_json::json;",
        "",
    }:
        tests_body.pop(0)
    (OUT / "tests.inc.rs").write_text(tests_hdr + "".join(tests_body), encoding="utf-8")

    mod_rs = (
        "//! Context compaction for long conversations.\n\n"
        "mod execute;\n"
        "mod plan;\n"
        "mod prompt;\n"
        "mod prune;\n"
        "mod tokens;\n\n"
        "pub use deepseek_core::compaction::{CompactionConfig, MINIMUM_AUTO_COMPACTION_TOKENS};\n"
        "pub use execute::{compact_messages, compact_messages_safe, CompactionResult};\n"
        "pub use plan::{CompactionPlan, plan_compaction};\n"
        "pub use prompt::merge_system_prompts;\n"
        "pub use prune::prune_tool_results;\n"
        "pub use tokens::{\n"
        "    estimate_input_tokens_conservative, estimate_text_tokens_deepseek, estimate_tokens,\n"
        "    should_compact,\n"
        "};\n\n"
        "pub const KEEP_RECENT_MESSAGES: usize = 4;\n"
        f"{const_text}\n"
        "#[cfg(test)]\n"
        'include!("tests.inc.rs");\n'
    )
    (OUT / "mod.rs").write_text(mod_rs, encoding="utf-8")
    SRC.unlink()
    print("ok", OUT)


if __name__ == "__main__":
    main()
