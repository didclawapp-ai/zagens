//! Model routing rules persistence (R-003 A4.6).

use std::path::Path;

use anyhow::Result;

use super::{RoutingRule, RoutingRulesDoc};

pub fn load_routing_rules(path: &Path) -> Result<Vec<RoutingRule>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)?;
    let doc: RoutingRulesDoc = serde_json::from_str(&data)?;
    Ok(doc.rules)
}

pub fn save_routing_rules(path: &Path, rules: &[RoutingRule]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = RoutingRulesDoc {
        rules: rules.to_vec(),
    };
    let json = serde_json::to_string_pretty(&doc)?;
    std::fs::write(path, json)?;
    Ok(())
}
