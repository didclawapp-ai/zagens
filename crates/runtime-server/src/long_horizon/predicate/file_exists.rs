//! `file_exists` predicate — cross-platform path probe.

use std::path::Path;
use std::time::Instant;

use serde_json::Value;

use super::types::{PredicateError, PredicateResult, names};

pub fn evaluate_sync(workspace: &Path, args: &Value) -> Result<PredicateResult, PredicateError> {
    let started = Instant::now();
    let path_raw = args
        .get("path")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| PredicateError::InvalidArgs {
            predicate: names::FILE_EXISTS.into(),
            message: "missing `path`".into(),
        })?;
    let want_exists = args.get("exists").and_then(|v| v.as_bool()).unwrap_or(true);
    let path = workspace.join(path_raw);
    let exists = path.exists();
    let pass = exists == want_exists;
    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);

    if pass {
        Ok(PredicateResult::pass(names::FILE_EXISTS, duration_ms))
    } else {
        Ok(PredicateResult::fail(
            names::FILE_EXISTS,
            "path_mismatch",
            if want_exists {
                format!("expected `{path_raw}` to exist under workspace")
            } else {
                format!("expected `{path_raw}` to be absent")
            },
            duration_ms,
            1,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn file_exists_passes_when_present() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("out.txt"), b"x").unwrap();
        let result = evaluate_sync(dir.path(), &json!({"path": "out.txt"})).unwrap();
        assert!(result.pass);
    }

    #[test]
    fn file_exists_fails_when_missing() {
        let dir = TempDir::new().unwrap();
        let result = evaluate_sync(dir.path(), &json!({"path": "missing.txt"})).unwrap();
        assert!(!result.pass);
    }
}
