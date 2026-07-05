//! `tests_pass` predicate — canonical test commands (cargo / go).

use super::types::{PredicateContext, PredicateError, PredicateResult, names};
use serde_json::Value;

fn resolve_test_command(args: &Value) -> Result<String, PredicateError> {
    if let Some(cmd) = args
        .get("cmd")
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return Ok(cmd.to_string());
    }

    let toolchain = args
        .get("toolchain")
        .and_then(|v| v.as_str())
        .unwrap_or("auto");

    let package = args
        .get("package")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();

    match toolchain {
        "go" => Ok("go test ./...".to_string()),
        "cargo" | "rust" => {
            if package.is_empty() {
                Ok("cargo test".to_string())
            } else {
                Ok(format!("cargo test -p {package}"))
            }
        }
        "auto" => {
            if args.get("go").is_some() {
                Ok("go test ./...".to_string())
            } else {
                Ok("cargo test".to_string())
            }
        }
        other => Err(PredicateError::InvalidArgs {
            predicate: names::TESTS_PASS.into(),
            message: format!("unsupported toolchain `{other}`"),
        }),
    }
}

pub async fn evaluate(
    ctx: &PredicateContext<'_>,
    args: &Value,
) -> Result<PredicateResult, PredicateError> {
    let command = resolve_test_command(args)?;
    let mut wrapped = args.clone();
    if wrapped.get("cmd").is_none() {
        wrapped["cmd"] = serde_json::Value::String(command);
    }
    super::exit_code::evaluate(ctx, &wrapped)
        .await
        .map(|mut r| {
            r.predicate = names::TESTS_PASS.to_string();
            r
        })
}

/// CLI / queue fallback when building a test command string.
pub fn resolve_for_cli(args: &Value) -> Result<String, PredicateError> {
    resolve_test_command(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolves_go_command() {
        assert_eq!(
            resolve_test_command(&json!({"toolchain": "go"})).unwrap(),
            "go test ./..."
        );
    }

    #[test]
    fn resolves_cargo_command() {
        assert_eq!(
            resolve_test_command(&json!({"toolchain": "cargo"})).unwrap(),
            "cargo test"
        );
    }
}
