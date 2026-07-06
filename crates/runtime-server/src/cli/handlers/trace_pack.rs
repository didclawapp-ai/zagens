//! `zagens trace pack` — Replay pack v0 export / validate (Phase 3.4).

use std::fs;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use zagens_core::engine::{
    build_replay_pack_from_fixture, parse_replay_pack_json, replay_pack_to_json,
    validate_replay_pack,
};

use crate::cli::args::{TracePackExportArgs, TracePackValidateArgs};
use crate::cli::context::CliContext;
use crate::trace_export::build_replay_pack_for_thread;

pub fn run_export(ctx: &CliContext, args: TracePackExportArgs) -> Result<ExitCode> {
    let pack = match (&args.fixture, &args.thread) {
        (Some(fixture), None) => build_replay_pack_from_fixture(fixture)
            .with_context(|| format!("build replay pack from {}", fixture.display()))?,
        (None, Some(thread_id)) => build_replay_pack_for_thread(
            thread_id,
            &ctx.config,
            &ctx.workspace,
            args.include_harness,
            args.include_session,
            !args.no_redact,
        )
        .with_context(|| format!("build replay pack for thread {thread_id}"))?,
        (Some(_), Some(_)) => bail!("--fixture and --thread are mutually exclusive"),
        (None, None) => bail!("specify exactly one of --fixture or --thread"),
    };

    write_pack(&pack, &args.out)?;
    eprintln!(
        "Wrote replay pack → {} ({} events, session={})",
        args.out.display(),
        pack.trace.events.len(),
        pack.metadata.includes_session
    );
    Ok(ExitCode::SUCCESS)
}

pub fn run_validate(args: TracePackValidateArgs) -> Result<ExitCode> {
    let raw = fs::read_to_string(&args.input)
        .with_context(|| format!("read {}", args.input.display()))?;
    let pack = parse_replay_pack_json(&raw)?;
    let report = validate_replay_pack(&pack);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&args.input.display().to_string(), &report);
    }

    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn write_pack(pack: &zagens_core::engine::ReplayPack, out: &std::path::Path) -> Result<()> {
    if let Some(parent) = out.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("create output directory {}", parent.display()))?;
    }
    let json = replay_pack_to_json(pack)?;
    fs::write(out, json).with_context(|| format!("write {}", out.display()))?;
    Ok(())
}

fn print_human(path: &str, report: &zagens_core::engine::ReplayPackValidation) {
    use colored::Colorize;

    println!("{}", "Replay pack validation".bold());
    println!("  file: {path}");
    println!("  schema: {}", report.schema_version);
    println!(
        "  coherence: {}",
        if report.coherence_ok {
            "ok".green().to_string()
        } else {
            "FAIL".red().to_string()
        }
    );
    if let Some(err) = &report.coherence_error {
        println!("  coherence_error: {err}");
    }
    println!("  events: {}", report.event_count);
    println!("  session: {}", report.includes_session);
    println!(
        "  golden_replay_compatible: {}",
        report.golden_replay_compatible
    );
    if !report.warnings.is_empty() {
        println!("  warnings:");
        for w in &report.warnings {
            println!("    - {w}");
        }
    }
    println!(
        "  result: {}",
        if report.ok {
            "PASS".green()
        } else {
            "FAIL".red()
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validate_golden_fixture_pack() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay/lht_continue.json");
        let pack = build_replay_pack_from_fixture(&fixture).expect("build");
        let report = validate_replay_pack(&pack);
        assert!(report.ok);
    }
}
