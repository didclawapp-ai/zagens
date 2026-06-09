//! Resolve sandbox helper executables for setup and elevated spawn.

use std::path::{Path, PathBuf};

use crate::helper_materialization::HelperExecutable;
use crate::helper_materialization::resolve_helper_for_launch;

/// Resolve the elevated command-runner, materializing from the bundle when needed.
pub fn find_runner_exe(zagens_home: &Path, log_dir: Option<&Path>) -> PathBuf {
    resolve_helper_for_launch(HelperExecutable::CommandRunner, zagens_home, log_dir)
}

/// Resolve the elevated setup helper, materializing from the bundle when needed.
pub fn find_setup_exe(zagens_home: &Path, log_dir: Option<&Path>) -> PathBuf {
    resolve_helper_for_launch(HelperExecutable::Setup, zagens_home, log_dir)
}
