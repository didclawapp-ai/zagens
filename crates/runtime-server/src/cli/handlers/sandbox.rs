use anyhow::Result;

use crate::cli::args::{SandboxCommand, SandboxPocCommand};

pub fn run(command: SandboxCommand) -> Result<()> {
    match command {
        SandboxCommand::Poc { command } => run_poc(command),
        SandboxCommand::Teardown { keep_logs } => run_teardown(keep_logs),
        SandboxCommand::Setup => run_setup(),
        SandboxCommand::Run { .. } => {
            anyhow::bail!("sandbox run is not available in headless CLI yet — see TUI方案.md")
        }
    }
}

fn run_poc(command: SandboxPocCommand) -> Result<()> {
    match command {
        SandboxPocCommand::DenyRead => run_poc_deny_read(),
    }
}

fn run_poc_deny_read() -> Result<()> {
    let result = zagens_windows_sandbox::run_unelevated_deny_read_poc()?;
    let path = zagens_windows_sandbox::write_poc_result(&result)?;
    println!("Gate G0 PoC result: {}", result.result);
    if let Some(notes) = &result.notes {
        println!("Notes: {notes}");
    }
    if let Some(probe) = &result.probe {
        println!("Probe: {probe}");
    }
    println!("Written to {}", path.display());
    if result.result != "pass" {
        std::process::exit(2);
    }
    Ok(())
}

fn run_teardown(keep_logs: bool) -> Result<()> {
    let report = zagens_windows_sandbox::teardown_unelevated(keep_logs)?;
    println!(
        "Unelevated teardown complete: revoked {} path(s); cap_sid_removed={}",
        report.revoked_paths, report.cap_sid_removed
    );

    // Elevated artifacts (sandbox users, WFP filters, secrets) need the UAC
    // helper; only trigger it when an elevated setup actually ran (PR-2.9).
    let home = zagens_windows_sandbox::zagens_home_from_env();
    if zagens_windows_sandbox::sandbox_setup_artifacts_present(&home) {
        let real_user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "Administrators".to_string());
        zagens_windows_sandbox::run_elevated_teardown_default(real_user.as_str())?;
        println!("Elevated teardown complete: sandbox users, WFP filters, and secrets removed.");
    }
    Ok(())
}

fn run_setup() -> Result<()> {
    let real_user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "Administrators".to_string());
    zagens_windows_sandbox::run_elevated_provisioning_setup_default(real_user.as_str())?;
    println!(
        "Windows elevated sandbox setup completed for {real_user}. \
         Set `[windows] sandbox = \"elevated\"` in config.toml to use the elevated path."
    );
    Ok(())
}
