//! In-memory session allowlist for agent external https hosts (sidecar process lifetime).
//!
//! Mirrors desktop `BrowserHosts::session_allowlist`; populated after a successful
//! `browser_navigate` so repeat navigations in the same sidecar session skip approval.

use std::collections::HashSet;
use std::sync::Mutex;

use zagens_browser_policy::normalize_host;

static SESSION_HOSTS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

fn session_hosts() -> std::sync::MutexGuard<'static, Option<HashSet<String>>> {
    SESSION_HOSTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Record a host allowed for the remainder of this sidecar process (after successful navigate).
pub fn remember_session_host(host: &str) {
    let host = normalize_host(host);
    if host.is_empty() {
        return;
    }
    let mut guard = session_hosts();
    let set = guard.get_or_insert_with(HashSet::new);
    set.insert(host);
}

pub fn session_host_allowed(host: &str) -> bool {
    let host = normalize_host(host);
    session_hosts()
        .as_ref()
        .is_some_and(|set| set.contains(&host))
}

/// Merge persistent (from prefs.json) + in-process session hosts for approval checks.
pub fn merged_allowlist(persistent: &[String]) -> Vec<String> {
    let mut out: HashSet<String> = persistent
        .iter()
        .map(|h| normalize_host(h))
        .filter(|h| !h.is_empty())
        .collect();
    if let Some(session) = session_hosts().as_ref() {
        out.extend(session.iter().cloned());
    }
    let mut v: Vec<String> = out.into_iter().collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_hosts_merge_and_persist_in_process() {
        remember_session_host("Example.COM");
        assert!(session_host_allowed("example.com"));
        let merged = merged_allowlist(&["persisted.test".into()]);
        assert!(merged.iter().any(|h| h == "example.com"));
        assert!(merged.iter().any(|h| h == "persisted.test"));
    }
}
