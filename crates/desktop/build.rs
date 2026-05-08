//! Ensures `binaries/deepseek-tui-<target>` exists before `tauri-build` validates `externalBin`.
//! Developers: run `npm run bundle:prepare` in this folder for a release sidecar, or build
//! `deepseek-tui` once (`cargo build -p deepseek-tui`) so we can copy from `../../target`.

use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if let Err(e) = ensure_sidecar_binaries() {
        panic!("{e}");
    }
    tauri_build::build();
}

fn ensure_sidecar_binaries() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let triple = std::env::var("TARGET").map_err(|e| e.to_string())?;
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());

    #[cfg(windows)]
    let ext = ".exe";
    #[cfg(not(windows))]
    let ext = "";

    let dest_dir = manifest_dir.join("binaries");
    let dest_name = format!("deepseek-tui-{triple}{ext}");
    let dest = dest_dir.join(&dest_name);

    if dest.exists() {
        return Ok(());
    }

    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    let bin = format!("deepseek-tui{ext}");
    let candidates = [
        manifest_dir
            .join("../../target")
            .join(&profile)
            .join(&bin),
        manifest_dir.join("../../target/release").join(&bin),
        manifest_dir.join("../../target/debug").join(&bin),
    ];

    for src in candidates {
        if src.is_file() {
            fs::copy(&src, &dest).map_err(|e| {
                format!(
                    "failed to copy sidecar from {} to {}: {e}",
                    src.display(),
                    dest.display()
                )
            })?;
            return Ok(());
        }
    }

    Err(format!(
        "missing Tauri sidecar binary at {}\n\
         Fix: run `npm run bundle:prepare` in crates/desktop, or `cargo build -p deepseek-tui` then rebuild desktop.",
        dest.display()
    ))
}
