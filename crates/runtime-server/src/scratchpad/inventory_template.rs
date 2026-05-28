//! Default full-repo audit inventory from workspace `Cargo.toml` members.

use std::fs;
use std::path::{Path, PathBuf};

use crate::tools::spec::ToolError;

use super::schema::InventoryArea;
use super::AreaStatus;

const MAX_FILES_PER_AREA: usize = 20;

/// Build inventory rows covering all workspace members (including `runtime-server`).
pub fn workspace_audit_inventory(workspace: &Path) -> Result<Vec<InventoryArea>, ToolError> {
    let members = parse_workspace_members(workspace)?;
    let mut areas = Vec::new();

    for member in members {
        let crate_root = workspace.join(&member);
        if !crate_root.is_dir() {
            continue;
        }
        if member == "crates/desktop" {
            areas.extend(desktop_inventory_areas(&crate_root));
            continue;
        }
        if member == "crates/runtime-server" {
            areas.extend(runtime_server_inventory_areas(&crate_root));
            continue;
        }
        areas.extend(scan_crate_src_areas(&member, &crate_root)?);
    }

    if areas.is_empty() {
        return Err(ToolError::invalid_input(
            "workspace_audit inventory: no source areas found under workspace members",
        ));
    }
    Ok(areas)
}

fn parse_workspace_members(workspace: &Path) -> Result<Vec<String>, ToolError> {
    let cargo_path = workspace.join("Cargo.toml");
    let raw = fs::read_to_string(&cargo_path).map_err(|e| {
        ToolError::execution_failed(format!("failed to read {}: {e}", cargo_path.display()))
    })?;
    let table: toml::Table = toml::from_str(&raw).map_err(|e| {
        ToolError::execution_failed(format!("invalid workspace Cargo.toml: {e}"))
    })?;
    let workspace_table = table
        .get("workspace")
        .and_then(|v| v.as_table())
        .ok_or_else(|| ToolError::invalid_input("Cargo.toml missing [workspace] table"))?;
    let members = workspace_table
        .get("members")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ToolError::invalid_input("Cargo.toml missing workspace.members"))?;
    let mut out = Vec::new();
    for m in members {
        let Some(s) = m.as_str() else {
            continue;
        };
        out.push(s.replace('\\', "/"));
    }
    Ok(out)
}

fn desktop_inventory_areas(crate_root: &Path) -> Vec<InventoryArea> {
    vec![
        area_row(
            "area-desktop",
            "crates/desktop/src",
            "Tauri desktop shell (Rust)",
        ),
        area_row(
            "area-webui-api",
            "crates/desktop/web-ui/src/api",
            "Web UI API layer",
        ),
        area_row(
            "area-webui-types",
            "crates/desktop/web-ui/src/types",
            "Web UI TypeScript types",
        ),
        area_row(
            "area-webui-components",
            "crates/desktop/web-ui/src/components",
            "React components",
        ),
        area_row(
            "area-webui-hooks",
            "crates/desktop/web-ui/src/hooks",
            "React hooks",
        ),
        area_row(
            "area-webui-lib",
            "crates/desktop/web-ui/src/lib",
            "Web UI utilities",
        ),
        area_row(
            "area-webui-i18n",
            "crates/desktop/web-ui/src/i18n",
            "Internationalization",
        ),
    ]
    .into_iter()
    .filter(|a| crate_root.join(&a.path).exists())
    .collect()
}

fn runtime_server_inventory_areas(crate_root: &Path) -> Vec<InventoryArea> {
    let src = crate_root.join("src");
    let subdirs = [
        ("area-runtime-server-core", "core", "Engine core"),
        ("area-runtime-server-tools", "tools", "Tool implementations"),
        (
            "area-runtime-server-runtime-api",
            "runtime_api",
            "HTTP runtime API",
        ),
        ("area-runtime-server-sandbox", "sandbox", "Sandbox backends"),
        ("area-runtime-server-config", "config", "Config loading"),
        (
            "area-runtime-server-runtime-threads",
            "runtime_threads",
            "Thread orchestration",
        ),
        ("area-runtime-server-cli", "cli", "CLI entrypoints"),
    ];
    subdirs
        .into_iter()
        .filter_map(|(id, sub, notes)| {
            let path = format!("crates/runtime-server/src/{sub}");
            if src.join(sub).is_dir() {
                Some(area_row(id, &path, notes))
            } else {
                None
            }
        })
        .collect()
}

fn scan_crate_src_areas(member: &str, crate_root: &Path) -> Result<Vec<InventoryArea>, ToolError> {
    let src = crate_root.join("src");
    if !src.is_dir() {
        return Ok(Vec::new());
    }

    let slug = member
        .trim_start_matches("crates/")
        .replace('/', "-")
        .replace('.', "-");

    let mut subdirs: Vec<PathBuf> = fs::read_dir(&src)
        .map_err(|e| ToolError::execution_failed(format!("read_dir {}: {e}", src.display())))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().ok().is_some_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    subdirs.sort();

    if subdirs.is_empty() {
        let rel = format!("{member}/src");
        return Ok(vec![area_row(
            &format!("area-{slug}"),
            &rel,
            &format!("{member} sources"),
        )]);
    }

    let mut areas = Vec::new();
    for sub in subdirs {
        let name = sub.file_name().and_then(|n| n.to_str()).unwrap_or("src");
        let rel = format!("{member}/src/{name}");
        let count = count_source_files(&sub);
        if count == 0 {
            continue;
        }
        if count <= MAX_FILES_PER_AREA {
            areas.push(area_row(
                &format!("area-{slug}-{name}"),
                &rel,
                &format!("{member}/src/{name} ({count} files)"),
            ));
        } else {
            areas.extend(split_large_area(&slug, &rel, &sub, count)?);
        }
    }
    Ok(areas)
}

fn split_large_area(
    slug: &str,
    rel_base: &str,
    dir: &Path,
    total: usize,
) -> Result<Vec<InventoryArea>, ToolError> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_source_files(dir, &mut files)?;
    files.sort();
    let chunks = files.len().div_ceil(MAX_FILES_PER_AREA);
    let chunk_size = files.len().div_ceil(chunks.max(1));
    let mut areas = Vec::new();
    for (idx, chunk) in files.chunks(chunk_size.max(1)).enumerate() {
        if chunk.is_empty() {
            continue;
        }
        let first = chunk[0]
            .strip_prefix(dir)
            .unwrap_or(&chunk[0])
            .to_string_lossy()
            .replace('\\', "/");
        areas.push(area_row(
            &format!("area-{slug}-part{}", idx + 1),
            rel_base,
            &format!("{rel_base} (~{}/{} files, start {first})", chunk.len(), total),
        ));
    }
    Ok(areas)
}

fn count_source_files(dir: &Path) -> usize {
    let mut files = Vec::new();
    let _ = collect_source_files(dir, &mut files);
    files.len()
}

fn collect_source_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), ToolError> {
    for entry in fs::read_dir(dir)
        .map_err(|e| ToolError::execution_failed(format!("read_dir {}: {e}", dir.display())))?
        .flatten()
    {
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, out)?;
        } else if is_source_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_source_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("rs") | Some("ts") | Some("tsx")
    )
}

fn area_row(id: &str, path: &str, notes: &str) -> InventoryArea {
    InventoryArea {
        id: id.to_string(),
        path: path.to_string(),
        status: AreaStatus::Pending,
        notes: notes.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_audit_inventory_includes_runtime_server() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let areas = workspace_audit_inventory(&root).expect("inventory");
        let ids: Vec<_> = areas.iter().map(|a| a.id.as_str()).collect();
        assert!(
            ids.iter().any(|id| id.starts_with("area-runtime-server")),
            "expected runtime-server areas, got {ids:?}"
        );
        assert!(
            areas.iter().any(|a| a.path.contains("runtime-server")),
            "expected runtime-server paths"
        );
    }
}
