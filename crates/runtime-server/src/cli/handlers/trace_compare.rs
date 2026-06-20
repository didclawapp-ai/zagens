//! `zagens trace compare` — side-by-side Kernel Trace Report diff.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use zagens_core::engine::{
    TraceBundle, build_trace_bundle_from_fixture, build_trace_compare_document,
    embed_trace_compare_in_html, trace_compare_to_json,
};

use crate::cli::args::TraceCompareArgs;
use crate::cli::context::CliContext;
use crate::cli::trace_thread::build_trace_bundle_for_thread_cli;
use crate::trace_export::load_trace_report_template;

pub fn run(ctx: &CliContext, args: TraceCompareArgs) -> Result<ExitCode> {
    let (left_label, left) = load_compare_side(
        ctx,
        args.left.as_deref(),
        args.left_fixture.as_ref(),
        args.include_harness,
        !args.no_redact,
        "left",
    )?;
    let (right_label, right) = load_compare_side(
        ctx,
        args.right.as_deref(),
        args.right_fixture.as_ref(),
        args.include_harness,
        !args.no_redact,
        "right",
    )?;

    let doc = build_trace_compare_document(left_label, left, right_label, right);
    write_compare_output(&doc, &args)?;
    Ok(ExitCode::SUCCESS)
}

fn load_compare_side(
    ctx: &CliContext,
    thread_id: Option<&str>,
    fixture: Option<&PathBuf>,
    include_harness: bool,
    redact: bool,
    side: &str,
) -> Result<(String, TraceBundle)> {
    match (thread_id, fixture) {
        (Some(id), None) if !id.trim().is_empty() => {
            let bundle = build_trace_bundle_for_thread_cli(ctx, id.trim(), include_harness, redact)
                .with_context(|| format!("build trace bundle for {side} thread {id}"))?;
            Ok((id.trim().to_string(), bundle))
        }
        (None, Some(path)) => {
            let bundle = build_trace_bundle_from_fixture(path).with_context(|| {
                format!("build trace bundle from {side} fixture {}", path.display())
            })?;
            let label = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("fixture")
                .to_string();
            Ok((label, bundle))
        }
        (Some(_), Some(_)) => {
            bail!("{side}: specify only one of --{side} thread id or --{side}-fixture")
        }
        _ => bail!("{side}: specify --{side} <thread_id> or --{side}-fixture <path>"),
    }
}

fn write_compare_output(
    doc: &zagens_core::engine::TraceCompareDocument,
    args: &TraceCompareArgs,
) -> Result<()> {
    let out = &args.out;
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }

    match args.format.as_str() {
        "bundle" => {
            let json = trace_compare_to_json(doc)?;
            fs::write(out, json).with_context(|| format!("write {}", out.display()))?;
            eprintln!("Wrote trace compare bundle → {}", out.display());
        }
        "html" => {
            let template = load_trace_report_template(args.template.as_deref())?;
            let html = embed_trace_compare_in_html(&template, doc)?;
            fs::write(out, html).with_context(|| format!("write {}", out.display()))?;
            eprintln!("Wrote trace compare report → {}", out.display());
        }
        other => bail!("unknown --format {other:?} (expected html or bundle)"),
    }

    Ok(())
}
