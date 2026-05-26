use anyhow::Result;

static RESIDENT_LEASES: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

/// Release all resident file leases held by `agent_id`. Called when an
/// agent transitions to a terminal state (completed, failed, cancelled).
pub(crate) fn release_resident_leases_for(agent_id: &str) {
    if let Some(lock) = RESIDENT_LEASES.get()
        && let Ok(mut guard) = lock.lock()
    {
        guard.retain(|_, owner| owner != agent_id);
    }
}

/// Claim a resident-file lease. Returns `Err` when another agent already holds
/// the path (hard lock — spawn is rejected).
pub(crate) fn try_claim_resident_file_lease(file_path: &str, owner: &str) -> Result<(), String> {
    let leases = RESIDENT_LEASES
        .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut guard = leases.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(existing) = guard.get(file_path) {
        return Err(format!(
            "resident_file '{file_path}' is already held by agent {existing}; spawn rejected"
        ));
    }
    guard.insert(file_path.to_string(), owner.to_string());
    Ok(())
}

/// Drop a resident-file lease for `file_path` (spawn failure cleanup).
pub(crate) fn release_resident_file_lease(file_path: &str) {
    if let Some(lock) = RESIDENT_LEASES.get()
        && let Ok(mut guard) = lock.lock()
    {
        guard.remove(file_path);
    }
}


/// Replace a pending resident-file lease placeholder with the spawned agent id.
pub(crate) fn upgrade_pending_resident_lease(file_path: &str, agent_id: &str) {
    if let Some(lock) = RESIDENT_LEASES.get()
        && let Ok(mut guard) = lock.lock()
        && let Some(owner) = guard.get_mut(file_path)
        && owner == "pending"
    {
        *owner = agent_id.to_string();
    }
}
