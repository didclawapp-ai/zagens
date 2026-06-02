//! Runtime deliverable manifest merge — workspace overlay + Tauri IPC auto-discovery (P1-2 / P1c).

use std::path::{Path, PathBuf};

use deepseek_core::long_horizon::CompletionGateDeliverableEntry;
use deepseek_config::{CompletionGateDeliverableToml, workspace_meta_dir_read};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct DeliverableOverlayToml {
    #[serde(default)]
    deliverable: Vec<CompletionGateDeliverableToml>,
}

/// Operator manifest entries plus optional workspace overlay and auto-discovered IPC modules.
#[must_use]
pub fn merge_runtime_deliverables(
    workspace: &Path,
    operator: &[CompletionGateDeliverableEntry],
) -> Vec<CompletionGateDeliverableEntry> {
    let mut out: Vec<CompletionGateDeliverableEntry> = operator.to_vec();
    let mut seen: std::collections::HashSet<String> = out
        .iter()
        .filter_map(|e| e.path.as_deref().or(e.glob.as_deref()).map(str::to_string))
        .collect();

    for entry in load_workspace_overlay(workspace) {
        let key = entry
            .path
            .clone()
            .or_else(|| entry.glob.clone())
            .unwrap_or_else(|| entry.id.clone());
        if seen.insert(key) {
            out.push(entry);
        }
    }

    for entry in discover_tauri_command_deliverables(workspace) {
        if seen.insert(entry.id.clone()) {
            out.push(entry);
        }
    }

    for entry in discover_migration_deliverables(workspace) {
        let key = entry
            .path
            .clone()
            .or_else(|| entry.glob.clone())
            .unwrap_or_else(|| entry.id.clone());
        if seen.insert(key) {
            out.push(entry);
        }
    }

    out
}

fn load_workspace_overlay(workspace: &Path) -> Vec<CompletionGateDeliverableEntry> {
    let path = workspace_meta_dir_read(workspace).join("lht-deliverables.toml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    let parsed: DeliverableOverlayToml = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("lht-deliverables.toml parse failed: {e}");
            return Vec::new();
        }
    };
    parsed.deliverable.into_iter().map(toml_deliverable_to_entry).collect()
}

fn toml_deliverable_to_entry(d: CompletionGateDeliverableToml) -> CompletionGateDeliverableEntry {
    CompletionGateDeliverableEntry {
        id: d.id,
        path: d.path,
        glob: d.glob,
        optional_verify_cmd: d.optional_verify_cmd,
        tracked: d.tracked.unwrap_or(false),
    }
}

/// Auto-discover `src-tauri/src/commands/*.rs` and `#[tauri::command]` in lib.rs (P1c+).
fn discover_tauri_command_deliverables(workspace: &Path) -> Vec<CompletionGateDeliverableEntry> {
    let mut out = Vec::new();
    if let Some(commands_dir) = tauri_commands_dir(workspace) {
        let Ok(entries) = std::fs::read_dir(&commands_dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Some(rel) = path.strip_prefix(workspace).ok().map(path_to_slash) else {
                continue;
            };
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("cmd");
            out.push(CompletionGateDeliverableEntry {
                id: format!("ipc_cmd_{stem}"),
                path: Some(rel),
                glob: None,
                optional_verify_cmd: None,
                tracked: false,
            });
        }
    }

    for rel in tauri_rust_sources(workspace) {
        let abs = workspace.join(&rel);
        if !abs.is_file() {
            continue;
        }
        for fn_name in scan_tauri_command_fns(&abs) {
            out.push(CompletionGateDeliverableEntry {
                id: format!("ipc_fn_{fn_name}"),
                path: Some(rel.clone()),
                glob: None,
                optional_verify_cmd: None,
                tracked: false,
            });
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Migration-shaped workspaces: config + adapter globs (layer-3 observe/enforce).
fn discover_migration_deliverables(workspace: &Path) -> Vec<CompletionGateDeliverableEntry> {
    if !workspace.join("src-tauri").join("Cargo.toml").exists() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let conf = "src-tauri/tauri.conf.json";
    if workspace.join(conf).exists() {
        out.push(CompletionGateDeliverableEntry {
            id: "tauri_conf".into(),
            path: Some(conf.into()),
            glob: None,
            optional_verify_cmd: None,
            tracked: false,
        });
    }
    for (id, glob) in [
        ("tauri_adapter", "**/tauri-api.ts"),
        ("desktop_adapter", "**/desktop-api.ts"),
    ] {
        out.push(CompletionGateDeliverableEntry {
            id: id.into(),
            glob: Some(glob.into()),
            path: None,
            optional_verify_cmd: None,
            tracked: false,
        });
    }
    out
}

fn tauri_rust_sources(workspace: &Path) -> Vec<String> {
    ["src-tauri/src/lib.rs", "src-tauri/src/main.rs"]
        .iter()
        .filter(|p| workspace.join(p).is_file())
        .map(|s| (*s).to_string())
        .collect()
}

fn scan_tauri_command_fns(path: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut names = Vec::new();
    for i in 0..lines.len() {
        let line = lines[i].trim();
        if !line.contains("#[tauri::command]") && line != "#[command]" && !line.starts_with("#[command(")
        {
            continue;
        }
        for j in i + 1..lines.len().min(i + 6) {
            if let Some(name) = parse_rust_fn_name(lines[j]) {
                names.push(name);
                break;
            }
        }
    }
    names
}

fn parse_rust_fn_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    for prefix in ["pub async fn ", "async fn ", "pub fn ", "fn "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}

fn tauri_commands_dir(workspace: &Path) -> Option<PathBuf> {
    let candidates = [
        workspace.join("src-tauri/src/commands"),
        workspace.join("src-tauri/src/command"),
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

fn path_to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_tauri_command_modules() {
        let dir = std::env::temp_dir().join(format!("lht-deliv-{}", std::process::id()));
        let cmd_dir = dir.join("src-tauri/src/commands");
        let _ = std::fs::create_dir_all(&cmd_dir);
        std::fs::write(cmd_dir.join("db_erp.rs"), b"// ipc").unwrap();
        std::fs::write(cmd_dir.join("mod.rs"), b"mod x;").unwrap();
        let merged = merge_runtime_deliverables(&dir, &[]);
        assert!(merged.iter().any(|e| e.id == "ipc_cmd_db_erp"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discovers_tauri_command_fns_in_lib_rs() {
        let dir = std::env::temp_dir().join(format!("lht-deliv-lib-{}", std::process::id()));
        let lib = dir.join("src-tauri/src");
        let _ = std::fs::create_dir_all(&lib);
        std::fs::write(
            lib.join("lib.rs"),
            b"#[tauri::command]\npub fn open_file() {}\n#[tauri::command]\npub async fn save_doc() {}\n",
        )
        .unwrap();
        let merged = merge_runtime_deliverables(&dir, &[]);
        assert!(merged.iter().any(|e| e.id == "ipc_fn_open_file"));
        assert!(merged.iter().any(|e| e.id == "ipc_fn_save_doc"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn discovers_migration_deliverables_for_tauri() {
        let dir = std::env::temp_dir().join(format!("lht-deliv-mig-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("src-tauri"));
        std::fs::write(dir.join("src-tauri/Cargo.toml"), b"[package]\n").unwrap();
        std::fs::write(dir.join("src-tauri/tauri.conf.json"), b"{}\n").unwrap();
        let merged = merge_runtime_deliverables(&dir, &[]);
        assert!(merged.iter().any(|e| e.id == "tauri_conf"));
        assert!(merged.iter().any(|e| e.id == "tauri_adapter"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn overlay_merges_without_duplicating_operator() {
        let dir = std::env::temp_dir().join(format!("lht-deliv-ov-{}", std::process::id()));
        let meta = dir.join(".zagens");
        let _ = std::fs::create_dir_all(&meta);
        std::fs::write(
            meta.join("lht-deliverables.toml"),
            r#"
[[deliverable]]
id = "adapter"
path = "src/desktop-api.ts"
"#,
        )
        .unwrap();
        let _ = std::fs::create_dir_all(dir.join("src"));
        std::fs::write(dir.join("src/desktop-api.ts"), b"export {}").unwrap();
        let op = vec![CompletionGateDeliverableEntry {
            id: "adapter".into(),
            path: Some("src/desktop-api.ts".into()),
            glob: None,
            optional_verify_cmd: None,
            tracked: false,
        }];
        let merged = merge_runtime_deliverables(&dir, &op);
        assert_eq!(
            merged
                .iter()
                .filter(|e| e.path.as_deref() == Some("src/desktop-api.ts"))
                .count(),
            1
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
