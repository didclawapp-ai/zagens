//! Top-level spawn dispatch (unelevated vs elevated).

use anyhow::{Result, anyhow};

use crate::plan::{WindowsExecPlan, WindowsSandboxMode};
use crate::process::{CapturedOutput, ManagedProcess, SpawnStdio};

pub fn spawn(plan: &WindowsExecPlan, stdio: SpawnStdio) -> Result<ManagedProcess> {
    match plan.mode {
        WindowsSandboxMode::Unelevated => crate::unelevated::spawn(plan, stdio),
        WindowsSandboxMode::Elevated => Err(anyhow!(
            "elevated background spawn uses ElevatedChild::spawn, not the handle-based path"
        )),
    }
}

/// Background spawn through the elevated runner (IPC-streamed output).
/// Returns an [`crate::ElevatedChild`]; call `start_output_pump` to stream.
pub fn spawn_background_elevated(
    plan: &WindowsExecPlan,
    stdin_data: Option<&str>,
) -> Result<crate::elevated::ElevatedChild> {
    match plan.mode {
        WindowsSandboxMode::Elevated => crate::elevated::ElevatedChild::spawn(plan, stdin_data),
        WindowsSandboxMode::Unelevated => Err(anyhow!(
            "spawn_background_elevated requires an elevated-mode plan"
        )),
    }
}

pub fn spawn_sync(
    plan: &WindowsExecPlan,
    stdin_data: Option<&str>,
    timeout: Option<std::time::Duration>,
) -> Result<CapturedOutput> {
    match plan.mode {
        WindowsSandboxMode::Unelevated => crate::unelevated::spawn_sync(plan, stdin_data, timeout),
        WindowsSandboxMode::Elevated => crate::elevated::spawn_sync(plan, stdin_data, timeout),
    }
}
