//! Named predicate dispatcher (`predicate::evaluate`).

use std::time::Instant;

use serde_json::Value;

use super::command_output_matches;
use super::exit_code;
use super::file_exists;
use super::tests_pass;
use super::types::{PredicateContext, PredicateError, PredicateResult, names};

/// Evaluate a registered predicate by name and JSON args.
pub async fn evaluate(
    name: &str,
    args: &Value,
    ctx: &PredicateContext<'_>,
) -> Result<PredicateResult, PredicateError> {
    match name {
        names::FILE_EXISTS => file_exists::evaluate_sync(ctx.workspace, args),
        names::COMMAND_OUTPUT_MATCHES => command_output_matches::evaluate_sync(ctx.workspace, args),
        names::EXIT_CODE => exit_code::evaluate(ctx, args).await,
        names::TESTS_PASS => tests_pass::evaluate(ctx, args).await,
        other => Err(PredicateError::Unknown(other.to_string())),
    }
}

/// Sync-only predicates (no shell exec context required).
pub fn evaluate_sync(
    name: &str,
    args: &Value,
    workspace: &std::path::Path,
) -> Result<PredicateResult, PredicateError> {
    let _started = Instant::now();
    match name {
        names::FILE_EXISTS => file_exists::evaluate_sync(workspace, args),
        names::COMMAND_OUTPUT_MATCHES => command_output_matches::evaluate_sync(workspace, args),
        other => Err(PredicateError::Unknown(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn smoke_file_exists_predicate() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"1").unwrap();
        let result =
            evaluate_sync(names::FILE_EXISTS, &json!({"path": "a.txt"}), dir.path()).unwrap();
        assert!(result.pass);
    }

    #[test]
    fn smoke_command_output_matches_grep() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("f.rs"), "fn ok() {}\n").unwrap();
        let result = evaluate_sync(
            names::COMMAND_OUTPUT_MATCHES,
            &json!({"command": "grep -c not_impl f.rs"}),
            dir.path(),
        )
        .unwrap();
        assert!(result.pass);
    }

    #[test]
    fn unknown_predicate_errors() {
        let dir = TempDir::new().unwrap();
        let err = evaluate_sync("not_a_predicate", &json!({}), dir.path()).unwrap_err();
        assert!(matches!(err, PredicateError::Unknown(_)));
    }
}
