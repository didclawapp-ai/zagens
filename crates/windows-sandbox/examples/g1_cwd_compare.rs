use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
fn probe(cwd: &str) -> anyhow::Result<()> {
    let profile = std::env::var("USERPROFILE")?;
    let id_rsa = PathBuf::from(&profile).join(".ssh").join("id_rsa");
    let input = zagens_windows_sandbox::PlanInput {
        program: "powershell".into(),
        args: vec!["-Command".into(), format!("type {}", id_rsa.display())],
        cwd: PathBuf::from(cwd),
        env: HashMap::new(),
        writable_roots: vec![PathBuf::from(cwd)],
        protected_write_paths: vec![],
        network_allowed: false,
        mode: zagens_windows_sandbox::WindowsSandboxMode::Unelevated,
        private_desktop: false,
        tty: false,
    };
    let plan = zagens_windows_sandbox::plan_exec(input)?;
    let out = zagens_windows_sandbox::spawn_sync(&plan, None, Some(Duration::from_secs(15)))?;
    let leaked = out.stdout.contains("BEGIN OPENSSH PRIVATE KEY");
    println!("cwd={cwd} exit={} leaked={leaked}", out.exit_code);
    Ok(())
}
fn main() -> anyhow::Result<()> {
    probe(r"F:\DeepSeek-TUI-desktop")?;
    probe(r"\\?\F:\DeepSeek-TUI-desktop")?;
    Ok(())
}
