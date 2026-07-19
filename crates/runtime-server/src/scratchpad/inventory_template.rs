//! Default full-repo audit inventory from workspace `Cargo.toml` members.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::tools::spec::ToolError;

use super::AreaStatus;
use super::schema::InventoryArea;

/// Raised from 20 → 40 so core/adapters split less aggressively (target ~15–25 areas).
const MAX_FILES_PER_AREA: usize = 40;
/// Mark area `high_complexity` when symbol index reports this many fn/impl_fn under the path.
const HIGH_COMPLEXITY_FN_THRESHOLD: usize = 200;

/// Workspace-relative path prefixes that must appear in a `workspace_audit` inventory.
///
/// Keep in sync with skill D1 must-hit crates; update when members are renamed/removed.
pub const MUST_HIT_PATHS: &[&str] = &[
    "crates/desktop",
    "crates/runtime-server",
    "crates/secrets",
    "crates/windows-sandbox",
];

#[derive(Debug, Deserialize)]
struct AuditAreaDecl {
    id: String,
    path: String,
    #[serde(default)]
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuditPackageMeta {
    #[serde(default)]
    areas: Vec<AuditAreaDecl>,
    #[serde(default)]
    must_hit: bool,
}

/// Build inventory rows covering all workspace members (including `runtime-server`).
pub fn workspace_audit_inventory(workspace: &Path) -> Result<Vec<InventoryArea>, ToolError> {
    let members = parse_workspace_members(workspace)?;
    let mut areas = Vec::new();
    let symbol_index = try_load_symbol_index(workspace);

    for member in members {
        let crate_root = workspace.join(&member);
        if !crate_root.is_dir() {
            continue;
        }
        if let Some(meta) = load_audit_metadata(&crate_root)? {
            if meta.areas.is_empty() {
                return Err(ToolError::invalid_input(format!(
                    "workspace_audit: {member} has [package.metadata.zagens.audit] but empty areas"
                )));
            }
            areas.extend(areas_from_metadata(&member, &crate_root, &meta)?);
            continue;
        }
        areas.extend(scan_crate_src_areas(&member, &crate_root)?);
    }

    if areas.is_empty() {
        return Err(ToolError::invalid_input(
            "workspace_audit inventory: no source areas found under workspace members",
        ));
    }

    if let Some(index) = symbol_index.as_ref() {
        apply_high_complexity(index, &mut areas);
    }

    ensure_must_hit_coverage(&areas)?;
    Ok(areas)
}

/// Fail loud when a must-hit crate prefix has no inventory area (silent `exists()` drops).
pub fn ensure_must_hit_coverage(areas: &[InventoryArea]) -> Result<(), ToolError> {
    let missing: Vec<&str> = MUST_HIT_PATHS
        .iter()
        .copied()
        .filter(|prefix| !areas.iter().any(|a| path_covers_prefix(&a.path, prefix)))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(ToolError::invalid_input(format!(
        "workspace_audit inventory contract failed: missing must-hit path coverage for {missing:?}. \
         Each of {MUST_HIT_PATHS:?} must have at least one area path equal to the prefix or nested under it."
    )))
}

fn path_covers_prefix(path: &str, prefix: &str) -> bool {
    let path = path.replace('\\', "/");
    path == prefix || path.starts_with(&format!("{prefix}/"))
}

fn parse_workspace_members(workspace: &Path) -> Result<Vec<String>, ToolError> {
    let cargo_path = workspace.join("Cargo.toml");
    let raw = fs::read_to_string(&cargo_path).map_err(|e| {
        ToolError::execution_failed(format!("failed to read {}: {e}", cargo_path.display()))
    })?;
    let table: toml::Table = toml::from_str(&raw)
        .map_err(|e| ToolError::execution_failed(format!("invalid workspace Cargo.toml: {e}")))?;
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

fn load_audit_metadata(crate_root: &Path) -> Result<Option<AuditPackageMeta>, ToolError> {
    let cargo_path = crate_root.join("Cargo.toml");
    let Ok(raw) = fs::read_to_string(&cargo_path) else {
        return Ok(None);
    };
    let table: toml::Table = match toml::from_str(&raw) {
        Ok(t) => t,
        Err(e) => {
            return Err(ToolError::execution_failed(format!(
                "invalid {}: {e}",
                cargo_path.display()
            )));
        }
    };
    let Some(audit) = table
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("zagens"))
        .and_then(|z| z.get("audit"))
    else {
        return Ok(None);
    };
    let meta: AuditPackageMeta = audit.clone().try_into().map_err(|e| {
        ToolError::invalid_input(format!(
            "invalid [package.metadata.zagens.audit] in {}: {e}",
            cargo_path.display()
        ))
    })?;
    let _ = meta.must_hit; // reserved for future cross-check with MUST_HIT_PATHS
    Ok(Some(meta))
}

fn areas_from_metadata(
    member: &str,
    crate_root: &Path,
    meta: &AuditPackageMeta,
) -> Result<Vec<InventoryArea>, ToolError> {
    let mut areas = Vec::new();
    for decl in &meta.areas {
        let local = decl
            .path
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        let abs = crate_root.join(&local);
        if !abs.exists() {
            return Err(ToolError::invalid_input(format!(
                "workspace_audit metadata area '{}' path '{}' does not exist under {member}",
                decl.id, decl.path
            )));
        }
        let workspace_path = format!("{member}/{local}");
        let notes = decl
            .label
            .clone()
            .unwrap_or_else(|| format!("{member}/{local}"));
        areas.push(area_row(&decl.id, &workspace_path, &notes));
    }
    Ok(areas)
}

fn scan_crate_src_areas(member: &str, crate_root: &Path) -> Result<Vec<InventoryArea>, ToolError> {
    let src = crate_root.join("src");
    if !src.is_dir() {
        return Ok(Vec::new());
    }

    let slug = member
        .trim_start_matches("crates/")
        .replace(['/', '.'], "-");

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
            &format!(
                "{rel_base} (~{}/{} files, start {first})",
                chunk.len(),
                total
            ),
        ));
    }
    Ok(areas)
}

fn try_load_symbol_index(workspace: &Path) -> Option<crate::symbol_index::SymbolIndex> {
    let index_path = zagens_config::workspace_meta_file_read(workspace, "symbols.json");
    let raw = fs::read_to_string(index_path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn apply_high_complexity(index: &crate::symbol_index::SymbolIndex, areas: &mut [InventoryArea]) {
    for area in areas.iter_mut() {
        let prefix = area.path.replace('\\', "/");
        let fn_count = index
            .files
            .iter()
            .filter(|(file, _)| {
                let f = file.replace('\\', "/");
                f == prefix || f.starts_with(&format!("{prefix}/"))
            })
            .flat_map(|(_, fs)| fs.symbols.iter())
            .filter(|s| s.kind == "fn" || s.kind == "impl_fn")
            .count();
        if fn_count >= HIGH_COMPLEXITY_FN_THRESHOLD {
            area.high_complexity = true;
            if !area.notes.contains("high_complexity") {
                area.notes = format!(
                    "{} — high_complexity (~{fn_count} fn/impl_fn; deeper P1 review)",
                    area.notes
                );
            }
        }
    }
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
        high_complexity: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_audit_inventory_covers_must_hit_and_desktop_webui() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let areas = workspace_audit_inventory(&root).expect("inventory");
        let ids: Vec<_> = areas.iter().map(|a| a.id.as_str()).collect();

        for prefix in MUST_HIT_PATHS {
            assert!(
                areas.iter().any(|a| path_covers_prefix(&a.path, prefix)),
                "must-hit {prefix} missing; ids={ids:?}"
            );
        }
        assert!(
            ids.contains(&"area-desktop"),
            "expected area-desktop, got {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id.starts_with("area-webui-")),
            "expected area-webui-* areas, got {ids:?}"
        );
        assert!(
            ids.iter().any(|id| id.starts_with("area-runtime-server")),
            "expected runtime-server areas, got {ids:?}"
        );
        // Metadata-driven: no silent double-join drop; area count stays in skill band.
        assert!(
            (10..=40).contains(&areas.len()),
            "expected 10–40 areas, got {}",
            areas.len()
        );
    }

    #[test]
    fn ensure_must_hit_fails_when_desktop_missing() {
        let areas = vec![
            area_row(
                "area-runtime-server-core",
                "crates/runtime-server/src/core",
                "",
            ),
            area_row("area-secrets", "crates/secrets/src", ""),
            area_row("area-windows-sandbox", "crates/windows-sandbox/src", ""),
        ];
        let err = ensure_must_hit_coverage(&areas).expect_err("desktop missing");
        let msg = err.to_string();
        assert!(msg.contains("crates/desktop"), "{msg}");
        assert!(msg.contains("must-hit"), "{msg}");
    }

    #[test]
    fn path_covers_prefix_exact_and_nested() {
        assert!(path_covers_prefix("crates/desktop", "crates/desktop"));
        assert!(path_covers_prefix("crates/desktop/src", "crates/desktop"));
        assert!(!path_covers_prefix(
            "crates/desktop-extra/src",
            "crates/desktop"
        ));
        assert!(!path_covers_prefix("crates/desk", "crates/desktop"));
    }

    #[test]
    fn metadata_missing_path_fails_loud() {
        let dir = tempfile::tempdir().expect("tmp");
        let crate_root = dir.path().join("crates/fake");
        fs::create_dir_all(crate_root.join("src")).expect("mkdir");
        fs::write(
            crate_root.join("Cargo.toml"),
            r#"
[package]
name = "fake"
version = "0.0.0"

[package.metadata.zagens.audit]
must_hit = true
areas = [
  { id = "area-missing", path = "web-ui/src/missing", label = "gone" },
]
"#,
        )
        .expect("toml");
        let meta = load_audit_metadata(&crate_root)
            .expect("load")
            .expect("meta");
        let err = areas_from_metadata("crates/fake", &crate_root, &meta).expect_err("missing path");
        assert!(err.to_string().contains("does not exist"), "{err}");
    }
}
