//! `file_count` predicate — workspace glob match count bounds.

use std::path::Path;
use std::time::Instant;

use globset::{Glob, GlobSetBuilder};
use serde_json::Value;

use super::types::{PredicateError, PredicateResult, names};

pub fn evaluate_sync(workspace: &Path, args: &Value) -> Result<PredicateResult, PredicateError> {
    let started = Instant::now();
    let glob_raw = args
        .get("glob")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| PredicateError::InvalidArgs {
            predicate: names::FILE_COUNT.into(),
            message: "missing `glob`".into(),
        })?;

    let min = args.get("min").and_then(|v| v.as_u64()).unwrap_or(0);
    let max = args.get("max").and_then(|v| v.as_u64());

    let normalized = glob_raw.replace('\\', "/");
    let mut builder = GlobSetBuilder::new();
    builder.add(
        Glob::new(&normalized).map_err(|e| PredicateError::InvalidArgs {
            predicate: names::FILE_COUNT.into(),
            message: format!("invalid glob: {e}"),
        })?,
    );
    let glob_set = builder.build().map_err(|e| PredicateError::InvalidArgs {
        predicate: names::FILE_COUNT.into(),
        message: format!("invalid glob: {e}"),
    })?;

    let count = count_glob_matches(workspace, &glob_set);
    let pass = count >= min && max.is_none_or(|m| count <= m);
    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

    if pass {
        Ok(PredicateResult::pass(names::FILE_COUNT, duration_ms))
    } else {
        Ok(PredicateResult::fail(
            names::FILE_COUNT,
            "count_out_of_range",
            format!("glob `{glob_raw}` matched {count} files (min={min}, max={max:?})"),
            duration_ms,
            1,
        ))
    }
}

fn count_glob_matches(workspace: &Path, glob_set: &globset::GlobSet) -> u64 {
    let mut count = 0u64;
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return 0;
    };
    walk_dir(workspace, workspace, glob_set, &mut count);
    let _ = entries;
    count
}

fn walk_dir(workspace: &Path, dir: &Path, glob_set: &globset::GlobSet, count: &mut u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            walk_dir(workspace, &path, glob_set, count);
            continue;
        }
        let Ok(rel) = path.strip_prefix(workspace) else {
            continue;
        };
        let posix = rel.to_string_lossy().replace('\\', "/");
        if glob_set.is_match(&posix) {
            *count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn file_count_min_passes() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), b"").unwrap();
        std::fs::write(dir.path().join("src/b.rs"), b"").unwrap();
        let result = evaluate_sync(dir.path(), &json!({"glob": "src/**/*.rs", "min": 2})).unwrap();
        assert!(result.pass);
    }

    #[test]
    fn file_count_fails_below_min() {
        let dir = TempDir::new().unwrap();
        let result = evaluate_sync(dir.path(), &json!({"glob": "**/*.rs", "min": 1})).unwrap();
        assert!(!result.pass);
    }
}
