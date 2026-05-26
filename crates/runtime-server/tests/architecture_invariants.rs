//! D15 architecture invariants — prevent legacy CLI/TUI paths from re-entering production code.

use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn read_crate_sources(crate_dir: &str) -> String {
    let root = workspace_root().join(crate_dir);
    let mut out = String::new();
    collect_rs(&root, &mut out);
    out
}

fn collect_rs(dir: &Path, out: &mut String) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if path.components().any(|c| c.as_os_str() == "tests") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                out.push_str(&text);
                out.push('\n');
            }
        }
    }
}

#[test]
fn production_crates_do_not_reference_deepseek_state() {
    for crate_dir in ["runtime-server/src", "desktop/src", "core/src"] {
        let sources = read_crate_sources(crate_dir);
        assert!(
            !sources.contains("deepseek_state"),
            "{crate_dir} must not reference deepseek_state (D15 SSOT: RuntimeThreadStore only)"
        );
        assert!(
            !sources.contains("StateStore"),
            "{crate_dir} must not reference StateStore"
        );
    }
}

#[test]
fn production_crates_do_not_reference_legacy_thread_message_turn_port() {
    for crate_dir in ["runtime-server/src", "desktop/src", "core/src"] {
        let sources = read_crate_sources(crate_dir);
        assert!(
            !sources.contains("ThreadMessageTurnPort"),
            "{crate_dir} must not reference ThreadMessageTurnPort (removed in D15)"
        );
    }
}

#[test]
fn workspace_has_no_state_crate() {
    let state_cargo = workspace_root().join("crates").join("state").join("Cargo.toml");
    assert!(
        !state_cargo.exists(),
        "crates/state must be removed (D15)"
    );
}

#[test]
fn core_cargo_has_no_state_dependency() {
    let manifest =
        std::fs::read_to_string(workspace_root().join("crates/core/Cargo.toml")).expect("core manifest");
    assert!(
        !manifest.contains("deepseek-state"),
        "core must not depend on deepseek-state"
    );
}
