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
    cli.workspace
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
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
            deepseek_config::user_data_path("config.toml")
                .unwrap_or_else(|_| PathBuf::from("config.toml"))
        })
}

pub fn display_path(path: &Path) -> String {
    crate::utils::display_path(path)
}
