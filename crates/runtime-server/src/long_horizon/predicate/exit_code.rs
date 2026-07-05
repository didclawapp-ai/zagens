//! `exit_code` predicate — shell command with exit-code oracle.

use std::time::Instant;

use serde_json::Value;

use super::shell_exec::run_shell_command;
use super::types::{PredicateContext, PredicateError, PredicateResult, names};
use super::verify_result::VerifyExitClass;

pub async fn evaluate(
    ctx: &PredicateContext<'_>,
    args: &Value,
) -> Result<PredicateResult, PredicateError> {
    let exec = ctx
        .exec
        .ok_or_else(|| PredicateError::NeedsExec(names::EXIT_CODE.into()))?;
    let started = Instant::now();

    let command = args
        .get("cmd")
        .or_else(|| args.get("command"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| PredicateError::InvalidArgs {
            predicate: names::EXIT_CODE.into(),
            message: "missing `cmd` or `command`".into(),
        })?;

    let run_dir = super::super::generic_gate::resolve_command_root(ctx.workspace, command);
    if let Some(native) =
        super::super::verify_platform::try_native_verify(run_dir.as_path(), command)
    {
        return Ok(super::command_output_matches::from_native(
            names::EXIT_CODE,
            started,
            native,
        ));
    }

    let adapted = super::super::verify_platform::adapt_verify_command_for_platform(command);
    let run = run_shell_command(
        run_dir.as_path(),
        adapted.as_ref(),
        &ctx.run_id,
        command,
        ctx.timeout_ms,
        exec,
    )
    .await;

    let duration_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    if run.pass() {
        Ok(PredicateResult::pass(names::EXIT_CODE, duration_ms)
            .with_output(run.stdout_tail, run.stderr_tail))
    } else {
        let code = match run.exit_class {
            VerifyExitClass::Infra => "infra",
            VerifyExitClass::Timeout => "timeout",
            VerifyExitClass::Cancelled => "cancelled",
            _ => "non_zero_exit",
        };
        Ok(PredicateResult::fail(
            names::EXIT_CODE,
            code,
            if run.stderr_tail.is_empty() {
                format!("command exited with {}", run.exit_code)
            } else {
                run.stderr_tail.clone()
            },
            duration_ms,
            run.exit_code,
        )
        .with_output(run.stdout_tail, run.stderr_tail))
    }
}
