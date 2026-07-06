//! `zagens skill` — list drafts, promote after human review (Phase 4.2).

use std::process::ExitCode;

use anyhow::Result;
use colored::Colorize;
use serde::Serialize;

use crate::cli::args::{SkillArgs, SkillCommand, SkillDraftsArgs, SkillPromoteArgs};
use crate::cli::context::CliContext;
use crate::skills::draft::{list_drafts, promote_draft};

#[derive(Serialize)]
struct DraftJson<'a> {
    name: &'a str,
    path: String,
    has_harness: bool,
}

pub fn run(ctx: &CliContext, args: SkillArgs) -> Result<ExitCode> {
    match args.command {
        SkillCommand::Drafts(list) => run_drafts(ctx, list),
        SkillCommand::Promote(promote) => run_promote(ctx, promote),
    }
}

fn run_drafts(ctx: &CliContext, args: SkillDraftsArgs) -> Result<ExitCode> {
    let drafts = list_drafts(&ctx.workspace)?;
    if args.json {
        let items: Vec<DraftJson> = drafts
            .iter()
            .map(|d| DraftJson {
                name: &d.name,
                path: d.path.display().to_string(),
                has_harness: d.has_harness,
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(ExitCode::SUCCESS);
    }
    if drafts.is_empty() {
        println!(
            "No skill drafts under {}.",
            ctx.workspace.join(".zagens/skill-drafts").display()
        );
        return Ok(ExitCode::SUCCESS);
    }
    println!("{}", "Skill drafts (awaiting human promote)".bold());
    for d in &drafts {
        let harness = if d.has_harness {
            "harness.toml"
        } else {
            "no harness"
        };
        println!(
            "  {}  {}  {}",
            d.name.cyan(),
            harness.dimmed(),
            d.path.display()
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn run_promote(ctx: &CliContext, args: SkillPromoteArgs) -> Result<ExitCode> {
    let outcome = promote_draft(&ctx.workspace, &args.name, args.global, args.replace)?;
    eprintln!(
        "Promoted `{}` → {}",
        outcome.name,
        outcome.installed_path.display()
    );
    eprintln!(
        "Skills dir: {} — load with `load_skill name={}` on next session.",
        outcome.skills_dir.display(),
        outcome.name
    );
    eprintln!(
        "{}",
        "Script trust is NOT auto-granted; use existing skill trust flow if the bundle ships scripts."
            .dimmed()
    );
    Ok(ExitCode::SUCCESS)
}
