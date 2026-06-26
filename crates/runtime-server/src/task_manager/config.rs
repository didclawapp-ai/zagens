use std::path::PathBuf;

use crate::config::{Config, DEFAULT_TEXT_MODEL, MAX_SUBAGENTS};

use super::DEFAULT_WORKERS;

/// Task manager startup options.
#[derive(Debug, Clone)]
pub struct TaskManagerConfig {
    pub data_dir: PathBuf,
    pub worker_count: usize,
    pub default_workspace: PathBuf,
    pub default_model: String,
    pub default_mode: String,
    pub allow_shell: bool,
    pub trust_mode: bool,
    #[allow(dead_code)]
    pub max_subagents: usize,
}

impl TaskManagerConfig {
    #[must_use]
    pub fn from_runtime(
        config: &Config,
        workspace: PathBuf,
        default_model: Option<String>,
        worker_count: Option<usize>,
    ) -> Self {
        Self {
            data_dir: super::persist::default_tasks_dir(),
            worker_count: worker_count.unwrap_or(DEFAULT_WORKERS),
            default_workspace: workspace,
            default_model: default_model.unwrap_or_else(|| {
                config
                    .default_text_model
                    .clone()
                    .unwrap_or_else(|| DEFAULT_TEXT_MODEL.to_string())
            }),
            default_mode: "agent".to_string(),
            allow_shell: config.allow_shell(),
            trust_mode: config.trust_mode(),
            max_subagents: config.max_subagents().clamp(1, MAX_SUBAGENTS),
        }
    }
}
