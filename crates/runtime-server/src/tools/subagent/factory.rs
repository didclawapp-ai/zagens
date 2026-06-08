use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::config::MAX_SUBAGENTS;

use super::constants::{SUBAGENT_STATE_FILE, ZOMBIE_SCAN_INTERVAL};
use super::manager::SubAgentManager;

/// Thread-safe wrapper for `SubAgentManager`.
pub type SharedSubAgentManager = Arc<RwLock<SubAgentManager>>;

use zagens_config::workspace_meta_file_write;

pub(crate) fn default_state_path(workspace: &Path) -> PathBuf {
    workspace_meta_file_write(workspace, &format!("state/{}", SUBAGENT_STATE_FILE))
}

pub(super) fn epoch_millis_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis()).unwrap_or(u64::MAX),
        Err(_) => 0,
    }
}

pub(super) fn instant_from_duration(duration: Duration) -> Instant {
    Instant::now()
        .checked_sub(duration)
        .unwrap_or_else(Instant::now)
}

pub(super) fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(value)?;
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, payload)?;
    fs::rename(tmp_path, path)?;
    Ok(())
}

/// Create a shared sub-agent manager with a configurable limit.
#[must_use]
pub fn new_shared_subagent_manager(
    workspace: PathBuf,
    max_agents: usize,
    heartbeat_timeout: Duration,
) -> SharedSubAgentManager {
    let max_agents = max_agents.clamp(1, MAX_SUBAGENTS);
    let state_path = default_state_path(&workspace);
    let mut manager = SubAgentManager::new(workspace, max_agents)
        .with_heartbeat_timeout(heartbeat_timeout)
        .with_state_path(state_path);
    if let Err(err) = manager.load_state() {
        eprintln!("Failed to load sub-agent state: {err}");
    }
    Arc::new(RwLock::new(manager))
}

/// Background zombie scan (P2-10): correct `Running` agents whose task finished
/// without a status update, even when nothing calls `agent_list`.
///
/// `Engine::new` is a **synchronous** constructor that can be called outside a
/// Tokio runtime — unit tests build an `Engine` from a plain `#[test]`, and a
/// host may construct the engine before entering its async runtime. Bare
/// `tokio::spawn` panics with "there is no reactor running" in that case, which
/// would abort engine construction. This maintenance loop is best-effort
/// background hygiene, so when no runtime is present we skip spawning it rather
/// than panic; the sidecar always builds the engine inside its Tokio runtime,
/// so production behavior is unchanged.
pub fn spawn_subagent_maintenance_task(manager: SharedSubAgentManager) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(ZOMBIE_SCAN_INTERVAL).await;
            let mut mgr = manager.write().await;
            mgr.run_maintenance();
        }
    });
}
