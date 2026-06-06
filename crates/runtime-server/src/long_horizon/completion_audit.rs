//! Layer-3 completion gate — pure machine deliverable reconciliation (§6.2–6.3).

use std::path::{Path, PathBuf};

use deepseek_core::long_horizon::CompletionGateDeliverableEntry;
use globset::{Glob, GlobSetBuilder};
use serde::Serialize;

use crate::path_guard::resolve_under_workspace;
use crate::runtime_api::workspace::run_git;
use crate::tools::workspace_walk::collect_workspace_files;

use super::manifest_gate::{CompletionGateExec, run_optional_verify_cmd};

/// One missing deliverable (deterministic output, not LLM).
#[derive(Debug, Clone, Serialize)]
pub struct MissingDeliverable {
    pub id: String,
    pub what: String,
    pub evidence: String,
}

/// Layer-3 structured audit output (§6.3).
#[derive(Debug, Clone, Serialize)]
pub struct CompletionAuditResult {
    pub pass: bool,
    pub failing_gates: Vec<String>,
    pub missing_deliverables: Vec<MissingDeliverable>,
    pub manifest_round: u32,
    pub layer2_cache_trusted: bool,
}

/// Reconcile deliverable manifest against the workspace working tree.
///
/// `layer2_trusted` is set by the flow when no **enforced** layer-2 verify
/// failed this round (§7.7 same-round trust); `layer2_failing_diag` carries any
/// enforced-failing ids purely as a diagnostic (normally empty on this path).
#[must_use]
pub fn audit_deliverables(
    workspace: &Path,
    entries: &[CompletionGateDeliverableEntry],
    layer2_trusted: bool,
    layer2_failing_diag: &[String],
    manifest_round: u32,
) -> CompletionAuditResult {
    let layer2_cache_trusted = layer2_trusted;
    let failing_gates = if layer2_cache_trusted {
        Vec::new()
    } else {
        layer2_failing_diag.to_vec()
    };

    let mut missing = Vec::new();
    for entry in entries {
        if let Some(m) = check_deliverable(workspace, entry) {
            missing.push(m);
        }
    }

    let pass = failing_gates.is_empty() && missing.is_empty();
    CompletionAuditResult {
        pass,
        failing_gates,
        missing_deliverables: missing,
        manifest_round,
        layer2_cache_trusted,
    }
}

/// Layer-3 audit plus optional per-deliverable verify commands (§6.2).
pub async fn audit_deliverables_async(
    workspace: &Path,
    entries: &[CompletionGateDeliverableEntry],
    layer2_trusted: bool,
    layer2_failing_diag: &[String],
    manifest_round: u32,
    exec: Option<&CompletionGateExec<'_>>,
) -> CompletionAuditResult {
    let mut audit = audit_deliverables(
        workspace,
        entries,
        layer2_trusted,
        layer2_failing_diag,
        manifest_round,
    );
    let Some(exec) = exec else {
        return audit;
    };

    for entry in entries {
        let Some(cmd) = entry.optional_verify_cmd.as_deref() else {
            continue;
        };
        if cmd.trim().is_empty() {
            continue;
        }
        if audit.missing_deliverables.iter().any(|m| m.id == entry.id) {
            continue;
        }
        let run = run_optional_verify_cmd(workspace, &entry.id, cmd, exec).await;
        if run.exit_code != 0 {
            audit.pass = false;
            audit.missing_deliverables.push(MissingDeliverable {
                id: entry.id.clone(),
                what: format!("optional_verify_cmd 未通过: {cmd}"),
                evidence: format!(
                    "verify_failed exit={} class={:?}",
                    run.exit_code, run.exit_class
                ),
            });
        }
    }

    audit
}

fn check_deliverable(
    workspace: &Path,
    entry: &CompletionGateDeliverableEntry,
) -> Option<MissingDeliverable> {
    if let Some(path) = entry.path.as_deref() {
        return check_path_deliverable(workspace, entry, path);
    }
    if let Some(glob) = entry.glob.as_deref() {
        return check_glob_deliverable(workspace, entry, glob);
    }
    Some(MissingDeliverable {
        id: entry.id.clone(),
        what: "manifest entry has neither path nor glob".to_string(),
        evidence: "invalid_manifest_entry".to_string(),
    })
}

fn check_path_deliverable(
    workspace: &Path,
    entry: &CompletionGateDeliverableEntry,
    raw_path: &str,
) -> Option<MissingDeliverable> {
    let resolved = match resolve_under_workspace(workspace, raw_path) {
        Ok(p) => p,
        Err(e) => {
            return Some(MissingDeliverable {
                id: entry.id.clone(),
                what: format!("{raw_path} 路径无效"),
                evidence: e,
            });
        }
    };

    if !resolved.exists() {
        return Some(MissingDeliverable {
            id: entry.id.clone(),
            what: format!("{raw_path} 不存在"),
            evidence: "workspace path 不存在".to_string(),
        });
    }

    if entry.tracked {
        return check_git_tracked(workspace, entry, raw_path, &resolved);
    }

    None
}

fn check_glob_deliverable(
    workspace: &Path,
    entry: &CompletionGateDeliverableEntry,
    pattern: &str,
) -> Option<MissingDeliverable> {
    let hits = count_glob_hits(workspace, pattern);
    if hits == 0 {
        return Some(MissingDeliverable {
            id: entry.id.clone(),
            what: format!("glob `{pattern}` 零命中"),
            evidence: format!("glob {pattern} 零命中"),
        });
    }
    None
}

fn count_glob_hits(workspace: &Path, pattern: &str) -> usize {
    let normalized = pattern.replace('\\', "/");
    let Ok(glob) = Glob::new(&normalized) else {
        return 0;
    };
    let Ok(set) = GlobSetBuilder::new().add(glob).build() else {
        return 0;
    };
    let files = collect_workspace_files(workspace, true);
    files
        .into_iter()
        .filter(|p| {
            p.strip_prefix(workspace)
                .ok()
                .and_then(|rel| rel.to_str())
                .is_some_and(|rel| set.is_match(&rel.replace('\\', "/")))
        })
        .count()
}

fn check_git_tracked(
    workspace: &Path,
    entry: &CompletionGateDeliverableEntry,
    raw_path: &str,
    resolved: &PathBuf,
) -> Option<MissingDeliverable> {
    if !resolved.exists() {
        return Some(MissingDeliverable {
            id: entry.id.clone(),
            what: format!("{raw_path} 不存在"),
            evidence: "path_missing".to_string(),
        });
    }

    let tracked = run_git(workspace, &["ls-files", "--", raw_path])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false);

    if tracked {
        None
    } else {
        Some(MissingDeliverable {
            id: entry.id.clone(),
            what: format!("{raw_path} 未纳入 git 索引"),
            evidence: "untracked".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &std::path::Path, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn path_deliverable_detects_missing_file() {
        let dir = std::env::temp_dir().join(format!("lht-audit-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let entry = CompletionGateDeliverableEntry {
            id: "gzip".into(),
            path: Some("middleware/gzip.go".into()),
            glob: None,
            optional_verify_cmd: None,
            tracked: false,
        };
        let result = audit_deliverables(&dir, &[entry], true, &[], 1);
        assert!(!result.pass);
        assert_eq!(result.missing_deliverables.len(), 1);
        assert_eq!(result.missing_deliverables[0].id, "gzip");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn path_deliverable_passes_when_file_exists() {
        let dir = std::env::temp_dir().join(format!("lht-audit-ok-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir.join("pkg"));
        std::fs::write(dir.join("pkg/main.go"), b"package pkg\n").unwrap();
        let entry = CompletionGateDeliverableEntry {
            id: "main".into(),
            path: Some("pkg/main.go".into()),
            glob: None,
            optional_verify_cmd: None,
            tracked: false,
        };
        let result = audit_deliverables(&dir, &[entry], true, &[], 1);
        assert!(result.pass);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tracked_requires_git_ls_files() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("lht-audit-git-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(git(&dir, &["init"]));
        std::fs::write(dir.join("contracts.go"), b"package c\n").unwrap();
        let entry = CompletionGateDeliverableEntry {
            id: "contracts".into(),
            path: Some("contracts.go".into()),
            glob: None,
            optional_verify_cmd: None,
            tracked: true,
        };
        let result = audit_deliverables(&dir, &[entry.clone()], true, &[], 1);
        assert!(!result.pass);
        assert!(git(&dir, &["add", "contracts.go"]));
        let result2 = audit_deliverables(&dir, &[entry], true, &[], 1);
        assert!(result2.pass);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
