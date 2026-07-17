//! Persist Browser session prefs (allowlist / LAN) under the user data dir.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

fn prefs_path() -> Option<PathBuf> {
    let base = dirs::data_dir()?;
    Some(
        base.join("zagens")
            .join("browser-profile")
            .join("prefs.json"),
    )
}

pub fn load() -> PersistedBrowserPrefs {
    let Some(path) = prefs_path() else {
        return PersistedBrowserPrefs::default();
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return PersistedBrowserPrefs::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
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
