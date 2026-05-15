//! Build and query a symbol index for the workspace.
//!
//! The index is a JSON file at `.deepseek/symbols.json` mapping
//! workspace-relative file paths → list of (kind, name, line).
//!
//! Rebuilt on every `serve --http` start. Supports incremental rebuild
//! by comparing file mtimes. Missing/unparseable files are skipped
//! silently — the index is best-effort and must never block the runtime.

#![allow(clippy::too_many_lines)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// ── Public types ──────────────────────────────────────────────

/// The granularity of symbol visibility to include in the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolVisibility {
    /// Only `pub` symbols (default).
    Public,
    /// Include private symbols within workspace crates.
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    /// "struct" | "enum" | "fn" | "trait" | "mod" | "type" | "const" |
    /// "static" | "macro" | "impl_fn"
    pub kind: String,
    pub name: String,
    /// 1-based line number
    pub line: usize,
    /// File mtime (seconds since UNIX epoch) at time of parsing.
    /// Used for incremental rebuild.
    #[serde(default)]
    pub source_mtime: u64,
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

/// Status of the symbol index relative to the current workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexStatus {
    /// Index is up to date (no .rs file newer than index).
    Fresh,
    /// Index exists but at least one .rs file is newer.
    Stale,
    /// Index file does not exist.
    Missing,
    /// Index build is in progress (background thread).
    Building,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    /// Default — case-insensitive substring match.
    Substring,
    /// Whole-word match (name is surrounded by non-alphanumeric or boundaries).
    WholeWord,
    /// Name starts with the query.
    Prefix,
    /// Exact case-insensitive match.
    Exact,
}

// ── Build ─────────────────────────────────────────────────────

/// Build the index by walking the workspace and parsing every `.rs`
/// file with `syn`. Uses incremental rebuild: files whose mtime has
/// not changed since the last build are copied from the old index.
///
/// `visibility` controls whether private symbols are included.
/// `is_building` is set to `true` while the build is in progress
/// (the caller should toggle it).
pub fn build_index(
    workspace: &Path,
    visibility: SymbolVisibility,
) -> SymbolIndex {
    // Load old index for incremental skip.
    let old_index = load_old_index(workspace);

    let mut files: BTreeMap<String, FileSymbols> = BTreeMap::new();
    let rs_entries = walk_rs_files(workspace);

    for (path, mtime) in rs_entries {
        let rel = match path.strip_prefix(workspace) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");

        // Incremental: reuse old entries when mtime hasn't changed.
        if let Some(old) = old_index.get(&rel_str) {
            if old.symbols.first().map_or(false, |s| s.source_mtime >= mtime) {
                files.insert(rel_str, old.clone());
                continue;
            }
        }

        if let Some(symbols) = extract_symbols(&path, visibility, mtime) {
            if !symbols.is_empty() {
                files.insert(rel_str, FileSymbols { symbols });
            }
        }
    }

    SymbolIndex {
        schema_version: 2,
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
    }
}

/// Determine the current status of the symbol index.
pub fn index_status(workspace: &Path) -> IndexStatus {
    let index_path = workspace.join(".deepseek").join("symbols.json");
    if !index_path.exists() {
        return IndexStatus::Missing;
    }

    let Ok(raw) = std::fs::read_to_string(&index_path) else {
        return IndexStatus::Missing;
    };
    let Ok(index): Result<SymbolIndex, _> = serde_json::from_str(&raw) else {
        return IndexStatus::Missing;
    };

    // Find the newest .rs file mtime in the workspace.
    let mut newest_rs_mtime: u64 = 0;
    for (_path, mtime) in walk_rs_files(workspace) {
        if mtime > newest_rs_mtime {
            newest_rs_mtime = mtime;
        }
    }

    // The index's generated_at isn't directly comparable to mtimes,
    // so check if any indexed file has a source_mtime older than
    // the file on disk.
    for (rel_str, file_syms) in &index.files {
        let disk_path = workspace.join(rel_str.replace('/', std::path::MAIN_SEPARATOR_STR));
        if let Ok(meta) = std::fs::metadata(&disk_path) {
            if let Ok(disk_mtime) = meta
                .modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
            {
                let idx_mtime = file_syms.symbols.first().map_or(0, |s| s.source_mtime);
                if disk_mtime > idx_mtime {
                    return IndexStatus::Stale;
                }
            }
        }
    }

    IndexStatus::Fresh
}

// ── Query ─────────────────────────────────────────────────────

/// Query the index for a symbol name with configurable match mode
/// and optional kind filter.
pub fn query_symbol_with_mode<'a>(
    index: &'a SymbolIndex,
    name: &str,
    mode: MatchMode,
    kind_filter: Option<&'a str>,
) -> Vec<(&'a str, usize)> {
    let name_lower = name.to_lowercase();
    let mut results: Vec<(&str, usize, u8)> = Vec::new(); // (file, line, priority 0=exact,1=prefix,2=word,3=substr)

    for (file, file_syms) in &index.files {
        for sym in &file_syms.symbols {
            if let Some(k) = kind_filter {
                if sym.kind != k {
                    continue;
                }
            }
            let sym_lower = sym.name.to_lowercase();
            let prio = match mode {
                MatchMode::Exact => {
                    if sym_lower == name_lower {
                        0
                    } else {
                        continue;
                    }
                }
                MatchMode::Prefix => {
                    if sym_lower == name_lower {
                        0
                    } else if sym_lower.starts_with(&name_lower) {
                        1
                    } else {
                        continue;
                    }
                }
                MatchMode::WholeWord => {
                    if sym_lower == name_lower {
                        0
                    } else if is_whole_word_match(&sym.name, &name_lower) {
                        2
                    } else {
                        continue;
                    }
                }
                MatchMode::Substring => {
                    if sym_lower == name_lower {
                        0
                    } else if sym_lower.starts_with(&name_lower) {
                        1
                    } else if is_whole_word_match(&sym.name, &name_lower) {
                        2
                    } else if sym_lower.contains(&name_lower) {
                        3
                    } else {
                        continue;
                    }
                }
            };
            results.push((file.as_str(), sym.line, prio));
        }
    }

    results.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(b.0)));
    results.into_iter().map(|(f, l, _)| (f, l)).collect()
}

/// Backward-compatible wrapper: substring match, no kind filter.
pub fn query_symbol<'a>(index: &'a SymbolIndex, name: &str) -> Vec<(&'a str, usize)> {
    query_symbol_with_mode(index, name, MatchMode::Substring, None)
}

// ── File summary ──────────────────────────────────────────────

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

/// Directories whose contents are skipped entirely during file walk.
const SKIP_DIRS: &[&str] = &[
    "target", "node_modules", "dist", ".git", ".deepseek", "binaries",
];

fn walk_rs_files(root: &Path) -> Vec<(PathBuf, u64)> {
    let mut files = Vec::new();
    walk_rs_files_impl(root, &mut files);
    files
}

fn walk_rs_files_impl(dir: &Path, out: &mut Vec<(PathBuf, u64)>) {
    let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
        return;
    };
    if SKIP_DIRS.contains(&name) {
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
            let mtime = std::fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            out.push((p, mtime));
        }
    }
}

/// Load the previous index for incremental rebuild.
fn load_old_index(workspace: &Path) -> BTreeMap<String, FileSymbols> {
    let index_path = workspace.join(".deepseek").join("symbols.json");
    let raw = match std::fs::read_to_string(&index_path) {
        Ok(r) => r,
        Err(_) => return BTreeMap::new(),
    };
    let index: SymbolIndex = match serde_json::from_str(&raw) {
        Ok(idx) => idx,
        Err(_) => return BTreeMap::new(),
    };
    index.files
}

fn extract_symbols(
    path: &Path,
    visibility: SymbolVisibility,
    source_mtime: u64,
) -> Option<Vec<SymbolEntry>> {
    let src = std::fs::read_to_string(path).ok()?;
    let file = syn::parse_file(&src).ok()?;
    let mut symbols: Vec<SymbolEntry> = Vec::new();

    // Build a byte-offset → line-number lookup once.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(src.match_indices('\n').map(|(i, _)| i + 1))
        .collect();

    // Top-level items (skip impl blocks — handled separately)
    for item in &file.items {
        if matches!(item, syn::Item::Impl(_)) {
            continue;
        }
        if let Some(entry) = item_symbol(item, visibility, &line_starts, source_mtime) {
            symbols.push(entry);
        }
    }

    // Also visit items inside modules (nested mods)
    for item in &file.items {
        if let syn::Item::Mod(m) = item {
            if let Some((_, ref content)) = m.content {
                for inner in content {
                    if !matches!(inner, syn::Item::Impl(_)) {
                        if let Some(entry) = item_symbol(inner, visibility, &line_starts, source_mtime) {
                            symbols.push(entry);
                        }
                    }
                }
            }
        }
    }

    // Handle impl blocks: collect method names with type prefix
    for item in &file.items {
        if let syn::Item::Impl(imp) = item {
            let impl_for = match &*imp.self_ty {
                syn::Type::Path(tp) => tp.path.segments.last().map(|s| s.ident.to_string()),
                _ => None,
            };
            for inner in &imp.items {
                if let syn::ImplItem::Fn(method) = inner {
                    if visibility == SymbolVisibility::Public && !is_pub(&method.vis) {
                        continue;
                    }
                    let name = if let Some(ref for_type) = impl_for {
                        format!("{}::{}", for_type, method.sig.ident)
                    } else {
                        method.sig.ident.to_string()
                    };
                    symbols.push(make_entry(
                        name,
                        "impl_fn",
                        method.sig.ident.span().byte_range().start,
                        &line_starts,
                        source_mtime,
                    ));
                }
            }
        }
    }

    // Dedup by (kind, name)
    symbols.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    symbols.dedup_by(|a, b| a.kind == b.kind && a.name == b.name);

    Some(symbols)
}

fn is_pub(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn line_for_byte_offset(line_starts: &[usize], byte_offset: usize) -> usize {
    line_starts.partition_point(|&start| start <= byte_offset)
}

fn make_entry(
    name: String,
    kind: &str,
    span_byte_start: usize,
    line_starts: &[usize],
    source_mtime: u64,
) -> SymbolEntry {
    SymbolEntry {
        kind: kind.into(),
        name,
        line: line_for_byte_offset(line_starts, span_byte_start),
        source_mtime,
    }
}

fn item_symbol(
    item: &syn::Item,
    visibility: SymbolVisibility,
    line_starts: &[usize],
    source_mtime: u64,
) -> Option<SymbolEntry> {
    match item {
        syn::Item::Fn(f) => {
            if visibility == SymbolVisibility::Public && !is_pub(&f.vis) {
                return None;
            }
            Some(make_entry(
                f.sig.ident.to_string(),
                "fn",
                f.sig.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Struct(s) => {
            if visibility == SymbolVisibility::Public && !is_pub(&s.vis) {
                return None;
            }
            Some(make_entry(
                s.ident.to_string(),
                "struct",
                s.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Enum(e) => {
            if visibility == SymbolVisibility::Public && !is_pub(&e.vis) {
                return None;
            }
            Some(make_entry(
                e.ident.to_string(),
                "enum",
                e.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Trait(t) => {
            if visibility == SymbolVisibility::Public && !is_pub(&t.vis) {
                return None;
            }
            Some(make_entry(
                t.ident.to_string(),
                "trait",
                t.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Mod(m) => {
            if visibility == SymbolVisibility::Public && !is_pub(&m.vis) {
                return None;
            }
            Some(make_entry(
                m.ident.to_string(),
                "mod",
                m.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Type(t) => {
            if visibility == SymbolVisibility::Public && !is_pub(&t.vis) {
                return None;
            }
            Some(make_entry(
                t.ident.to_string(),
                "type",
                t.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Const(c) => {
            if visibility == SymbolVisibility::Public && !is_pub(&c.vis) {
                return None;
            }
            Some(make_entry(
                c.ident.to_string(),
                "const",
                c.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Static(s) => {
            if visibility == SymbolVisibility::Public && !is_pub(&s.vis) {
                return None;
            }
            Some(make_entry(
                s.ident.to_string(),
                "static",
                s.ident.span().byte_range().start,
                line_starts,
                source_mtime,
            ))
        }
        syn::Item::Macro(m) => {
            if let Some(ident) = &m.ident {
                // ItemMacro does not have a `vis` field — all macros are
                // effectively crate-visible, indexed regardless of visibility.
                // Private macros from external crates are excluded via
                // the workspace walk anyway.
                Some(make_entry(
                    ident.to_string(),
                    "macro",
                    ident.span().byte_range().start,
                    line_starts,
                    source_mtime,
                ))
            } else {
                None
            }
        }
        syn::Item::Impl(_) => {
            // impl blocks are handled inline in extract_symbols
            None
        }
        _ => None,
    }
}

// ── Helpers ───────────────────────────────────────────────────

/// Check if `query_lower` appears as a whole word in `sym_original`.
/// `sym_original` preserves case (e.g. "AppConfig"), `query_lower` is
/// the lowercased query. Word boundaries are: start/end of string,
/// underscore, non-alphanumeric, OR CamelCase transitions (lowercase→uppercase
/// before the match, or uppercase following the match).
fn is_whole_word_match(sym_original: &str, query_lower: &str) -> bool {
    // Case-insensitive find: use the lowered version for position, then
    // check boundaries on the original (case-preserved) string.
    let sym_lower = sym_original.to_lowercase();
    let Some(pos) = sym_lower.find(query_lower) else {
        return false;
    };

    let before = if pos == 0 {
        true
    } else {
        let c = sym_original.as_bytes().get(pos - 1).copied().unwrap_or(0);
        match c {
            // Non-alphanumeric (except underscore which is a word separator)
            b if !b.is_ascii_alphanumeric() => true,
            b'_' => true,
            // Lowercase letter before an uppercase match start → CamelCase boundary
            // e.g. "AppConfig" matching "Config": 'p' before 'C' at pos 3
            b if b.is_ascii_lowercase() => {
                sym_original.as_bytes().get(pos).map_or(false, |m| m.is_ascii_uppercase())
            }
            _ => false,
        }
    };

    let after = {
        let end = pos + query_lower.len();
        end >= sym_original.len()
            || {
                let c = sym_original.as_bytes()[end];
                !c.is_ascii_alphanumeric()
                    || c == b'_'
                    || c.is_ascii_uppercase() // next word starts with uppercase
            }
    };

    before && after
}

// ── Tests ─────────────────────────────────────────────────────

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

        let idx = build_index(tmp.path(), SymbolVisibility::Public);
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
                    source_mtime: 0,
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
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"real.rs".into()));
        assert!(!names.contains(&"junk.rs".into()));
    }

    #[test]
    fn match_mode_exact() {
        let mut idx = empty_index();
        add_sym(&mut idx, "Config", "struct", 10);
        add_sym(&mut idx, "ConfigStore", "struct", 20);

        let hits = query_symbol_with_mode(&idx, "Config", MatchMode::Exact, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].1, 10);
    }

    #[test]
    fn match_mode_whole_word() {
        let mut idx = empty_index();
        add_sym(&mut idx, "Config", "struct", 10);
        add_sym(&mut idx, "AppConfig", "struct", 20);
        add_sym(&mut idx, "Reconfigure", "fn", 30);

        let hits = query_symbol_with_mode(&idx, "Config", MatchMode::WholeWord, None);
        assert_eq!(hits.len(), 2); // Config + AppConfig
        assert!(hits.iter().any(|h| h.1 == 10));
        assert!(hits.iter().any(|h| h.1 == 20));
        // Reconfigure NOT matched (whole-word fail)
    }

    #[test]
    fn kind_filter() {
        let mut idx = empty_index();
        add_sym(&mut idx, "Config", "struct", 10);
        add_sym(&mut idx, "Config", "fn", 15);

        let struct_hits = query_symbol_with_mode(&idx, "Config", MatchMode::Exact, Some("struct"));
        assert_eq!(struct_hits.len(), 1);
        assert_eq!(struct_hits[0].1, 10);

        let fn_hits = query_symbol_with_mode(&idx, "Config", MatchMode::Exact, Some("fn"));
        assert_eq!(fn_hits.len(), 1);
        assert_eq!(fn_hits[0].1, 15);
    }

    #[test]
    fn private_symbols_excluded_by_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("lib.rs"),
            "pub fn pub_fn() {}\nfn priv_fn() {}\npub struct PubStruct;\nstruct PrivStruct;",
        )
        .unwrap();

        let idx = build_index(tmp.path(), SymbolVisibility::Public);
        let syms = &idx.files["src/lib.rs"].symbols;
        assert!(syms.iter().any(|s| s.name == "pub_fn"));
        assert!(syms.iter().any(|s| s.name == "PubStruct"));
        assert!(!syms.iter().any(|s| s.name == "priv_fn"));
        assert!(!syms.iter().any(|s| s.name == "PrivStruct"));
    }

    #[test]
    fn private_symbols_included_when_all() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "fn priv_fn() {}").unwrap();

        let idx = build_index(tmp.path(), SymbolVisibility::All);
        let syms = &idx.files["src/lib.rs"].symbols;
        assert!(syms.iter().any(|s| s.name == "priv_fn"));
    }

    #[test]
    fn incremental_rebuild_skips_unchanged() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let lib_rs = src.join("lib.rs");
        std::fs::write(&lib_rs, "pub fn foo() {}").unwrap();

        // First build
        let idx1 = build_index(tmp.path(), SymbolVisibility::Public);
        assert!(idx1.files["src/lib.rs"]
            .symbols
            .iter()
            .any(|s| s.name == "foo"));

        // Second build — unchanged file should reuse old entry
        let idx2 = build_index(tmp.path(), SymbolVisibility::Public);
        assert!(idx2.files["src/lib.rs"]
            .symbols
            .iter()
            .any(|s| s.name == "foo"));
    }

    #[test]
    fn index_status_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let status = index_status(tmp.path());
        assert_eq!(status, IndexStatus::Missing);
    }

    fn empty_index() -> SymbolIndex {
        SymbolIndex {
            schema_version: 1,
            generated_at: String::new(),
            files: BTreeMap::new(),
        }
    }

    fn add_sym(idx: &mut SymbolIndex, name: &str, kind: &str, line: usize) {
        idx.files
            .entry("src/lib.rs".into())
            .or_insert_with(|| FileSymbols {
                symbols: Vec::new(),
            })
            .symbols
            .push(SymbolEntry {
                kind: kind.into(),
                name: name.into(),
                line,
                source_mtime: 0,
            });
    }
}
