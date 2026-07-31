//! `zagens report` subcommands (Phase 2b).

use anyhow::Result;
use serde_json::json;

use crate::cli::args::{HarnessReportArgs, ReportArgs, ReportCommand};
use crate::cli::context::CliContext;
use crate::cli::doctor_tools::{build_tool_telemetry_report, default_sessions_db_path};
use crate::harness_report::{
    ReportFormats, default_out_dir, from_tool_telemetry, render_markdown, write_report_bundle,
};

pub async fn run(ctx: &CliContext, args: ReportArgs) -> Result<()> {
    match args.command {
        ReportCommand::Harness(harness) => run_harness(ctx, harness),
    }
}

fn run_harness(ctx: &CliContext, args: HarnessReportArgs) -> Result<()> {
    let db_path = args.sessions_db.unwrap_or_else(default_sessions_db_path);
    let telemetry = build_tool_telemetry_report(&db_path)?;
    let report_ctx = from_tool_telemetry(&telemetry);

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "telemetry": telemetry,
                "report": report_ctx,
                "markdown": render_markdown(&report_ctx),
            }))?
        );
        return Ok(());
    }

    let formats = if args.all_formats {
        ReportFormats::markdown_only()
    } else if args.format.is_empty() {
        ReportFormats::default_bundle()
    } else {
        ReportFormats::from_csv(&args.format)
    };

    let out_dir = args
        .out
        .unwrap_or_else(|| default_out_dir(&ctx.workspace, &report_ctx));
    let written = write_report_bundle(&ctx.workspace, &out_dir, &report_ctx, &formats)?;

    println!("Harness report written to {}", written.out_dir.display());
    if let Some(path) = written.markdown {
        println!("  markdown: {}", path.display());
    }
    for warn in written.warnings {
        eprintln!("  warning: {warn}");
    }

    Ok(())
}
