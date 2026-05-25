//! Zagens must not link the TUI `Engine` crate; turns run in the `deepseek-tui` sidecar (P2 spike).

use std::path::Path;

#[test]
fn desktop_cargo_toml_has_no_tui_runtime_library_dependency() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let manifest =
        std::fs::read_to_string(manifest_dir.join("Cargo.toml")).expect("read desktop Cargo.toml");

    for (line_no, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        assert!(
            !trimmed.contains("deepseek-tui"),
            "line {}: desktop must not depend on deepseek-tui as a library (use sidecar binary)",
            line_no + 1
        );
        assert!(
            !trimmed.contains("../tui"),
            "line {}: desktop must not path-depend on crates/tui",
            line_no + 1
        );
    }
}
