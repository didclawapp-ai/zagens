//! Parse queue gate specs from CLI / HTTP (Phase 1a · desktop enqueue).

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use zagens_core::long_horizon::HarnessContract;

use super::model::GatePredicateSpec;

#[derive(Debug, Clone, Default)]
pub struct EnqueueGateInput {
    pub gate: Vec<String>,
    pub gate_file: Option<std::path::PathBuf>,
    pub gate_preset: Option<String>,
}

pub fn resolve_gate_specs(input: &EnqueueGateInput) -> Result<Vec<GatePredicateSpec>> {
    let mut gate = parse_gates(&input.gate)?;
    if input.gate_file.is_some() && input.gate_preset.is_some() {
        bail!("gate_file and gate_preset are mutually exclusive");
    }
    if let Some(path) = input.gate_file.as_deref() {
        gate.extend(load_gate_from_file(path)?);
    } else if let Some(id) = input.gate_preset.as_deref() {
        let raw = crate::cli::handlers::gate::resolve_preset(id)?;
        let contract = HarnessContract::parse_toml(raw)?;
        let report = contract.validate();
        if !report.ok {
            bail!(
                "gate preset `{id}` failed validation: {}",
                report.errors.join("; ")
            );
        }
        gate.extend(contract_to_gate_specs(&contract));
    }
    Ok(gate)
}

fn parse_gates(specs: &[String]) -> Result<Vec<GatePredicateSpec>> {
    specs.iter().map(|s| parse_gate_spec(s)).collect()
}

fn load_gate_from_file(path: &Path) -> Result<Vec<GatePredicateSpec>> {
    let (contract, report) = crate::cli::handlers::gate::load_gate_file(path)?;
    if !report.ok {
        bail!(
            "gate file {} failed validation: {}",
            path.display(),
            report.errors.join("; ")
        );
    }
    if contract.flat_queue_gate_rows().is_empty() {
        bail!(
            "gate file {} has no flat [[verify]] rows (stage-bound skill rows are skipped for queue)",
            path.display()
        );
    }
    Ok(contract_to_gate_specs(&contract))
}

fn contract_to_gate_specs(contract: &HarnessContract) -> Vec<GatePredicateSpec> {
    contract
        .flat_queue_gate_rows()
        .into_iter()
        .map(|row| GatePredicateSpec {
            predicate: row.predicate,
            args: row.args,
        })
        .collect()
}

pub fn parse_gate_spec(raw: &str) -> Result<GatePredicateSpec> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        bail!("empty gate value");
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
