//! SSH config Include/IdentityFile dependency roots (Codex-aligned depth limit).

use std::path::{Path, PathBuf};

const MAX_INCLUDE_DEPTH: usize = 32;

pub fn filter_ssh_config_dependency_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let ssh_config = std::env::var("USERPROFILE")
        .ok()
        .map(|profile| PathBuf::from(profile).join(".ssh").join("config"));
    let Some(ssh_config) = ssh_config else {
        return roots.to_vec();
    };
    let dependencies = collect_ssh_dependency_paths(&ssh_config, 0);
    if dependencies.is_empty() {
        return roots.to_vec();
    }
    roots
        .iter()
        .filter(|root| {
            !dependencies
                .iter()
                .any(|dep| root.starts_with(dep) || dep.starts_with(root))
        })
        .cloned()
        .collect()
}

fn collect_ssh_dependency_paths(config: &Path, depth: usize) -> Vec<PathBuf> {
    if depth >= MAX_INCLUDE_DEPTH || !config.is_file() {
        return Vec::new();
    }
    let Ok(content) = std::fs::read_to_string(config) else {
        return Vec::new();
    };
    let base = config.parent().unwrap_or_else(|| Path::new("."));
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("include ") {
            let pattern = line.split_whitespace().nth(1).unwrap_or("");
            if !pattern.is_empty() {
                out.push(base.join(pattern));
            }
        } else if lower.starts_with("identityfile ") {
            let path = line.split_whitespace().nth(1).unwrap_or("");
            if !path.is_empty() {
                out.push(base.join(path));
            }
        }
    }
    for include in out.clone() {
        out.extend(collect_ssh_dependency_paths(&include, depth + 1));
    }
    out
}
