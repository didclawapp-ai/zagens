//! `zagens gate` — Gate-as-Code validate / list presets (Phase 4.1).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use colored::Colorize;

use zagens_core::long_horizon::{ContractValidationReport, HarnessContract};

use crate::cli::args::{GateArgs, GateCommand, GateListArgs, GateValidateArgs};

struct BundledPreset {
    id: &'static str,
    description: &'static str,
    raw: &'static str,
}

const BUNDLED_PRESETS: &[BundledPreset] = &[
    BundledPreset {
        id: "rust-cargo-smoke",
        description: "cargo check + cargo test (Rust workspace smoke)",
        raw: include_str!("../../../../../docs/harness/gates/presets/rust-cargo-smoke.toml"),
    },
    BundledPreset {
        id: "go-build-vet",
        description: "go build, vet, and tests",
        raw: include_str!("../../../../../docs/harness/gates/presets/go-build-vet.toml"),
    },
    BundledPreset {
        id: "deliverables-min",
        description: "require deliverables/** without shell exec",
        raw: include_str!("../../../../../docs/harness/gates/presets/deliverables-min.toml"),
    },
    BundledPreset {
        id: "go-microstack-migrated",
        description: "MicroStack Layer-2 verify rows (predicate-native migration sample)",
        raw: include_str!("../../../../../docs/harness/gates/presets/go-microstack-migrated.toml"),
    },
];

pub fn run(args: GateArgs) -> Result<ExitCode> {
    match args.command {
        GateCommand::Validate(validate) => run_validate(validate),
        GateCommand::List(list) => run_list(list),
    }
}

pub fn load_gate_file(path: &Path) -> Result<(HarnessContract, ContractValidationReport)> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let (contract, report) = HarnessContract::parse_and_validate(&raw)?;
    Ok((contract, report))
}

pub fn resolve_preset(id: &str) -> Result<&'static str> {
    BUNDLED_PRESETS
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.raw)
        .ok_or_else(|| {
            let ids: Vec<_> = BUNDLED_PRESETS.iter().map(|p| p.id).collect();
            anyhow::anyhow!("unknown preset `{id}` (bundled: {})", ids.join(", "))
        })
}

fn run_validate(args: GateValidateArgs) -> Result<ExitCode> {
    let (source_label, raw) = match (&args.file, &args.preset) {
        (Some(path), None) => {
            let raw =
                fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
            (path.display().to_string(), raw)
        }
        (None, Some(id)) => {
            let raw = resolve_preset(id)?.to_string();
            (format!("preset:{id}"), raw)
        }
        (Some(_), Some(_)) => bail!("--file and --preset are mutually exclusive"),
        (None, None) => bail!("specify --file or --preset"),
    };

    let contract = HarnessContract::parse_toml(&raw)?;
    let report = contract.validate();

    if args.json {
        let payload = serde_json::json!({
            "source": source_label,
            "harness_id": contract.harness.id,
            "report": report,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        print_human(&source_label, &contract.harness.id, &report);
    }

    Ok(if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}

fn run_list(args: GateListArgs) -> Result<ExitCode> {
    if args.json {
        let items: Vec<_> = BUNDLED_PRESETS
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "description": p.description,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(ExitCode::SUCCESS);
    }

    println!("{}", "Bundled gate presets (Gate-as-Code v0)".bold());
    for preset in BUNDLED_PRESETS {
        println!("  {}  {}", preset.id.cyan(), preset.description.dimmed());
    }
    println!();
    println!(
        "{}",
        "Validate: zagens gate validate --preset <id>  |  --file path/to/gate.toml".dimmed()
    );
    Ok(ExitCode::SUCCESS)
}

fn print_human(source: &str, harness_id: &str, report: &ContractValidationReport) {
    println!("{}", "Gate contract validation".bold());
    println!("  source: {source}");
    if !harness_id.is_empty() {
        println!("  harness.id: {harness_id}");
    }
    println!(
        "  rows: {} verify · {} stages",
        report.verify_count, report.stage_count
    );

    for err in &report.errors {
        println!("  {} {}", "error:".red().bold(), err);
    }
    for warn in &report.warnings {
        println!("  {} {}", "warn:".yellow(), warn);
    }

    if report.ok {
        println!("  {}", "OK".green().bold());
    } else {
        println!("  {}", "FAILED".red().bold());
    }
}

/// Resolve a user path or preset id to a manifest path for display (queue --gate-file).
pub fn preset_path_hint(id: &str) -> Option<PathBuf> {
    if BUNDLED_PRESETS.iter().any(|p| p.id == id) {
        return Some(PathBuf::from(format!(
            "docs/harness/gates/presets/{id}.toml"
        )));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_presets_validate() {
        for preset in BUNDLED_PRESETS {
            let contract = HarnessContract::parse_toml(preset.raw).expect(preset.id);
            let report = contract.validate();
            assert!(report.ok, "{}: {:?}", preset.id, report.errors);
        }
    }
}
