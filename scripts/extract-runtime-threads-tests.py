#!/usr/bin/env python3
"""Extract runtime_threads #[cfg(test)] mod tests to tests.rs (A4.6 stage 3)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MOD = ROOT / "crates/tui/src/runtime_threads/mod.rs"
TESTS = ROOT / "crates/tui/src/runtime_threads/tests.rs"

MARKER_START = "#[cfg(test)]\nmod tests {"
MARKER_END = "\n}\n"  # last closing brace of mod tests — file ends with tests module


def main() -> None:
    text = MOD.read_text(encoding="utf-8")
    start = text.find(MARKER_START)
    if start < 0:
        raise SystemExit(f"marker not found in {MOD}")
    # tests module runs to EOF (last `}` closes mod tests)
    body = text[start + len("#[cfg(test)]\nmod tests {") :]
    if not body.rstrip().endswith("}"):
        raise SystemExit("unexpected tests module shape")
    body = body.rstrip()[:-1]  # drop closing `}`

    tests_src = (
        "//! Runtime thread manager contract tests (R-003 A4.6).\n\n"
        + body.lstrip("\n")
    )
    TESTS.write_text(tests_src, encoding="utf-8", newline="\n")

    new_mod = text[:start].rstrip() + '\n\n#[path = "tests.rs"]\n#[cfg(test)]\nmod tests;\n'
    MOD.write_text(new_mod, encoding="utf-8", newline="\n")
    print(f"Wrote {TESTS} ({TESTS.read_text(encoding='utf-8').count(chr(10))} lines)")
    print(f"Updated {MOD} ({new_mod.count(chr(10))} lines)")


if __name__ == "__main__":
    main()
