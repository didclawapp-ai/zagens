//! `zagens trace export` — Kernel Trace Report (KTR).

use std::fs;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use zagens_core::engine::{TraceBundle, build_trace_bundle_from_fixture, trace_bundle_to_json};

use crate::cli::args::TraceExportArgs;
use crate::cli::context::CliContext;
use crate::cli::trace_thread::build_trace_bundle_for_thread_cli;
use crate::trace_export::{load_trace_report_template, render_trace_html};

pub fn run(ctx: &CliContext, args: TraceExportArgs) -> Result<ExitCode> {
    let bundle = match (&args.fixture, &args.thread) {
        (Some(fixture), None) => build_trace_bundle_from_fixture(fixture)
            .with_context(|| format!("build trace bundle from {}", fixture.display()))?,
        (None, Some(thread_id)) => {
            build_trace_bundle_for_thread_cli(ctx, thread_id, args.include_harness, !args.no_redact)
                .with_context(|| format!("build trace bundle for thread {thread_id}"))?
        }
        (Some(_), Some(_)) => bail!("--fixture and --thread are mutually exclusive"),
        (None, None) => bail!("specify exactly one of --fixture or --thread"),
    };

    write_bundle_output(&bundle, &args)?;
    Ok(ExitCode::SUCCESS)
}

fn write_bundle_output(bundle: &TraceBundle, args: &TraceExportArgs) -> Result<()> {
    let out = &args.out;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }

    match args.format.as_str() {
        "bundle" => {
            let json = trace_bundle_to_json(bundle)?;
            fs::write(out, json).with_context(|| format!("write {}", out.display()))?;
            eprintln!("Wrote trace bundle → {}", out.display());
        }
        "html" => {
            let template = load_trace_report_template(args.template.as_deref())?;
            let html = render_trace_html(bundle, &template)?;
            fs::write(out, html).with_context(|| format!("write {}", out.display()))?;
            eprintln!("Wrote trace report → {}", out.display());
        }
        other => bail!("unknown --format {other:?} (expected html or bundle)"),
    }

    Ok(())
}
