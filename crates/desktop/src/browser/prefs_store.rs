//! Persist Browser session prefs (allowlist / LAN / yolo) under `~/.zagens/browser/`.
//!
//! Precedence: `prefs.json` (UI / allow_host) → legacy AppData migrate → seed
//! from `~/.zagens/config.toml` `[browser]` when no prefs file exists yet.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zagens_config::{default_config_path, legacy_config_path, user_data_path};

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

/// Minimal `[browser]` table from user `config.toml` (runtime also parses these keys).
#[derive(Debug, Default, Deserialize)]
struct ConfigBrowserSeed {
    #[serde(default)]
    allow_private_lan: Option<bool>,
    #[serde(default)]
    yolo: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct ConfigFileSeed {
    #[serde(default)]
    browser: Option<ConfigBrowserSeed>,
}

fn read_browser_seed_from_toml(path: &Path) -> Option<ConfigBrowserSeed> {
    let raw = std::fs::read_to_string(path).ok()?;
    let file: ConfigFileSeed = toml::from_str(&raw).ok()?;
    file.browser
}

/// Seed LAN / yolo from `config.toml` when prefs.json has never been written.
fn seed_from_user_config() -> PersistedBrowserPrefs {
    let seed = default_config_path()
        .ok()
        .and_then(|p| read_browser_seed_from_toml(&p))
        .or_else(|| {
            legacy_config_path()
                .ok()
                .and_then(|p| read_browser_seed_from_toml(&p))
        })
        .unwrap_or_default();
    PersistedBrowserPrefs {
        allowlist: Vec::new(),
        allow_private_lan: seed.allow_private_lan.unwrap_or(false),
        yolo: seed.yolo.unwrap_or(false),
    }
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
    seed_from_user_config()
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

    #[test]
    fn seed_parses_browser_table_from_toml() {
        let path = std::env::temp_dir().join(format!(
            "zagens-browser-seed-cfg-{}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            r#"
[browser]
allow_private_lan = true
yolo = true
"#,
        )
        .expect("write");
        let seed = read_browser_seed_from_toml(&path).expect("seed");
        let _ = std::fs::remove_file(&path);
        assert_eq!(seed.allow_private_lan, Some(true));
        assert_eq!(seed.yolo, Some(true));
    }
}
