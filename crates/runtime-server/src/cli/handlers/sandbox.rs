use anyhow::Result;

use crate::cli::args::{SandboxCommand, SandboxPocCommand};

pub fn run(command: SandboxCommand) -> Result<()> {
    match command {
        SandboxCommand::Poc { command } => run_poc(command),
        SandboxCommand::Teardown { keep_logs } => run_teardown(keep_logs),
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
        "Teardown complete: revoked {} path(s); cap_sid_removed={}",
        report.revoked_paths, report.cap_sid_removed
    );
    Ok(())
}
