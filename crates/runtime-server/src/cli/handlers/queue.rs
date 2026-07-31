//! `zagens queue` subcommands (Phase 1a).

use anyhow::Result;

use crate::cli::context::CliContext;
use crate::night_queue::{
    self, EnqueueGateInput, QueueTaskStatus, RunOptions, render_briefing, resolve_gate_specs,
};

use super::super::args::{QueueArgs, QueueBriefingArgs, QueueCommand};

pub async fn run(ctx: &CliContext, args: QueueArgs) -> Result<()> {
    match args.command {
        QueueCommand::Add(add) => run_add(ctx, add),
        QueueCommand::List => run_list(ctx),
        QueueCommand::Run(run) => run_run(ctx, run).await,
        QueueCommand::Briefing(args) => run_briefing(ctx, args),
    }
}

fn run_add(ctx: &CliContext, add: super::super::args::QueueAddArgs) -> Result<()> {
    let gate = resolve_gate_specs(&EnqueueGateInput {
        gate: add.gate.clone(),
        gate_file: add.gate_file.clone(),
        gate_preset: add.gate_preset.clone(),
    })?;
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

fn run_briefing(ctx: &CliContext, args: QueueBriefingArgs) -> Result<()> {
    let doc = night_queue::load(&ctx.workspace)?;
    let md = render_briefing(&doc);
    println!("{md}");
    night_queue::write_briefing_to_handoff(&ctx.workspace, &doc)?;
    println!(
        "\n(written to {})",
        zagens_config::workspace_meta_file_write(&ctx.workspace, "handoff.md").display()
    );

    if args.office {
        eprintln!(
            "warning: --office export removed; use --format md or the zagens-office skill for docx/xlsx/pptx"
        );
    }

    Ok(())
}

fn format_status(status: QueueTaskStatus) -> &'static str {
    match status {
        QueueTaskStatus::Pending => "pending",
        QueueTaskStatus::Running => "running",
        QueueTaskStatus::Passed => "passed",
        QueueTaskStatus::Failed => "failed",
        QueueTaskStatus::RolledBack => "rolled_back",
        QueueTaskStatus::Canceled => "canceled",
    }
}

#[cfg(test)]
mod tests {
    use crate::night_queue::gate_parse::parse_gate_spec;

    #[test]
    fn parse_gate_with_args() {
        let g = parse_gate_spec("file_exists:path=foo.txt").unwrap();
        assert_eq!(g.predicate, "file_exists");
        assert_eq!(g.args["path"], "foo.txt");
    }
}
