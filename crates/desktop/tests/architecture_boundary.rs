//! Zagens must not link runtime engine crates; turns run in the `deepseek-runtime` sidecar (D17 I1).

use std::path::Path;

#[test]
fn desktop_cargo_toml_has_no_runtime_library_dependency() {
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
            !trimmed.contains("../runtime-server"),
            "line {}: desktop must not path-depend on crates/runtime-server (use sidecar binary)",
            line_no + 1
        );
        assert!(
            !trimmed.contains("../core"),
            "line {}: desktop must not path-depend on crates/core (use sidecar binary)",
            line_no + 1
        );
        assert!(
            !trimmed.contains("zagens-core"),
            "line {}: desktop must not depend on zagens-core as a library (use sidecar binary)",
            line_no + 1
        );
    }
}
