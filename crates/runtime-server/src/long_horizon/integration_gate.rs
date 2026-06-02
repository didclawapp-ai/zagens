//! Cross-layer integration heuristics (P1-4 / P1d, P1′ shim-aware + enforce gaps).

use std::path::{Path, PathBuf};

use serde::Serialize;

const SCAN_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "vue", "svelte"];
const MAX_FILES: usize = 400;

/// Frontend integration scan counters (Electron→Tauri migration class).
#[derive(Debug, Clone, Default, Serialize)]
pub struct CrossLayerScan {
    pub electron_api_refs: u32,
    pub desktop_api_refs: u32,
    pub tauri_invoke_refs: u32,
    pub files_scanned: u32,
    pub shim_detected: bool,
}

/// Split observe vs enforce integration gaps (80% path).
#[derive(Debug, Clone, Default)]
pub struct CrossLayerGaps {
    pub observe: Vec<String>,
    pub enforce: Vec<String>,
}

/// Legacy entry — observe-only gaps (tests / backward compat).
#[must_use]
pub fn observe_cross_layer_gaps(workspace: &Path) -> Vec<String> {
    evaluate_cross_layer_gaps(workspace).observe
}

/// Evaluate migration-shaped workspaces for cross-layer integration gaps.
#[must_use]
pub fn evaluate_cross_layer_gaps(workspace: &Path) -> CrossLayerGaps {
    if !looks_like_tauri_migration(workspace) {
        return CrossLayerGaps::default();
    }
    let shim = has_electron_api_shim(workspace);
    let scan = scan_cross_layer(workspace);
    let mut observe = Vec::new();
    let mut enforce = Vec::new();

    // High-signal enforce: old Electron tree still on disk (label_rust class).
    if workspace.join("electron").is_dir() {
        enforce.push(
            "legacy `electron/` directory still exists — delete or migrate before declaring done"
                .to_string(),
        );
    }

    if !has_tauri_adapter_file(workspace) {
        enforce.push(
            "missing frontend Tauri adapter (`**/tauri-api.ts` or `**/desktop-api.ts`)"
                .to_string(),
        );
    }

    if !shim {
        if scan.electron_api_refs > 0 && scan.desktop_api_refs == 0 {
            observe.push(format!(
                "frontend has {} `electronAPI` references but no `getDesktopAPI` adapter",
                scan.electron_api_refs
            ));
        }
        if scan.electron_api_refs > scan.tauri_invoke_refs.saturating_add(2)
            && scan.electron_api_refs >= 5
        {
            observe.push(format!(
                "legacy `electronAPI` refs ({}) >> Tauri `invoke(` ({}) — integration likely incomplete",
                scan.electron_api_refs, scan.tauri_invoke_refs
            ));
        }
    }

    CrossLayerGaps { observe, enforce }
}

#[must_use]
pub fn scan_cross_layer(workspace: &Path) -> CrossLayerScan {
    let mut scan = CrossLayerScan {
        shim_detected: has_electron_api_shim(workspace),
        ..CrossLayerScan::default()
    };
    for root in frontend_roots(workspace) {
        scan_dir(&root, &mut scan);
        if scan.files_scanned >= MAX_FILES as u32 {
            break;
        }
    }
    scan
}

/// True when a shim assigns `window.electronAPI` from Tauri (label_rust pattern).
#[must_use]
pub fn has_electron_api_shim(workspace: &Path) -> bool {
    for root in frontend_roots(workspace) {
        if scan_tree_for_shim(&root) {
            return true;
        }
    }
    false
}

fn scan_tree_for_shim(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "dist" | "target" | ".git" | "build") {
                continue;
            }
            if scan_tree_for_shim(&path) {
                return true;
            }
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.contains("tauri-api") && !name.contains("desktop-api") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        if text.contains("electronAPI") && (text.contains("invoke(") || text.contains("@tauri-apps")) {
            return true;
        }
    }
    false
}

#[must_use]
fn has_tauri_adapter_file(workspace: &Path) -> bool {
    for root in frontend_roots(workspace) {
        if find_adapter_in_tree(&root) {
            return true;
        }
    }
    false
}

fn find_adapter_in_tree(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "dist" | "target" | ".git" | "build") {
                continue;
            }
            if find_adapter_in_tree(&path) {
                return true;
            }
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name == "tauri-api.ts" || name == "desktop-api.ts" {
            return true;
        }
    }
    false
}

fn looks_like_tauri_migration(workspace: &Path) -> bool {
    workspace.join("src-tauri").join("Cargo.toml").exists()
        && frontend_roots(workspace).iter().any(|p| p.is_dir())
}

fn frontend_roots(workspace: &Path) -> Vec<PathBuf> {
    ["src", "frontend", "web-ui/src", "app", "client", "resources"]
        .iter()
        .map(|d| workspace.join(d))
        .filter(|p| p.is_dir())
        .collect()
}

fn scan_dir(dir: &Path, scan: &mut CrossLayerScan) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if scan.files_scanned >= MAX_FILES as u32 {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(name, "node_modules" | "dist" | "target" | ".git" | "build") {
                continue;
            }
            scan_dir(&path, scan);
            continue;
        }
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !SCAN_EXTENSIONS.contains(&ext) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        scan.files_scanned += 1;
        scan.electron_api_refs += count_substr(&text, "electronAPI");
        scan.desktop_api_refs += count_substr(&text, "getDesktopAPI");
        scan.tauri_invoke_refs += count_substr(&text, "invoke(");
    }
}

fn count_substr(haystack: &str, needle: &str) -> u32 {
    haystack.match_indices(needle).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tauri_skeleton(dir: &Path, frontend: &str) {
        let fe = dir.join(frontend);
        let _ = std::fs::create_dir_all(fe.join("services"));
        let _ = std::fs::create_dir_all(dir.join("src-tauri"));
        std::fs::write(dir.join("src-tauri/Cargo.toml"), b"[package]\n").unwrap();
    }

    #[test]
    fn enforce_when_electron_dir_remains() {
        let dir = std::env::temp_dir().join(format!("lht-xenf-{}", std::process::id()));
        tauri_skeleton(&dir, "frontend");
        let _ = std::fs::create_dir_all(dir.join("electron"));
        std::fs::write(
            dir.join("frontend/services/tauri-api.ts"),
            b"import { invoke } from '@tauri-apps/api/core'; (window as any).electronAPI = {};",
        )
        .unwrap();
        let gaps = evaluate_cross_layer_gaps(&dir);
        assert!(gaps.enforce.iter().any(|g| g.contains("electron/")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_observe_electronapi_count_when_shim_present() {
        let dir = std::env::temp_dir().join(format!("lht-xshim-{}", std::process::id()));
        tauri_skeleton(&dir, "frontend");
        std::fs::write(
            dir.join("frontend/services/tauri-api.ts"),
            b"import { invoke } from '@tauri-apps/api/core'; window.electronAPI = {};",
        )
        .unwrap();
        std::fs::write(
            dir.join("frontend/App.vue"),
            b"window.electronAPI.open(); window.electronAPI.save();",
        )
        .unwrap();
        let gaps = evaluate_cross_layer_gaps(&dir);
        assert!(gaps.observe.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gaps_when_electron_without_adapter() {
        let dir = std::env::temp_dir().join(format!("lht-xlayer-{}", std::process::id()));
        let frontend = dir.join("src");
        let _ = std::fs::create_dir_all(&frontend);
        let _ = std::fs::create_dir_all(dir.join("src-tauri"));
        std::fs::write(dir.join("src-tauri/Cargo.toml"), b"[package]\n").unwrap();
        std::fs::write(
            frontend.join("app.ts"),
            b"window.electronAPI.open(); window.electronAPI.save();",
        )
        .unwrap();
        let gaps = evaluate_cross_layer_gaps(&dir);
        assert!(!gaps.observe.is_empty() || !gaps.enforce.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
