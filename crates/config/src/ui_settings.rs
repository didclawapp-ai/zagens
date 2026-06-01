//! Minimal `settings.toml` access for the Zagens desktop shell.
//!
//! Full schema lives in `deepseek-runtime-server`; here we only need a stable
//! path and read/write of the `locale` field that drives model output language.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::default_config_path;

/// Resolve `settings.toml` beside the active config file (`~/.zagens/config.toml`
/// by default, or sibling to `ZAGENS_CONFIG_PATH` / `DEEPSEEK_CONFIG_PATH`).
pub fn settings_path() -> Result<PathBuf> {
    if let Some(path) = env_config_path() {
        if let Some(parent) = path.parent() {
            return Ok(parent.join("settings.toml"));
        }
    }
    Ok(default_config_path()?.with_file_name("settings.toml"))
}

fn env_config_path() -> Option<PathBuf> {
    for key in ["ZAGENS_CONFIG_PATH", "DEEPSEEK_CONFIG_PATH"] {
        if let Ok(raw) = std::env::var(key) {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Some(expand_home_path(trimmed));
            }
        }
    }
    None
}

fn expand_home_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

fn normalize_locale_input(input: &str) -> String {
    input
        .split('.')
        .next()
        .unwrap_or(input)
        .split('@')
        .next()
        .unwrap_or(input)
        .trim()
        .replace('_', "-")
        .to_lowercase()
}

/// Normalize a configured locale tag. Returns `None` for unsupported values.
pub fn normalize_configured_locale(input: &str) -> Option<&'static str> {
    let normalized = normalize_locale_input(input);
    if matches!(normalized.as_str(), "" | "auto" | "system") {
        return Some("auto");
    }
    if normalized == "c" || normalized == "posix" || normalized.starts_with("en") {
        return Some("en");
    }
    if normalized.starts_with("ja") {
        return Some("ja");
    }
    if normalized.starts_with("zh") {
        if normalized.contains("hant")
            || normalized.contains("-tw")
            || normalized.contains("-hk")
            || normalized.contains("-mo")
        {
            return None;
        }
        return Some("zh-Hans");
    }
    if normalized.starts_with("pt") || normalized == "br" {
        return Some("pt-BR");
    }
    None
}
/// Read the raw `locale` field from disk (`auto` when absent).
pub fn read_locale_setting() -> Result<String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok("auto".to_string());
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings from {}", path.display()))?;
    let doc: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse settings from {}", path.display()))?;
    Ok(doc
        .get("locale")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "auto".to_string()))
}

/// Persist an explicit UI locale (`en`, `ja`, `zh-Hans`, `pt-BR`) for model output.
pub fn write_locale_setting(locale: &str) -> Result<()> {
    let Some(normalized) = normalize_configured_locale(locale) else {
        bail!(
            "invalid locale '{locale}'. Expected: auto, en, ja, zh-Hans, pt-BR."
        );
    };
    if normalized == "auto" {
        bail!("Zagens UI locale sync requires an explicit locale tag.");
    }

    let path = settings_path()?;
    ensure_parent(&path)?;

    let mut doc = if path.exists() {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read settings from {}", path.display()))?;
        toml::from_str::<toml::Value>(&content)
            .with_context(|| format!("Failed to parse settings from {}", path.display()))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = doc
        .as_table_mut()
        .context("settings.toml root must be a table")?;
    table.insert(
        "locale".to_string(),
        toml::Value::String(normalized.to_string()),
    );

    let serialized = toml::to_string_pretty(&doc).context("Failed to serialize settings")?;
    fs::write(&path, serialized)
        .with_context(|| format!("Failed to write settings to {}", path.display()))?;
    Ok(())
}

/// Read the `lht_strict` flag from disk (`false` when absent). Drives the
/// composer LHT (strict long-horizon) toggle; read live by the sidecar engine
/// spawn so the switch takes effect on the next turn without a restart.
pub fn read_lht_strict_setting() -> Result<bool> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(false);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings from {}", path.display()))?;
    let doc: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse settings from {}", path.display()))?;
    Ok(doc
        .get("lht_strict")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false))
}

/// Persist the `lht_strict` flag (composer LHT strict-mode toggle).
pub fn write_lht_strict_setting(enabled: bool) -> Result<()> {
    let path = settings_path()?;
    ensure_parent(&path)?;

    let mut doc = if path.exists() {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read settings from {}", path.display()))?;
        toml::from_str::<toml::Value>(&content)
            .with_context(|| format!("Failed to parse settings from {}", path.display()))?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let table = doc
        .as_table_mut()
        .context("settings.toml root must be a table")?;
    table.insert("lht_strict".to_string(), toml::Value::Boolean(enabled));

    let serialized = toml::to_string_pretty(&doc).context("Failed to serialize settings")?;
    fs::write(&path, serialized)
        .with_context(|| format!("Failed to write settings to {}", path.display()))?;
    Ok(())
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create settings directory {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_locale_tags() {
        assert_eq!(normalize_configured_locale("auto"), Some("auto"));
        assert_eq!(normalize_configured_locale("en_US.UTF-8"), Some("en"));
        assert_eq!(normalize_configured_locale("zh-CN"), Some("zh-Hans"));
        assert_eq!(normalize_configured_locale("pt"), Some("pt-BR"));
        assert_eq!(normalize_configured_locale("ar"), None);
    }
}
