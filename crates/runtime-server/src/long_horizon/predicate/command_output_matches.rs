//! `command_output_matches` predicate — in-process grep / pattern probes.

use std::path::Path;
use std::time::Instant;

use serde_json::Value;

use super::types::{PredicateError, PredicateResult, names};

pub fn evaluate_sync(workspace: &Path, args: &Value) -> Result<PredicateResult, PredicateError> {
    let started = Instant::now();
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| PredicateError::InvalidArgs {
            predicate: names::COMMAND_OUTPUT_MATCHES.into(),
            message: "missing `command` (or use pattern+file)".into(),
        })?;

    let native =
        super::super::verify_platform::try_native_verify(workspace, command).ok_or_else(|| {
            PredicateError::InvalidArgs {
                predicate: names::COMMAND_OUTPUT_MATCHES.into(),
                message: format!("command not recognized for native probe: {command}"),
            }
        })?;

    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    let pass = native.exit_code == 0
        && native.exit_class == super::super::verify_platform::NativeExitClass::Ok;

    if pass {
        Ok(
            PredicateResult::pass(names::COMMAND_OUTPUT_MATCHES, duration_ms)
                .with_output(native.stdout_tail, native.stderr_tail),
        )
    } else {
        let code = match native.exit_class {
            super::super::verify_platform::NativeExitClass::Infra => "infra",
            _ => "pattern_mismatch",
        };
        Ok(PredicateResult::fail(
            names::COMMAND_OUTPUT_MATCHES,
            code,
            native.stderr_tail.clone(),
            duration_ms,
            native.exit_code,
        )
        .with_output(native.stdout_tail, native.stderr_tail))
    }
}

/// Map layer-2 native verify to predicate result (shared with exit_code fallback).
#[must_use]
pub fn from_native(
    predicate: &str,
    started: Instant,
    native: super::super::verify_platform::NativeVerifyResult,
) -> PredicateResult {
    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    let pass = native.exit_code == 0
        && matches!(
            native.exit_class,
            super::super::verify_platform::NativeExitClass::Ok
        );
    if pass {
        PredicateResult::pass(predicate, duration_ms)
            .with_output(native.stdout_tail, native.stderr_tail)
    } else {
        PredicateResult::fail(
            predicate,
            "verify_failed",
            native.stderr_tail.clone(),
            duration_ms,
            native.exit_code,
        )
        .with_output(native.stdout_tail, native.stderr_tail)
    }
}
