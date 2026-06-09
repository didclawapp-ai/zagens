use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

fn main() -> anyhow::Result<()> {
    let profile = std::env::var("USERPROFILE")?;
    let id_rsa = PathBuf::from(&profile).join(".ssh").join("id_rsa");
    let cwd = PathBuf::from(r"F:\DeepSeek-TUI-desktop");
    let input = zagens_windows_sandbox::PlanInput {
        program: "powershell".into(),
        args: vec!["-Command".into(), format!("type {}", id_rsa.display())],
        cwd: cwd.clone(),
        env: HashMap::new(),
        writable_roots: vec![cwd.clone()],
        protected_write_paths: vec![],
        network_allowed: false,
    };
    let plan = zagens_windows_sandbox::plan_exec(input)?;

    // Path A: full spawn_sync (production)
    let out_a = zagens_windows_sandbox::spawn_sync(&plan, None, Some(Duration::from_secs(15)))?;
    println!(
        "spawn_sync exit={} leaked={}",
        out_a.exit_code,
        out_a.stdout.contains("BEGIN OPENSSH PRIVATE KEY")
    );

    // Path B: run_as_user with same argv as plan (G0-style)
    let home = zagens_windows_sandbox::paths::zagens_home_from_env();
    let caps = zagens_windows_sandbox::cap::load_or_create_cap_sids(&home)?;
    zagens_windows_sandbox::deny_read::apply_deny_read_acls(
        &[profile.parse::<PathBuf>().unwrap().join(".ssh")],
        &zagens_windows_sandbox::token::LocalSid::from_string(&caps.workspace)?,
    )?;
    let token = zagens_windows_sandbox::token::create_restricted_token_with_capabilities(&[
        &caps.workspace
    ])?;
    let out_b = zagens_windows_sandbox::process::run_as_user(
        token.handle(),
        &plan.argv,
        &plan.cwd,
        &plan.env,
    )?;
    println!(
        "run_as_user argv={:?} exit={} leaked={}",
        plan.argv,
        out_b.exit_code,
        out_b.stdout.contains("BEGIN OPENSSH PRIVATE KEY")
    );

    Ok(())
}
