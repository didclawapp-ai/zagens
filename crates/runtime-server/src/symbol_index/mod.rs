//! Build and query a symbol index for the workspace.
//!
//! The index is a JSON file at `.deepseek/symbols.json` mapping
//! workspace-relative file paths → list of (kind, name, line).
//!
//! Warmed up at `serve --http` startup (non-blocking background build)
//! and lazily refreshed on first `grep_files` call.  Supports incremental
//! rebuild by comparing file mtimes and schema version.  Missing/unparseable
//! files are skipped silently — the index is best-effort and must never
//! block the runtime.

#![allow(clippy::too_many_lines)]

mod extract;

use extract::{
    extract_cpp_symbols, extract_go_symbols, extract_py_symbols, extract_sfc_symbols,
    extract_ts_symbols,
};
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
    /// Known symbol names called within this function's body (best-effort).
    /// Only records names present in the current project's index.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSymbols {
    pub symbols: Vec<SymbolEntry>,
}

/// Bump when the index format changes in a way that invalidates
/// old caches (new file types, new symbol kinds, etc.).
const CURRENT_SCHEMA_VERSION: u32 = 5;

/// A symbol that calls the queried name (reverse of `calls`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerRef {
    pub name: String,
    pub file: String,
    pub line: usize,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub schema_version: u32,
    pub generated_at: String,
    pub files: BTreeMap<String, FileSymbols>,
    /// Optional: Tauri command bridge pairs (Rust ↔ TS).
    #[serde(default)]
    pub bridge_pairs: Vec<BridgePair>,
}

/// A cross-language Tauri command: Rust `#[tauri::command]` function
/// paired with its TS `invoke()` caller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgePair {
    pub command: String,
    pub rust_file: String,
    pub rust_line: usize,
    pub ts_file: String,
    pub ts_line: usize,
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
    let src_entries = walk_source_files(workspace);
    let mut to_parse: Vec<(PathBuf, u64, &'static str, String)> = Vec::new();

    for (path, mtime, lang) in src_entries {
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

        to_parse.push((path, mtime, lang, rel_str));
    }

    for (rel_str, syms) in parse_files_parallel(&to_parse, visibility) {
        files.insert(rel_str, FileSymbols { symbols: syms });
    }

    // Second pass: annotate calls for each function-like symbol.
    annotate_calls(workspace, &mut files);

    let bridge_pairs = build_bridge_pairs(workspace, &files);

    // Write change log for CRAFT audit trail (compare old vs new index).
    let _ = write_changes(workspace, &old_index, &files);

    SymbolIndex {
        schema_version: CURRENT_SCHEMA_VERSION,
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
        bridge_pairs,
    }
}

fn parse_file_symbols(
    path: &Path,
    lang: &str,
    visibility: SymbolVisibility,
    mtime: u64,
) -> Option<Vec<SymbolEntry>> {
    match lang {
        "rs" => extract_symbols(path, visibility, mtime),
        "ts" => extract_ts_symbols(path, mtime),
        "py" => extract_py_symbols(path, mtime),
        "go" => extract_go_symbols(path, mtime),
        "cpp" => extract_cpp_symbols(path, mtime),
        "sfc" => extract_sfc_symbols(path, mtime),
        _ => None,
    }
}

fn parse_files_parallel(
    entries: &[(PathBuf, u64, &'static str, String)],
    visibility: SymbolVisibility,
) -> Vec<(String, Vec<SymbolEntry>)> {
    if entries.is_empty() {
        return Vec::new();
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);

    if entries.len() == 1 || workers <= 1 {
        return entries
            .iter()
            .filter_map(|(path, mtime, lang, rel)| {
                parse_file_symbols(path, lang, visibility, *mtime)
                    .filter(|syms| !syms.is_empty())
                    .map(|syms| (rel.clone(), syms))
            })
            .collect();
    }

    std::thread::scope(|scope| {
        let chunk_size = entries.len().div_ceil(workers);
        let handles: Vec<_> = entries
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|(path, mtime, lang, rel)| {
                            parse_file_symbols(path, lang, visibility, *mtime)
                                .filter(|syms| !syms.is_empty())
                                .map(|syms| (rel.clone(), syms))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect()
    })
}

/// Second pass: scan each function body for calls to known symbol names
/// and record them in the `calls` field of the callee's SymbolEntry.
///
/// Only records names that appear in the current project's index
/// (external crate calls like `tokio::spawn` are excluded).
fn annotate_calls(workspace: &Path, files: &mut BTreeMap<String, FileSymbols>) {
    use regex::Regex;

    // Collect all known symbol names for lookup.
    let known_names: std::collections::HashSet<&str> = files
        .values()
        .flat_map(|f| f.symbols.iter().map(|s| s.name.as_str()))
        .collect();
    if known_names.is_empty() {
        return;
    }

    // Building one giant alternation regex over tens of thousands of symbols (e.g. when
    // workspace accidentally points at user home) can consume multiple GB of RAM.
    const MAX_ANNOTATE_CALLS_SYMBOLS: usize = 8_000;
    if known_names.len() > MAX_ANNOTATE_CALLS_SYMBOLS {
        tracing::warn!(
            "annotate_calls skipped: {} symbols exceed cap {}",
            known_names.len(),
            MAX_ANNOTATE_CALLS_SYMBOLS
        );
        return;
    }

    // Build a regex that matches any known symbol name as a word.
    // (Sorted by length descending so longer names match first — avoids
    // partial matches like "foo" inside "foo_bar".)
    let mut sorted_names: Vec<&&str> = known_names.iter().collect();
    sorted_names.sort_by_key(|n| -(n.len() as isize));
    let pattern = sorted_names
        .iter()
        .map(|n| regex::escape(n))
        .collect::<Vec<_>>()
        .join("|");
    let re_calls = match Regex::new(&pattern) {
        Ok(r) => r,
        Err(_) => return,
    };

    for (rel_path, file_syms) in files.iter_mut() {
        let abs_path = workspace.join(rel_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        let body = match std::fs::read_to_string(&abs_path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        // Build line-offset lookup for efficient line→offset mapping.
        let line_offsets: Vec<usize> = std::iter::once(0)
            .chain(body.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        // Pre-compute end-line for each symbol (needs immutable borrow).
        let end_lines: Vec<usize> = file_syms
            .symbols
            .iter()
            .map(|sym| {
                file_syms
                    .symbols
                    .iter()
                    .filter(|s| s.line > sym.line)
                    .map(|s| s.line.saturating_sub(1))
                    .min()
                    .unwrap_or(body.lines().count())
            })
            .collect();

        for (i, sym) in file_syms.symbols.iter_mut().enumerate() {
            // Only function-like symbols get calls.
            if !matches!(
                sym.kind.as_str(),
                "fn" | "impl_fn" | "method" | "trait_fn"
            ) {
                continue;
            }
            let start_line = sym.line.saturating_sub(1); // 0-based
            let end_line = end_lines.get(i).copied().unwrap_or(body.lines().count());
            let start = *line_offsets.get(start_line).unwrap_or(&0);
            let end = *line_offsets.get(end_line.min(line_offsets.len() - 1)).unwrap_or(&body.len());
            let fn_body = &body[start..end.min(body.len())];

            let mut calls: Vec<String> = re_calls
                .find_iter(fn_body)
                .map(|m| m.as_str().to_string())
                .filter(|c| *c != sym.name) // exclude self-calls
                .collect();
            calls.sort();
            calls.dedup();
            sym.calls = calls;
        }
    }
}

/// Write a lightweight change log comparing the new index against the
/// old one.  Only keeps the most recent diff — no history stacking.
fn write_changes(
    workspace: &Path,
    old_index: &BTreeMap<String, FileSymbols>,
    new_files: &BTreeMap<String, FileSymbols>,
) -> std::io::Result<()> {
    let mut added: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    let mut modified: Vec<String> = Vec::new();

    // Added / Modified: symbols in new but not old (or changed).
    for (path, new_syms) in new_files {
        if let Some(old_syms) = old_index.get(path) {
            // Compare line numbers and calls.
            let old_map: std::collections::HashMap<&str, (&str, usize)> = old_syms
                .symbols
                .iter()
                .map(|s| (s.name.as_str(), (s.kind.as_str(), s.line)))
                .collect();
            let new_map: std::collections::HashMap<&str, (&str, usize)> = new_syms
                .symbols
                .iter()
                .map(|s| (s.name.as_str(), (s.kind.as_str(), s.line)))
                .collect();
            for (name, &(_, new_line)) in &new_map {
                if let Some(&(_, old_line)) = old_map.get(name) {
                    if new_line.abs_diff(old_line) > 2 {
                        modified.push(format!("{path}::{name}"));
                    }
                } else {
                    added.push(format!("{path}::{name}"));
                }
            }
        } else {
            for s in &new_syms.symbols {
                added.push(format!("{path}::{}", s.name));
            }
        }
    }

    // Removed: symbols in old but not new.
    for (path, old_syms) in old_index {
        if !new_files.contains_key(path) {
            for s in &old_syms.symbols {
                removed.push(format!("{path}::{}", s.name));
            }
        }
    }

    if added.is_empty() && removed.is_empty() && modified.is_empty() {
        return Ok(());
    }

    let changes = serde_json::json!({
        "rebuilt_at": chrono::Utc::now().to_rfc3339(),
        "added": added,
        "removed": removed,
        "modified": modified,
    });

    let changes_dir = workspace.join(".deepseek");
    std::fs::create_dir_all(&changes_dir)?;
    std::fs::write(
        changes_dir.join(".symbols_changes.json"),
        serde_json::to_string_pretty(&changes).unwrap_or_default(),
    )?;
    Ok(())
}

/// Scan Rust and TS/TSX files for Tauri command bridges.
///
/// Rust side: locates `#[tauri::command]` attributes and the function
/// right below them.
/// TS side: matches `invoke('command_name')` calls.
/// Pairs are joined by command name.
fn build_bridge_pairs(workspace: &Path, _files: &BTreeMap<String, FileSymbols>) -> Vec<BridgePair> {
    use regex::Regex;
    let mut pairs: Vec<BridgePair> = Vec::new();

    // Collect Rust tauri commands: find #[tauri::command] then next fn name.
    let re_attr = Regex::new(r"#\[tauri::command\]").unwrap();
    let re_fn = Regex::new(r"\bfn\s+(\w+)").unwrap();

    // Collect TS invoke calls: invoke('cmd') or invoke("cmd").
    let re_invoke = Regex::new(r#"invoke\s*\(\s*['"]([^'"]+)['"]"#).unwrap();

    let mut rust_commands: std::collections::HashMap<String, (String, usize)> =
        std::collections::HashMap::new();
    let mut ts_commands: std::collections::HashMap<String, (String, usize)> =
        std::collections::HashMap::new();

    let src_entries = walk_source_files(workspace);
    for (path, _, lang) in &src_entries {
        let _ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let rel = match path.strip_prefix(workspace) {
            Ok(p) => p.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };

        match *lang {
            "rs" => {
                if let Ok(body) = std::fs::read_to_string(path) {
                    for m in re_attr.find_iter(&body) {
                        let after = &body[m.end()..];
                        let limit = after.floor_char_boundary(200.min(after.len()));
                        if let Some(cap) = re_fn.captures(&after[..limit]) {
                            let fn_name = cap[1].to_string();
                            let pre = match std::str::from_utf8(&body.as_bytes()[..m.start()]) {
                                Ok(s) => s,
                                Err(_) => continue,
                            };
                            let line = pre.lines().count() + 1;
                            rust_commands.insert(fn_name.clone(), (rel.clone(), line));
                        }
                    }
                }
            }
            "ts" => {
                if let Ok(body) = std::fs::read_to_string(path) {
                    for cap in re_invoke.captures_iter(&body) {
                        let cmd_name = cap[1].to_string();
                        if cmd_name.is_empty() { continue; }
                        let pre = &body[..cap.get(0).unwrap().start()];
                        let line = pre.lines().count() + 1;
                        ts_commands.insert(cmd_name, (rel.clone(), line));
                    }
                }
            }
            _ => {}
        }
    }

    // Match by command name.
    for (name, (rust_file, rust_line)) in &rust_commands {
        if let Some((ts_file, ts_line)) = ts_commands.get(name) {
            pairs.push(BridgePair {
                command: name.clone(),
                rust_file: rust_file.clone(),
                rust_line: *rust_line,
                ts_file: ts_file.clone(),
                ts_line: *ts_line,
            });
        }
    }
    pairs.sort_by(|a, b| a.command.cmp(&b.command));
    pairs
}

/// Compute a hex-encoded SHA-256 fingerprint of all source files'
/// paths + mtimes.  When this fingerprint matches the cached copy,
/// `index_status()` can skip the expensive per-file mtime comparison.
pub(crate) fn compute_fingerprint(workspace: &Path) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    // Sort for deterministic output.
    let mut entries: Vec<(PathBuf, u64)> = walk_source_files(workspace)
        .into_iter()
        .map(|(p, m, _)| (p, m))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, mtime) in &entries {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(b"\x00");
        hasher.update(&mtime.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Determine the current status of the symbol index.
///
/// Returns `Stale` when the index schema version is older than the
/// current version, or when any source file's on-disk mtime is newer
/// than the index entry.
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

    // Schema version mismatch → force rebuild (e.g. new file types added).
    if index.schema_version < CURRENT_SCHEMA_VERSION {
        return IndexStatus::Stale;
    }

    // Fast path: if a cached fingerprint matches the current workspace,
    // no source file has changed since the last build.
    let fp_path = workspace.join(".deepseek").join(".symbols_fingerprint");
    if let Ok(cached) = std::fs::read_to_string(&fp_path) {
        let current = compute_fingerprint(workspace);
        if cached.trim() == current {
            return IndexStatus::Fresh;
        }
    }

    // Slow path: fall back to per-file mtime comparison.
    let mut newest_rs_mtime: u64 = 0;
    for (_path, mtime, _lang) in walk_source_files(workspace) {
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

/// Build or refresh the symbol index in the background if it's missing or stale.
/// Non-blocking — spawns a background thread and returns immediately.
/// Idempotent: only one build runs per workspace at a time.
///
/// Called from two places:
/// - `grep_files` tool (on-demand, before the first symbol lookup)
/// - `update_thread` (when workspace is changed by Zagens or other clients)
pub fn ensure_symbol_index(workspace: &Path) {
    use std::collections::HashSet;
    use std::sync::{LazyLock, Mutex};

    static BUILDING: LazyLock<Mutex<HashSet<PathBuf>>> =
        LazyLock::new(|| Mutex::new(HashSet::new()));

    let index_path = workspace.join(".deepseek").join("symbols.json");
    let index: Option<SymbolIndex> = std::fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok());

    let needs_build = match &index {
        Some(_idx) => index_status(workspace) != IndexStatus::Fresh,
        None => true,
    };

    if !needs_build {
        return;
    }

    let ws_canonical = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf());

    {
        let mut set = BUILDING.lock().unwrap();
        if !set.insert(ws_canonical.clone()) {
            return; // already building for this workspace
        }
    }

    let ws = ws_canonical;
    std::thread::Builder::new()
        .name("symbol-index".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let index = build_index(&ws, SymbolVisibility::Public);
            let _ = std::fs::create_dir_all(ws.join(".deepseek"));
            let _ = std::fs::write(
                ws.join(".deepseek").join("symbols.json"),
                serde_json::to_string_pretty(&index).unwrap_or_default(),
            );
            let fp = compute_fingerprint(&ws);
            let _ = std::fs::write(
                ws.join(".deepseek").join(".symbols_fingerprint"),
                fp,
            );
            BUILDING.lock().unwrap().remove(&ws);
        })
        .ok();
}


/// Query the index for a symbol name with configurable match mode
/// and optional kind filter.
///
/// Returns (file, line, kind, match_priority) where priority is:
/// 0 = exact name match, 1 = prefix, 2 = whole-word, 3 = substring.
pub fn query_symbol_with_mode<'a>(
    index: &'a SymbolIndex,
    name: &str,
    mode: MatchMode,
    kind_filter: Option<&'a str>,
) -> Vec<(&'a str, usize, &'a str, u8)> {
    let name_lower = name.to_lowercase();
    let mut results: Vec<(&str, usize, &str, u8)> = Vec::new();

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
                    } else if name_lower.len() >= 3 && subsequence_match(&sym.name, &name_lower) {
                        4
                    } else if camel_acronym_match(&sym.name, &name_lower) {
                        4
                    } else {
                        continue;
                    }
                }
            };
            results.push((file.as_str(), sym.line, sym.kind.as_str(), prio));
        }
    }

    results.sort_by(|a, b| a.3.cmp(&b.3).then_with(|| a.0.cmp(b.0)));
    results.into_iter().map(|(f, l, k, p)| (f, l, k, p)).collect()
}

/// Backward-compatible wrapper: substring match, no kind filter.
pub fn query_symbol<'a>(index: &'a SymbolIndex, name: &str) -> Vec<(&'a str, usize, &'a str, u8)> {
    query_symbol_with_mode(index, name, MatchMode::Substring, None)
}

/// Find symbols whose `calls` list references `name` (reverse call graph).
pub fn query_callers(index: &SymbolIndex, name: &str) -> Vec<CallerRef> {
    let mut out: Vec<CallerRef> = Vec::new();

    for (file, file_syms) in &index.files {
        for sym in &file_syms.symbols {
            let references = sym.calls.iter().any(|c| {
                c.eq_ignore_ascii_case(name)
                    || c.rsplit("::")
                        .next()
                        .is_some_and(|short| short.eq_ignore_ascii_case(name))
            });
            if references {
                out.push(CallerRef {
                    name: sym.name.clone(),
                    file: file.clone(),
                    line: sym.line,
                    kind: sym.kind.clone(),
                });
            }
        }
    }

    out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    out
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
    "target",
    "node_modules",
    "dist",
    ".git",
    ".deepseek",
    "binaries",
    "AppData",
    ".cursor",
    ".vscode",
    ".cargo",
    "vendor",
    "__pycache__",
    ".pnpm-store",
    ".next",
    "build",
    ".cache",
    ".venv",
    "venv",
    "site-packages",
    "Windows",
    "Program Files",
    "Program Files (x86)",
    "Library",
    ".nuget",
    ".gradle",
    ".m2",
    ".zagens",
];

/// Skip symbol-index warmup on user home or non-project roots (V7 sidecar regression).
fn is_likely_user_home(workspace: &Path) -> bool {
    let Some(home) = dirs::home_dir() else {
        return false;
    };
    workspace
        .canonicalize()
        .ok()
        .zip(home.canonicalize().ok())
        .is_some_and(|(ws, hm)| ws == hm)
}

fn looks_like_project_root(workspace: &Path) -> bool {
    workspace.join("Cargo.toml").is_file()
        || workspace.join("package.json").is_file()
        || workspace.join("go.mod").is_file()
        || workspace.join("pyproject.toml").is_file()
        || workspace.join(".git").is_dir()
        || workspace.join(".deepseek").is_dir()
}

/// Non-blocking alias for sidecar / CLI startup warmup.
pub fn warmup_if_needed(workspace: &Path) {
    if is_likely_user_home(workspace) {
        tracing::debug!(
            "symbol index warmup skipped: workspace {:?} is user home",
            workspace
        );
        return;
    }
    if !looks_like_project_root(workspace) {
        tracing::debug!(
            "symbol index warmup skipped: no project markers under {:?}",
            workspace
        );
        return;
    }
    ensure_symbol_index(workspace);
}

fn walk_source_files(root: &Path) -> Vec<(PathBuf, u64, &'static str)> {
    let mut files = Vec::new();
    walk_source_files_impl(root, &mut files);
    files
}

fn walk_source_files_impl(dir: &Path, out: &mut Vec<(PathBuf, u64, &'static str)>) {
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
            walk_source_files_impl(&p, out);
        } else {
            let lang = match p.extension().and_then(|e| e.to_str()) {
                Some("rs") => "rs",
                Some("ts") | Some("tsx") | Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => {
                    "ts"
                }
                Some("py") | Some("pyi") => "py",
                Some("go") => "go",
                Some("c") | Some("h") | Some("cpp") | Some("cc") | Some("cxx") | Some("hpp")
                | Some("hxx") | Some("hh") => "cpp",
                Some("vue") | Some("svelte") => "sfc",
                _ => continue,
            };
            let mtime = std::fs::metadata(&p)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs())
                .unwrap_or(0);
            out.push((p, mtime, lang));
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

    // Top-level items + nested mods (recursive).
    extract_mod_items(&file.items, visibility, &line_starts, source_mtime, &mut symbols);

    // Handle impl blocks: collect method names with type prefix.
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

    // Handle trait methods: index as "TraitName::method_name" with kind "trait_fn".
    for item in &file.items {
        if let syn::Item::Trait(t) = item {
            if visibility == SymbolVisibility::Public && !is_pub(&t.vis) {
                continue;
            }
            let trait_name = t.ident.to_string();
            for trait_item in &t.items {
                if let syn::TraitItem::Fn(method) = trait_item {
                    let name = format!("{}::{}", trait_name, method.sig.ident);
                    symbols.push(make_entry(
                        name,
                        "trait_fn",
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

/// Recursively extract symbols from a list of items (handles nested mods).
fn extract_mod_items(
    items: &[syn::Item],
    visibility: SymbolVisibility,
    line_starts: &[usize],
    source_mtime: u64,
    symbols: &mut Vec<SymbolEntry>,
) {
    for item in items {
        if matches!(item, syn::Item::Impl(_)) {
            continue; // impl blocks handled separately
        }
        if let Some(entry) = item_symbol(item, visibility, line_starts, source_mtime) {
            symbols.push(entry);
        }
        // Recurse into inline mod content.
        if let syn::Item::Mod(m) = item {
            if let Some((_, ref content)) = m.content {
                extract_mod_items(content, visibility, line_starts, source_mtime, symbols);
            }
        }
    }
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
        calls: vec![],
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

/// True when all characters of `needle` appear in order within `haystack` (case-insensitive).
fn subsequence_match(haystack: &str, needle: &str) -> bool {
    if needle.len() < 3 {
        return false;
    }
    let h: Vec<char> = haystack.to_lowercase().chars().collect();
    let n: Vec<char> = needle.to_lowercase().chars().collect();
    let mut j = 0;
    for &c in &h {
        if j < n.len() && c == n[j] {
            j += 1;
        }
    }
    j == n.len()
}

/// CamelCase initials: `loadWorkspaceFile` → `lwf`.
fn camel_initials(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push(chars[0].to_ascii_lowercase());
    for i in 1..chars.len() {
        if chars[i].is_uppercase() && !chars[i - 1].is_uppercase() {
            out.push(chars[i].to_ascii_lowercase());
        }
    }
    out
}

fn camel_acronym_match(sym: &str, query_lower: &str) -> bool {
    if query_lower.len() < 2 || query_lower.len() > 16 {
        return false;
    }
    let initials = camel_initials(sym);
    initials == query_lower || initials.starts_with(query_lower)
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
            bridge_pairs: vec![],
        };
        index.files.insert(
            "src/lib.rs".into(),
            FileSymbols {
                symbols: vec![SymbolEntry {
                    kind: "fn".into(),
                    name: "build_router".into(),
                    line: 42,
                    source_mtime: 0,
                    calls: vec![],
                }],
            },
        );
        let hits = query_symbol(&index, "build_router");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0, "src/lib.rs");
        assert_eq!(hits[0].1, 42);
        assert_eq!(hits[0].2, "fn");
    }

    #[test]
    fn build_index_includes_python_and_go() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("worker.py"), "def worker():\n    pass\n").unwrap();
        std::fs::write(tmp.path().join("main.go"), "package main\n\nfunc main() {}\n").unwrap();

        let idx = build_index(tmp.path(), SymbolVisibility::Public);
        assert!(idx.files.contains_key("worker.py"));
        assert!(idx.files.contains_key("main.go"));
        assert_eq!(idx.schema_version, CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn walk_includes_js_and_python_extensions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("app.js"), "//").unwrap();
        std::fs::write(tmp.path().join("mod.py"), "#").unwrap();
        std::fs::write(tmp.path().join("main.go"), "//").unwrap();

        let files = walk_source_files(tmp.path());
        let exts: Vec<String> = files
            .iter()
            .filter_map(|(p, _, _)| p.extension().map(|e| e.to_string_lossy().into_owned()))
            .collect();
        assert!(exts.contains(&"js".into()));
        assert!(exts.contains(&"py".into()));
        assert!(exts.contains(&"go".into()));
    }

    #[test]
    fn warmup_skips_user_home() {
        let home = dirs::home_dir().expect("home");
        assert!(is_likely_user_home(&home));
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();
        assert!(!is_likely_user_home(tmp.path()));
        assert!(looks_like_project_root(tmp.path()));
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

        let files = walk_source_files(tmp.path());
        let names: Vec<String> = files
            .iter()
            .map(|(p, _, _)| p.file_name().unwrap().to_string_lossy().into_owned())
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
    fn substring_fuzzy_matches_camel_acronym() {
        let mut idx = empty_index();
        add_sym(&mut idx, "loadWorkspaceFileIntoPreview", "fn", 10);
        let hits = query_symbol_with_mode(&idx, "lwfip", MatchMode::Substring, None);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].3, 4);
    }

    #[test]
    fn query_callers_finds_referencing_symbols() {
        let mut idx = empty_index();
        add_sym_with_calls(&mut idx, "caller", "fn", 10, &["target"]);
        add_sym(&mut idx, "target", "fn", 20);
        let callers = query_callers(&idx, "target");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].name, "caller");
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
            bridge_pairs: vec![],
        }
    }

    fn add_sym_with_calls(
        idx: &mut SymbolIndex,
        name: &str,
        kind: &str,
        line: usize,
        calls: &[&str],
    ) {
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
                calls: calls.iter().map(|c| (*c).to_string()).collect(),
            });
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
                calls: vec![],
            });
    }
}