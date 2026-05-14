//! Build and query a symbol index for the workspace.
//!
//! The index is a JSON file at `.deepseek/symbols.json` mapping
//! workspace-relative file paths → list of (kind, name, line).
//!
//! Rebuilt on every `serve --http` start. Missing/unparseable
//! files are skipped silently — the index is best-effort and must
//! never block the runtime from starting.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    /// "struct" | "enum" | "fn" | "trait" | "mod"
    pub kind: String,
    pub name: String,
    /// 1-based line number
    pub line: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbols {
    pub symbols: Vec<SymbolEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub schema_version: u32,
    pub generated_at: String,
    pub files: BTreeMap<String, FileSymbols>,
}

/// Build the index by walking the workspace and parsing every `.rs` file
/// with `syn`. Skipped files (parse errors, non-UTF-8) are omitted without
/// error — the index is best-effort.
pub fn build_index(workspace: &Path) -> SymbolIndex {
    let mut files: BTreeMap<String, FileSymbols> = BTreeMap::new();

    for entry in walk_rs_files(workspace) {
        let rel = match entry.strip_prefix(workspace) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        if let Some(symbols) = extract_symbols(&entry) {
            if !symbols.is_empty() {
                files.insert(rel_str, FileSymbols { symbols });
            }
        }
    }

    SymbolIndex {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
    }
}

/// Query the index for a symbol name (case-insensitive substring match).
/// Returns `(file_path, line)` pairs sorted by exact-match priority.
pub fn query_symbol<'a>(index: &'a SymbolIndex, name: &str) -> Vec<(&'a str, usize)> {
    let name_lower = name.to_lowercase();
    let mut results: Vec<(&str, usize, bool)> = Vec::new();

    for (file, file_syms) in &index.files {
        for sym in &file_syms.symbols {
            let sym_lower = sym.name.to_lowercase();
            if sym_lower == name_lower {
                results.push((file.as_str(), sym.line, true));
            } else if sym_lower.contains(&name_lower) {
                results.push((file.as_str(), sym.line, false));
            }
        }
    }

    // Exact matches first, then substring matches.
    results.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(b.0)));
    results.into_iter().map(|(f, l, _)| (f, l)).collect()
}

/// Format a Markdown summary table for a file from the symbol index.
/// Returns `None` when the file has <500 lines or isn't in the index.
pub fn format_file_summary(
    index: &SymbolIndex,
    file_path: &str,
    total_lines: usize,
) -> Option<String> {
    if total_lines < 500 {
        return None;
    }
    let file_syms = index.files.get(file_path)?;
    if file_syms.symbols.is_empty() {
        return None;
    }

    let mut out = format!(
        "## File Summary: {file_path}\n| Line | Kind | Name |\n|------|------|------|\n"
    );
    for sym in &file_syms.symbols {
        out.push_str(&format!("| {} | {} | `{}` |\n", sym.line, sym.kind, sym.name));
    }
    Some(out)
}

// ── internals ─────────────────────────────────────────────────

fn walk_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_rs_files_impl(root, &mut files);
    files
}

fn walk_rs_files_impl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    // Skip build artifacts and vendored code to keep build fast.
    if name == "target" || name == "node_modules" || name == "dist" || name == "binaries" {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_rs_files_impl(&p, out);
        } else if p.extension().map_or(false, |e| e == "rs") {
            out.push(p);
        }
    }
}

fn extract_symbols(path: &Path) -> Option<Vec<SymbolEntry>> {
    let src = std::fs::read_to_string(path).ok()?;
    let file = syn::parse_file(&src).ok()?;
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    // Build a byte-offset → line-number lookup once.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    for item in &file.items {
        let line = match item {
            syn::Item::Fn(f) => {
                Some((f.sig.ident.to_string(), "fn", f.sig.ident.span().byte_range().start))
            }
            syn::Item::Struct(s) => {
                Some((s.ident.to_string(), "struct", s.ident.span().byte_range().start))
            }
            syn::Item::Enum(e) => {
                Some((e.ident.to_string(), "enum", e.ident.span().byte_range().start))
            }
            syn::Item::Trait(t) => {
                Some((t.ident.to_string(), "trait", t.ident.span().byte_range().start))
            }
            syn::Item::Mod(m) => {
                Some((m.ident.to_string(), "mod", m.ident.span().byte_range().start))
            }
            _ => None,
        };
        if let Some((name, kind, byte_offset)) = line {
            let line_no = line_starts.partition_point(|&start| start <= byte_offset);
            symbols.push(SymbolEntry {
                kind: kind.into(),
                name,
                line: line_no,
            });
        }
    }

    // Dedup by (kind, name) — macros and derives can produce duplicate
    // spans. Keep the first occurrence.
    symbols.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);

    Some(symbols)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_index_extracts_struct_and_fn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let lib_rs = src.join("lib.rs");
        std::fs::write(&lib_rs, "pub struct Foo;\npub fn bar() {}").unwrap();

        let idx = build_index(tmp.path());
        let syms = &idx.files["src/lib.rs"].symbols;
        assert_eq!(syms.len(), 2);
        assert!(syms.iter().any(|s| s.kind == "struct" && s.name == "Foo"));
        assert!(syms.iter().any(|s| s.kind == "fn" && s.name == "bar"));
    }

    #[test]
    fn query_symbol_finds_exact_match() {
        let mut index = SymbolIndex {
            schema_version: 1,
            generated_at: String::new(),
            files: BTreeMap::new(),
        };
        index.files.insert(
            "src/lib.rs".into(),
            FileSymbols {
                symbols: vec![SymbolEntry {
                    kind: "fn".into(),
                    name: "build_router".into(),
                    line: 42,
                }],
            },
        );
        let hits = query_symbol(&index, "build_router");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0], ("src/lib.rs", 42));
    }

    #[test]
    fn walk_skips_target_and_node_modules() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = tmp.path().join("target").join("debug");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("junk.rs"), "//").unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("real.rs"), "pub fn real() {}").unwrap();

        let files = walk_rs_files(tmp.path());
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"real.rs".into()));
        assert!(!names.contains(&"junk.rs".into()));
    }
}
