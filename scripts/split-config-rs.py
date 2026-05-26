#!/usr/bin/env python3
"""Split crates/tui/src/config.rs into config/ submodule (D1)."""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/tui/src/config.rs"
OUT = ROOT / "crates/tui/src/config"

PROVIDERS = (54, 280)
TYPES = (282, 1097)
LOAD = (1098, 3190)
TESTS_INNER = (3194, None)


def slice_lines(lines: list[str], start: int, end: int | None) -> list[str]:
    return lines[start - 1 : (len(lines) if end is None else end)]


def main() -> None:
    lines = SRC.read_text(encoding="utf-8").splitlines(keepends=True)

    mod_rs = (
        slice_lines(lines, 1, 2)
        + ["\n"]
        + slice_lines(lines, 20, 52)
        + [
            "\n",
            "pub(super) const API_KEYRING_SENTINEL: &str = \"__KEYRING__\";\n",
            "\n",
            "mod load;\n",
            "mod providers;\n",
            "mod types;\n",
            "\n",
            "#[cfg(test)]\n",
            "mod tests;\n",
            "\n",
            "pub use load::*;\n",
            "pub use providers::*;\n",
            "pub use types::*;\n",
        ]
    )

    providers_rs = slice_lines(lines, *PROVIDERS)

    types_rs = (
        [
            "use std::collections::HashMap;\n",
            "use std::path::PathBuf;\n",
            "\n",
            "use crate::features::{Features, FeaturesToml};\n",
            "use crate::hooks::HooksConfig;\n",
            "\n",
            "use super::providers::ApiProvider;\n",
            "\n",
        ]
        + slice_lines(lines, *TYPES)
    )

    load_rs = (
        [
            "use std::collections::HashMap;\n",
            "use std::fmt::Write;\n",
            "use std::fs;\n",
            "#[cfg(unix)]\n",
            "use std::io::Write as _;\n",
            "use std::path::{Path, PathBuf};\n",
            "\n",
            "use anyhow::{Context, Result};\n",
            "use serde::{Deserialize, Serialize};\n",
            "use serde_json::json;\n",
            "#[cfg(unix)]\n",
            "use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};\n",
            "\n",
            "use crate::audit::log_sensitive_event;\n",
            "use crate::features::is_known_feature_key;\n",
            "\n",
            "use super::providers::{canonical_model_name, normalize_model_name, ApiProvider};\n",
            "use super::types::*;\n",
            "use super::API_KEYRING_SENTINEL;\n",
            "\n",
        ]
        + slice_lines(lines, *LOAD)
    )

    tests_rs = slice_lines(lines, *TESTS_INNER)

    OUT.mkdir(parents=True, exist_ok=True)
    for name, body in [
        ("mod.rs", mod_rs),
        ("providers.rs", providers_rs),
        ("types.rs", types_rs),
        ("load.rs", load_rs),
        ("tests.rs", tests_rs),
    ]:
        (OUT / name).write_text("".join(body), encoding="utf-8", newline="\n")

    SRC.unlink()
    print(f"Wrote {OUT}/; removed {SRC.name}")


if __name__ == "__main__":
    main()
