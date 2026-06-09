//! Online vs offline identity + network probes.
use std::collections::HashMap;
use std::time::Duration;

use zagens_windows_sandbox::{
    PlanInput, WindowsSandboxMode, plan_exec, spawn_sync, zagens_home_from_env,
};

fn elevated(workspace: &std::path::PathBuf, cmd: &str, net: bool) -> anyhow::Result<()> {
    let plan = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec!["/C".into(), cmd.into()],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: vec![],
        network_allowed: net,
        mode: WindowsSandboxMode::Elevated,
    })?;
    let out = spawn_sync(&plan, None, Some(Duration::from_secs(30)))?;
    println!(
        "net={net} cmd={cmd:?} exit={} stdout={:?} stderr={:?}",
        out.exit_code,
        out.stdout.trim(),
        out.stderr.trim()
    );
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let home = zagens_home_from_env();
    let workspace = home.join(format!("g2-net-{}", std::process::id()));
    std::fs::create_dir_all(&workspace)?;

    for net in [false, true] {
        elevated(&workspace, "whoami", net)?;
        elevated(&workspace, "ping -n 2 127.0.0.1", net)?;
        elevated(
            &workspace,
            r"C:\Windows\System32\curl.exe -s -m 8 http://example.com",
            net,
        )?;
        elevated(
            &workspace,
            r"C:\Windows\System32\curl.exe -s -m 8 https://example.com",
            net,
        )?;
        println!("---");
    }

    let _ = std::fs::remove_dir_all(&workspace);
    Ok(())
}
