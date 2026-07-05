//! `zagens queue` subcommands (Phase 1a).

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::cli::context::CliContext;
use crate::night_queue::{self, GatePredicateSpec, QueueTaskStatus, RunOptions, render_briefing};

use super::super::args::{QueueArgs, QueueCommand};

pub async fn run(ctx: &CliContext, args: QueueArgs) -> Result<()> {
    match args.command {
        QueueCommand::Add(add) => run_add(ctx, add),
        QueueCommand::List => run_list(ctx),
        QueueCommand::Run(run) => run_run(ctx, run).await,
        QueueCommand::Briefing => run_briefing(ctx),
    }
}

fn run_add(ctx: &CliContext, add: super::super::args::QueueAddArgs) -> Result<()> {
    let gate = parse_gates(&add.gate)?;
    let task = night_queue::enqueue(&ctx.workspace, add.prompt, gate, !add.no_worktree)?;
    println!("Enqueued {} ({})", task.id, format_status(task.status));
    if !task.gate.is_empty() {
        println!("  gate: {} predicate(s)", task.gate.len());
    }
    Ok(())
}

fn run_list(ctx: &CliContext) -> Result<()> {
    let doc = night_queue::load(&ctx.workspace)?;
    if doc.tasks.is_empty() {
        println!(
            "Queue is empty ({}).",
            night_queue::queue_path(&ctx.workspace).display()
        );
        return Ok(());
    }
    println!(
        "Night queue — {} task(s) @ {}",
        doc.tasks.len(),
        night_queue::queue_path(&ctx.workspace).display()
    );
    for task in &doc.tasks {
        println!(
            "  {}  {:?}  {}",
            task.id,
            task.status,
            night_queue::preview(&task.prompt, 72)
        );
    }
    Ok(())
}

async fn run_run(ctx: &CliContext, run: super::super::args::QueueRunArgs) -> Result<()> {
    let report = night_queue::run_pending(
        ctx,
        &ctx.config,
        RunOptions {
            max_parallel: run.max_parallel,
            use_worktree: !run.no_worktree,
            write_briefing: !run.no_briefing,
        },
    )
    .await?;
    println!(
        "Queue run complete: {} ran, {} passed, {} failed/rolled back",
        report.ran, report.passed, report.failed
    );
    Ok(())
}

fn run_briefing(ctx: &CliContext) -> Result<()> {
    let doc = night_queue::load(&ctx.workspace)?;
    let md = render_briefing(&doc);
    println!("{md}");
    night_queue::write_briefing_to_handoff(&ctx.workspace, &doc)?;
    println!(
        "\n(written to {})",
        zagens_config::workspace_meta_file_write(&ctx.workspace, "handoff.md").display()
    );
    Ok(())
}

fn parse_gates(specs: &[String]) -> Result<Vec<GatePredicateSpec>> {
    specs.iter().map(|s| parse_gate_spec(s)).collect()
}

fn parse_gate_spec(raw: &str) -> Result<GatePredicateSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("empty --gate value");
    }
    if trimmed.starts_with('{') {
        return serde_json::from_str(trimmed).context("parse gate JSON object");
    }
    let (predicate, rest) = trimmed
        .split_once(':')
        .map(|(p, r)| (p.trim(), Some(r.trim())))
        .unwrap_or((trimmed, None));
    if predicate.is_empty() {
        bail!("gate predicate name required");
    }
    let mut args = serde_json::Map::new();
    if let Some(rest) = rest
        && !rest.is_empty()
    {
        for part in rest.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let (key, value) = part
                .split_once('=')
                .with_context(|| format!("gate arg must be key=value: {part}"))?;
            args.insert(
                key.trim().to_string(),
                Value::String(value.trim().to_string()),
            );
        }
    }
    Ok(GatePredicateSpec {
        predicate: predicate.to_string(),
        args: Value::Object(args),
    })
}

fn format_status(status: QueueTaskStatus) -> &'static str {
    match status {
        QueueTaskStatus::Pending => "pending",
        QueueTaskStatus::Running => "running",
        QueueTaskStatus::Passed => "passed",
        QueueTaskStatus::Failed => "failed",
        QueueTaskStatus::RolledBack => "rolled_back",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_gate_with_args() {
        let g = parse_gate_spec("file_exists:path=foo.txt").unwrap();
        assert_eq!(g.predicate, "file_exists");
        assert_eq!(g.args["path"], "foo.txt");
    }
}
