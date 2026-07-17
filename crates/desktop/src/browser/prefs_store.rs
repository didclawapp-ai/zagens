//! Persist Browser session prefs (allowlist / LAN / yolo) under `~/.zagens/browser/`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use zagens_config::user_data_path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedBrowserPrefs {
    #[serde(default)]
    pub allowlist: Vec<String>,
    #[serde(default)]
    pub allow_private_lan: bool,
    /// Mirrored for sidecar hot-read (C2); desktop `BrowserHosts` remains source of truth at runtime.
    #[serde(default)]
    pub yolo: bool,
}

/// Canonical path: `~/.zagens/browser/prefs.json`.
pub fn prefs_path() -> Option<PathBuf> {
    user_data_path("browser/prefs.json").ok()
}

/// Pre-0.8.x location under `%APPDATA%/zagens/browser-profile/prefs.json` (Windows Roaming).
fn legacy_prefs_path() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    Some(
        base.join("zagens")
            .join("browser-profile")
            .join("prefs.json"),
    )
}

fn read_prefs_file(path: &PathBuf) -> Option<PersistedBrowserPrefs> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn load() -> PersistedBrowserPrefs {
    if let Some(path) = prefs_path()
        && let Some(prefs) = read_prefs_file(&path)
    {
        return prefs;
    }
    // One-shot migrate from AppData if the new path is missing.
    if let Some(legacy) = legacy_prefs_path()
        && let Some(prefs) = read_prefs_file(&legacy)
    {
        save(&prefs);
        return prefs;
    }
    PersistedBrowserPrefs::default()
}

pub fn save(prefs: &PersistedBrowserPrefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(raw) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(path, raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_path_is_under_dot_zagens_browser() {
        let path = prefs_path().expect("home dir");
        let s = path.to_string_lossy().replace('\\', "/");
        assert!(
            s.ends_with(".zagens/browser/prefs.json"),
            "unexpected prefs path: {s}"
        );
    }
}
