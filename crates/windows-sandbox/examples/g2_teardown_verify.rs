//! Destructive Gate G2 probe: elevated teardown then residual inspection.
//!
//! **Warning:** removes sandbox users, WFP filters, and setup artifacts under
//! `ZAGENS_HOME`. Re-run `zagens sandbox setup` before other G2 probes.
//!
//! ```powershell
//! $env:ZAGENS_HOME = "F:\DeepSeek-TUI-desktop\.g2-teardown-home"
//! # Copy a provisioned tree first, or point at .g2-home if you accept re-provision.
//! cargo run --example g2_teardown_verify -p zagens-windows-sandbox
//! ```
//!
//! Pass `--inspect-only` to skip teardown and only report current residuals.

use std::env;

use zagens_windows_sandbox::{
    inspect_elevated_teardown_residuals, run_elevated_teardown, sandbox_setup_is_complete,
    zagens_home_from_env,
};

fn main() -> anyhow::Result<()> {
    let home = zagens_home_from_env();
    let inspect_only = env::args().any(|arg| arg == "--inspect-only");

    if !inspect_only && !sandbox_setup_is_complete(&home) {
        anyhow::bail!(
            "setup incomplete under {}; run provisioning first or use --inspect-only",
            home.display()
        );
    }

    if !inspect_only {
        let real_user = env::var("USERNAME").unwrap_or_else(|_| "Administrator".into());
        println!("running elevated teardown for {} …", home.display());
        run_elevated_teardown(&home, &real_user)?;
    }

    let report = inspect_elevated_teardown_residuals(&home)?;
    let report_path = home
        .join(".sandbox")
        .join("g2_teardown_residual_report.json");
    std::fs::create_dir_all(home.join(".sandbox"))?;
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    println!("clean={}", report.clean);
    println!("report: {}", report_path.display());
    println!(
        "marker={} secrets={} artifacts={} wfp={} offline_user={} online_user={}",
        report.setup_marker_present,
        report.secrets_dir_present,
        report.setup_artifacts_present,
        report.wfp_namespace_present,
        report.offline_user_exists,
        report.online_user_exists,
    );

    if !report.clean {
        std::process::exit(1);
    }
    Ok(())
}
