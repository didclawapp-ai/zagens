use std::path::Path;

use anyhow::{Context, Result};
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use serde::{Deserialize, Serialize};

use crate::paths::cap_sid_file;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapSids {
    pub workspace: String,
    pub readonly: String,
}

fn make_random_cap_sid_string() -> String {
    let mut rng = SmallRng::from_entropy();
    format!(
        "S-1-5-21-{}-{}-{}-{}",
        rng.next_u32(),
        rng.next_u32(),
        rng.next_u32(),
        rng.next_u32()
    )
}

fn persist_caps(path: &Path, caps: &CapSids) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("create cap sid dir {}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(caps)?;
    std::fs::write(path, json).with_context(|| format!("write cap sid file {}", path.display()))?;
    Ok(())
}

pub fn load_or_create_cap_sids(zagens_home: &Path) -> Result<CapSids> {
    let path = cap_sid_file(zagens_home);
    if path.exists() {
        let txt = std::fs::read_to_string(&path)
            .with_context(|| format!("read cap sid file {}", path.display()))?;
        let t = txt.trim();
        if t.starts_with('{') {
            return serde_json::from_str(t).context("parse cap_sid JSON");
        }
        if !t.is_empty() {
            let caps = CapSids {
                workspace: t.to_string(),
                readonly: make_random_cap_sid_string(),
            };
            persist_caps(&path, &caps)?;
            return Ok(caps);
        }
    }
    let caps = CapSids {
        workspace: make_random_cap_sid_string(),
        readonly: make_random_cap_sid_string(),
    };
    persist_caps(&path, &caps)?;
    Ok(caps)
}
