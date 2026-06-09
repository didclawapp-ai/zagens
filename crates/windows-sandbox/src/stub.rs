use anyhow::{Result, bail};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnelevatedDenyReadPocResult {
    pub result: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win32_last_error: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSandboxMode {
    Unelevated,
    Elevated,
}

#[derive(Debug, Clone)]
pub struct PlanInput {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub writable_roots: Vec<PathBuf>,
    pub protected_write_paths: Vec<PathBuf>,
    pub network_allowed: bool,
    pub mode: WindowsSandboxMode,
}

#[derive(Debug, Clone)]
pub struct WindowsExecPlan {
    pub mode: WindowsSandboxMode,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub writable_roots: Vec<PathBuf>,
    pub protected_write_paths: Vec<PathBuf>,
    pub apply_deny_read: bool,
    pub network_allowed: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SpawnStdio {
    pub capture_stdout: bool,
    pub capture_stderr: bool,
    pub stdin_open: bool,
    pub stdin_data: Option<String>,
}

pub struct CapturedOutput {
    pub exit_code: u32,
    pub stdout: String,
    pub stderr: String,
}

pub struct ManagedProcess;

pub struct TeardownReport {
    pub revoked_paths: usize,
    pub cap_sid_removed: bool,
}

pub fn zagens_home() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zagens")
}

pub fn sandbox_setup_is_complete(_home: &std::path::Path) -> bool {
    false
}

pub fn run_elevated_provisioning_setup_default(_real_user: &str) -> Result<()> {
    bail!("windows sandbox is only available on Windows")
}

pub fn is_enforcement_available() -> bool {
    false
}

pub fn plan_exec(_input: PlanInput) -> Result<WindowsExecPlan> {
    bail!("windows sandbox is only available on Windows")
}

pub fn protected_subdirs_for_root(_root: &std::path::Path) -> Vec<PathBuf> {
    Vec::new()
}

pub fn filter_ssh_config_dependency_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots.to_vec()
}

pub fn spawn(_plan: &WindowsExecPlan, _stdio: SpawnStdio) -> Result<ManagedProcess> {
    bail!("windows sandbox is only available on Windows")
}

pub fn spawn_sync(_plan: &WindowsExecPlan, _stdin: Option<&str>) -> Result<CapturedOutput> {
    bail!("windows sandbox is only available on Windows")
}

pub fn teardown_unelevated(_keep_logs: bool) -> Result<TeardownReport> {
    bail!("windows sandbox is only available on Windows")
}

pub fn run_unelevated_deny_read_poc() -> Result<UnelevatedDenyReadPocResult> {
    let _ = zagens_home();
    bail!("windows sandbox is only available on Windows")
}

pub fn write_poc_result(result: &UnelevatedDenyReadPocResult) -> Result<PathBuf> {
    let _ = result;
    bail!("windows sandbox is only available on Windows")
}
