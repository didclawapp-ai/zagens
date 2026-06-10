//! Sandbox metadata surfaced to tools and agents.

use crate::sandbox::ExecEnv;

/// Windows elevated vs unelevated label for a prepared `ExecEnv`.
#[must_use]
pub fn windows_sandbox_mode_from_env(exec_env: &ExecEnv) -> Option<String> {
    #[cfg(windows)]
    {
        exec_env.windows_plan.as_ref().map(|plan| match plan.mode {
            zagens_windows_sandbox::WindowsSandboxMode::Elevated => "elevated".to_string(),
            zagens_windows_sandbox::WindowsSandboxMode::Unelevated => "unelevated".to_string(),
        })
    }
    #[cfg(not(windows))]
    {
        let _ = exec_env;
        None
    }
}
