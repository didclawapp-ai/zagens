//! H3 symbol search facade — first-class search entry over `.zagens/symbols.json`.

use std::path::Path;

use serde::Serialize;

use crate::symbol_index::{self, MatchMode, SymbolIndex};

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SymbolSearchHit {
    pub file: String,
    pub line: usize,
    pub kind: String,
    pub name: String,
    pub match_priority: u8,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct SymbolSearchResult {
    pub query: String,
    pub hits: Vec<SymbolSearchHit>,
    pub index_status: String,
    pub truncated: bool,
}

/// Query the workspace symbol index (best-effort; ensures index exists).
pub fn search_workspace_symbols(
    workspace: &Path,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> SymbolSearchResult {
    let query = query.trim();
    let limit = limit.clamp(1, 100);
    symbol_index::ensure_symbol_index(workspace);
    let status = symbol_index::index_status(workspace);
    let index_status = format!("{status:?}").to_lowercase();

    if query.is_empty() {
        return SymbolSearchResult {
            query: String::new(),
            hits: Vec::new(),
            index_status,
            truncated: false,
        };
    }

    let index_path = zagens_config::workspace_meta_file_read(workspace, "symbols.json");
    let hits = std::fs::read_to_string(&index_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<SymbolIndex>(&raw).ok())
        .map(|index| collect_hits(&index, query, kind, limit))
        .unwrap_or_default();

    SymbolSearchResult {
        query: query.to_string(),
        hits: hits.clone(),
        index_status,
        truncated: hits.len() >= limit,
    }
}

fn collect_hits(
    index: &SymbolIndex,
    query: &str,
    kind: Option<&str>,
    limit: usize,
) -> Vec<SymbolSearchHit> {
    let mut names_seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for (file, line, kind, prio) in
        symbol_index::query_symbol_with_mode(index, query, MatchMode::Substring, kind)
    {
        let file_syms = index.files.get(file);
        let name = file_syms
            .and_then(|fs| fs.symbols.iter().find(|s| s.line == line && s.kind == kind))
            .map(|s| s.name.as_str())
            .unwrap_or(query);
        let key = format!("{file}:{line}:{name}");
        if !names_seen.insert(key) {
            continue;
        }
        out.push(SymbolSearchHit {
            file: file.to_string(),
            line,
            kind: kind.to_string(),
            name: name.to_string(),
            match_priority: prio,
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn empty_query_returns_no_hits() {
        let dir = TempDir::new().expect("tempdir");
        let res = search_workspace_symbols(dir.path(), "  ", None, 10);
        assert!(res.hits.is_empty());
    }

    #[test]
    fn reads_existing_index_file() {
        let dir = TempDir::new().expect("tempdir");
        let meta = dir.path().join(".zagens");
        fs::create_dir_all(&meta).expect("mkdir");
        fs::write(
            meta.join("symbols.json"),
            r#"{"schema_version":5,"generated_at":"x","files":{"src/lib.rs":{"symbols":[{"kind":"fn","name":"hello","line":1}]}}}"#,
        )
        .expect("write index");
        let res = search_workspace_symbols(dir.path(), "hello", None, 5);
        assert_eq!(res.hits.len(), 1);
        assert_eq!(res.hits[0].name, "hello");
    }
}
