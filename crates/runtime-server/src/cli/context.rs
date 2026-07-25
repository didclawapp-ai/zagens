//! CLI config / workspace resolution shared by dispatch and handlers.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::args::Cli;
use super::setup::merge_project_config;
use crate::config::Config;

#[derive(Debug, Clone)]
pub struct CliContext {
    pub config: Config,
    pub workspace: PathBuf,
}

pub fn resolve_workspace(cli: &Cli) -> PathBuf {
    if let Some(w) = &cli.workspace {
        return w.clone();
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    // When the binary is run from a build output directory (e.g. `target/debug/`),
    // walk up to find the real project root so the file tree shows source code instead
    // of build artifacts.
    escape_build_dir(&cwd).unwrap_or(cwd)
}

/// If `path` is inside a `target/` build directory, return the nearest ancestor
/// that contains a project root marker (`Cargo.toml`, `.git`, `package.json`).
/// Returns `None` when the path does not appear to be inside a build directory.
fn escape_build_dir(path: &Path) -> Option<PathBuf> {
    let in_target = path.components().any(|c| c.as_os_str() == "target");
    if !in_target {
        return None;
    }
    let mut candidate = path.to_path_buf();
    loop {
        let parent = candidate.parent()?;
        if parent == candidate {
            return None;
        }
        let looks_like_root = parent.join("Cargo.toml").exists()
            || parent.join(".git").exists()
            || parent.join("package.json").exists();
        if looks_like_root {
            return Some(parent.to_path_buf());
        }
        candidate = parent.to_path_buf();
    }
}

pub fn load_cli_context(cli: &Cli) -> Result<CliContext> {
    let profile = cli
        .profile
        .clone()
        .or_else(|| std::env::var("DEEPSEEK_PROFILE").ok());
    let mut config = Config::load(cli.config.clone(), profile.as_deref())?;
    cli.feature_toggles.apply(&mut config)?;
    let workspace = resolve_workspace(cli);
    if !cli.no_project_config {
        merge_project_config(&mut config, &workspace);
    }
    config.apply_agent_shell_runtime();
    Ok(CliContext { config, workspace })
}

pub fn config_path_for_report(cli: &Cli) -> PathBuf {
    cli.config.clone().unwrap_or_else(default_config_path)
}

pub fn default_config_path() -> PathBuf {
    std::env::var("DEEPSEEK_CONFIG_PATH")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            zagens_config::user_data_path("config.toml")
                .unwrap_or_else(|_| PathBuf::from("config.toml"))
        })
}

pub fn display_path(path: &Path) -> String {
    crate::utils::display_path(path)
}
