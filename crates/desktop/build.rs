//! Ensures `binaries/zagens-runtime-<target>` exists before `tauri-build` validates `externalBin`.
//! Developers: run `npm run bundle:prepare` in this folder for a release sidecar, or build
//! `zagens-runtime` once (`cargo build -p zagens-cli`) so we can copy from `../../target`.

use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if let Err(e) = ensure_sidecar_binaries() {
        panic!("{e}");
    }
    if let Err(e) = ensure_resource_stubs() {
        panic!("{e}");
    }
    #[cfg(windows)]
    if let Err(e) = ensure_sandbox_helper_stubs() {
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
    let dest_name = format!("zagens-runtime-{triple}{ext}");
    let dest = dest_dir.join(&dest_name);

    if dest.exists() {
        return Ok(());
    }

    fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;

    let bin = format!("zagens-runtime{ext}");
    let candidates = [
        manifest_dir.join("../../target").join(&profile).join(&bin),
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
         Fix: run `npm run bundle:prepare` in crates/desktop, or `cargo build -p zagens-cli` then rebuild desktop.",
        dest.display()
    ))
}

/// Create empty stub directories for Tauri resource paths that are absent in the
/// source tree (they are gitignored and populated by `npm run bundle:prepare`
/// before a production bundle build).  `tauri-build` only checks that each
/// declared resource *path exists*; empty directories satisfy that check and
/// allow `cargo test` / `cargo clippy` to succeed without the full release
/// artifacts on disk.
fn ensure_resource_stubs() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);

    // Mirrors the `bundle.resources` entries in `tauri.conf.json`.
    let stubs = ["bundle-legal", "binaries/zagens-resources"];

    for rel in stubs {
        let path = manifest_dir.join(rel);
        if !path.exists() {
            fs::create_dir_all(&path)
                .map_err(|e| format!("failed to create resource stub {}: {e}", path.display()))?;
        }
    }

    Ok(())
}

/// Best-effort copy of sandbox helpers into the dev resource stub dir so
/// `cargo build -p zagens-desktop` works without a full `bundle:prepare`.
#[cfg(windows)]
fn ensure_sandbox_helper_stubs() -> Result<(), String> {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
    let resources_dir = manifest_dir.join("binaries/zagens-resources");
    fs::create_dir_all(&resources_dir).map_err(|e| e.to_string())?;

    for name in ["zagens-sandbox-setup.exe", "zagens-command-runner.exe"] {
        let dest = resources_dir.join(name);
        if dest.is_file() {
            continue;
        }
        let candidates = [
            manifest_dir.join("../../target").join(&profile).join(name),
            manifest_dir.join("../../target/release").join(name),
            manifest_dir.join("../../target/debug").join(name),
        ];
        for src in candidates {
            if src.is_file() {
                fs::copy(&src, &dest).map_err(|e| {
                    format!(
                        "failed to copy sandbox helper from {} to {}: {e}",
                        src.display(),
                        dest.display()
                    )
                })?;
                break;
            }
        }
    }
    Ok(())
}
