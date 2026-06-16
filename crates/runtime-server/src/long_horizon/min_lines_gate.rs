//! Layer-3 minimum line-count gate (TS-11) — catches "padding modules" vs real delivery.

use std::path::Path;

use zagens_core::long_horizon::MinLinesGateConfig;

use super::completion_audit::MissingDeliverable;
use crate::tools::glob_files::build_glob_set;

const DEFAULT_FRONTEND_GLOB: &str = "**/*.{ts,tsx,vue,jsx}";
const DEFAULT_BACKEND_GLOB: &str = "**/*.{rs,go,py}";

fn count_lines_in_file(path: &Path) -> u32 {
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0;
    };
    u32::try_from(text.lines().count()).unwrap_or(u32::MAX)
}

fn count_lines_under_glob(workspace: &Path, pattern: &str) -> u32 {
    let Ok(glob_set) = build_glob_set(pattern) else {
        return 0;
    };
    let mut total = 0u32;
    for path in crate::tools::workspace_walk::collect_workspace_files(workspace, true) {
        if !path.is_file() {
            continue;
        }
        let rel = path
            .strip_prefix(workspace)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if glob_set.is_match(&rel) {
            total = total.saturating_add(count_lines_in_file(&path));
        }
    }
    total
}

/// Check configured min line buckets against the workspace working tree.
#[must_use]
pub fn check_min_lines(workspace: &Path, config: &MinLinesGateConfig) -> Vec<MissingDeliverable> {
    let mut missing = Vec::new();
    if let Some(min) = config.frontend {
        let glob = config
            .frontend_glob
            .as_deref()
            .unwrap_or(DEFAULT_FRONTEND_GLOB);
        let actual = count_lines_under_glob(workspace, glob);
        if actual < min {
            missing.push(MissingDeliverable {
                id: "min_lines_frontend".to_string(),
                what: format!("frontend line count {actual} < required {min} (glob `{glob}`)"),
                evidence: format!("line_count={actual} min={min} glob={glob}"),
            });
        }
    }
    if let Some(min) = config.backend {
        let glob = config
            .backend_glob
            .as_deref()
            .unwrap_or(DEFAULT_BACKEND_GLOB);
        let actual = count_lines_under_glob(workspace, glob);
        if actual < min {
            missing.push(MissingDeliverable {
                id: "min_lines_backend".to_string(),
                what: format!("backend line count {actual} < required {min} (glob `{glob}`)"),
                evidence: format!("line_count={actual} min={min} glob={glob}"),
            });
        }
    }
    missing
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn passes_when_enough_lines() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        let body = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(root.join("src/app.ts"), body).expect("write");

        let config = MinLinesGateConfig {
            frontend: Some(50),
            frontend_glob: Some("**/*.ts".into()),
            ..MinLinesGateConfig::default()
        };
        assert!(check_min_lines(root, &config).is_empty());
    }

    #[test]
    fn fails_when_below_threshold() {
        let tmp = tempdir().expect("tempdir");
        let root = tmp.path();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").expect("write");

        let config = MinLinesGateConfig {
            backend: Some(100),
            backend_glob: Some("**/*.rs".into()),
            ..MinLinesGateConfig::default()
        };
        let missing = check_min_lines(root, &config);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].id, "min_lines_backend");
    }
}
