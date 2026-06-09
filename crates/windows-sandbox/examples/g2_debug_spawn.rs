//! Minimal elevated spawn debug (admin + completed setup required).
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use zagens_windows_sandbox::{
    PlanInput, WindowsSandboxMode, plan_exec, sandbox_setup_is_complete, spawn_sync,
    zagens_home_from_env,
};

fn main() -> anyhow::Result<()> {
    let home = zagens_home_from_env();
    if !sandbox_setup_is_complete(&home) {
        anyhow::bail!("setup incomplete");
    }
    let workspace = home.join(format!("g2-debug-{}", std::process::id()));
    std::fs::create_dir_all(&workspace)?;

    // Unelevated baseline (same process, restricted token).
    let unelev = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec!["/C".into(), "echo unelev-ok".into()],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: vec![],
        network_allowed: false,
        mode: WindowsSandboxMode::Unelevated,
    })?;
    let t0 = Instant::now();
    let unelev_out = spawn_sync(&unelev, None, Some(Duration::from_secs(30)))?;
    println!(
        "unelevated {:?} exit={} stdout={:?} stderr={:?}",
        t0.elapsed(),
        unelev_out.exit_code,
        unelev_out.stdout.trim(),
        unelev_out.stderr.trim()
    );

    // Elevated via runner IPC.
    let elevated = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec!["/C".into(), "echo elev-ok".into()],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: vec![],
        network_allowed: false,
        mode: WindowsSandboxMode::Elevated,
    })?;
    let t1 = Instant::now();
    let elev_out = spawn_sync(&elevated, None, Some(Duration::from_secs(30)))?;
    println!(
        "elevated {:?} exit={} stdout={:?} stderr={:?}",
        t1.elapsed(),
        elev_out.exit_code,
        elev_out.stdout.trim(),
        elev_out.stderr.trim()
    );

    // Slow child — detects immediate kill / no-op spawn.
    let ping = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec!["/C".into(), "ping -n 3 127.0.0.1".into()],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: vec![],
        network_allowed: false,
        mode: WindowsSandboxMode::Elevated,
    })?;
    let t2 = Instant::now();
    let ping_out = spawn_sync(&ping, None, Some(Duration::from_secs(30)))?;
    println!(
        "elevated-ping {:?} exit={} stdout_len={}",
        t2.elapsed(),
        ping_out.exit_code,
        ping_out.stdout.len()
    );

    let smoke = workspace.join("elev-smoke.txt");
    let _ = std::fs::remove_file(&smoke);
    let write = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec![
            "/C".into(),
            format!(r#"echo elev-file> "{}""#, smoke.display()),
        ],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: vec![],
        network_allowed: false,
        mode: WindowsSandboxMode::Elevated,
    })?;
    let write_out = spawn_sync(&write, None, Some(Duration::from_secs(30)))?;
    let file_body = std::fs::read_to_string(&smoke).unwrap_or_default();
    println!(
        "elevated-write exit={} file_exists={} file={:?} stdout={:?}",
        write_out.exit_code,
        smoke.is_file(),
        file_body.trim(),
        write_out.stdout.trim()
    );

    let _ = std::fs::remove_dir_all(&workspace);
    Ok(())
}
