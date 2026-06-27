//! Minimal `settings.toml` access for the Zagens desktop shell.
//!
//! Full schema lives in `zagens-cli`; here we only need a stable
//! path and read/write of the `locale` field that drives model output language.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::default_config_path;

/// Resolve `settings.toml` beside the active config file (`~/.zagens/config.toml`
/// by default, or sibling to `ZAGENS_CONFIG_PATH` / `DEEPSEEK_CONFIG_PATH`).
pub fn settings_path() -> Result<PathBuf> {
    if let Some(path) = env_config_path()
        && let Some(parent) = path.parent()
    {
        return Ok(parent.join("settings.toml"));
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
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest);
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
        bail!("invalid locale '{locale}'. Expected: auto, en, ja, zh-Hans, pt-BR.");
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

/// Composer LHT tri-state override (`settings.toml` → read live each engine spawn).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LhtComposerMode {
    /// Inherit `[long_horizon]` from `config.toml` (default product: enabled + auto).
    #[default]
    Auto,
    /// Force harness on + strict plan/completion gates.
    Strict,
    /// Force harness off regardless of `config.toml` `enabled`.
    Off,
}

impl LhtComposerMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Strict => "strict",
            Self::Off => "off",
        }
    }

    #[must_use]
    pub fn from_storage(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "strict" => Self::Strict,
            "off" | "disabled" | "false" => Self::Off,
            _ => Self::Auto,
        }
    }

    /// Composer chip cycle: auto → strict → off → auto.
    #[must_use]
    pub fn cycle(self) -> Self {
        match self {
            Self::Auto => Self::Strict,
            Self::Strict => Self::Off,
            Self::Off => Self::Auto,
        }
    }
}

fn parse_lht_composer_mode(doc: &toml::Value) -> LhtComposerMode {
    if let Some(raw) = doc.get("lht_composer_mode").and_then(toml::Value::as_str) {
        return LhtComposerMode::from_storage(raw);
    }
    // Legacy boolean: true → strict; false/absent → auto (not off).
    if doc
        .get("lht_strict")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        LhtComposerMode::Strict
    } else {
        LhtComposerMode::Auto
    }
}

/// Read the composer LHT tri-state from disk (`auto` when absent).
pub fn read_lht_composer_mode_setting() -> Result<LhtComposerMode> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(LhtComposerMode::Auto);
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings from {}", path.display()))?;
    let doc: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse settings from {}", path.display()))?;
    Ok(parse_lht_composer_mode(&doc))
}

/// Persist the composer LHT tri-state. Removes legacy `lht_strict` when writing.
pub fn write_lht_composer_mode_setting(mode: LhtComposerMode) -> Result<()> {
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
    table.remove("lht_strict");
    table.insert(
        "lht_composer_mode".to_string(),
        toml::Value::String(mode.as_str().to_string()),
    );

    let serialized = toml::to_string_pretty(&doc).context("Failed to serialize settings")?;
    fs::write(&path, serialized)
        .with_context(|| format!("Failed to write settings to {}", path.display()))?;

    crate::lht_config::sync_long_horizon_with_composer_mode(mode.as_str())?;
    Ok(())
}

/// Legacy: `true` when composer mode is strict.
pub fn read_lht_strict_setting() -> Result<bool> {
    Ok(read_lht_composer_mode_setting()? == LhtComposerMode::Strict)
}

/// Legacy: `true` → strict; `false` → auto (not off).
pub fn write_lht_strict_setting(enabled: bool) -> Result<()> {
    write_lht_composer_mode_setting(if enabled {
        LhtComposerMode::Strict
    } else {
        LhtComposerMode::Auto
    })
}

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create settings directory {}", parent.display()))?;
    }
    Ok(())
}

fn load_settings_doc() -> Result<(PathBuf, toml::Value)> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok((path, toml::Value::Table(toml::map::Map::new())));
    }
    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings from {}", path.display()))?;
    let doc: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse settings from {}", path.display()))?;
    Ok((path, doc))
}

fn write_settings_doc(path: &Path, doc: &toml::Value) -> Result<()> {
    ensure_parent(path)?;
    let serialized = toml::to_string_pretty(doc).context("Failed to serialize settings")?;
    fs::write(path, serialized)
        .with_context(|| format!("Failed to write settings to {}", path.display()))?;
    Ok(())
}

fn normalize_task_type_preference(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "auto" => Some("auto"),
        "code" => Some("code"),
        "office" => Some("office"),
        _ => None,
    }
}

/// Whether the desktop onboarding wizard (API key + default mode) was completed.
pub fn read_onboarding_complete_setting() -> Result<bool> {
    let (_path, doc) = load_settings_doc()?;
    if doc
        .get("onboarding_complete")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        return Ok(true);
    }
    // Older builds persisted task type without flipping onboarding_complete.
    if doc.get("task_type_preference").is_some() {
        return Ok(true);
    }
    Ok(false)
}

/// Mark onboarding complete after the user finishes the first-run wizard.
pub fn write_onboarding_complete_setting(complete: bool) -> Result<()> {
    let (path, mut doc) = load_settings_doc()?;
    let table = doc
        .as_table_mut()
        .context("settings.toml root must be a table")?;
    table.insert(
        "onboarding_complete".to_string(),
        toml::Value::Boolean(complete),
    );
    write_settings_doc(&path, &doc)
}

/// Read the persisted default task type (`auto` / `code` / `office`).
pub fn read_task_type_preference_setting() -> Result<Option<String>> {
    let (_path, doc) = load_settings_doc()?;
    Ok(doc
        .get("task_type_preference")
        .and_then(toml::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|raw| normalize_task_type_preference(raw).map(str::to_string)))
}

/// Persist the desktop default task type preference.
pub fn write_task_type_preference_setting(value: &str) -> Result<()> {
    let Some(normalized) = normalize_task_type_preference(value) else {
        bail!("invalid task_type_preference '{value}'. Expected: auto, code, office.");
    };
    let (path, mut doc) = load_settings_doc()?;
    let table = doc
        .as_table_mut()
        .context("settings.toml root must be a table")?;
    table.insert(
        "task_type_preference".to_string(),
        toml::Value::String(normalized.to_string()),
    );
    write_settings_doc(&path, &doc)
}

/// Read desktop preference for new-session git worktree isolation.
pub fn read_new_session_use_worktree_setting() -> Result<Option<bool>> {
    let (_path, doc) = load_settings_doc()?;
    Ok(doc
        .get("new_session_use_worktree")
        .and_then(toml::Value::as_bool))
}

/// Persist desktop new-session worktree preference.
pub fn write_new_session_use_worktree_setting(enabled: bool) -> Result<()> {
    let (path, mut doc) = load_settings_doc()?;
    let table = doc
        .as_table_mut()
        .context("settings.toml root must be a table")?;
    table.insert(
        "new_session_use_worktree".to_string(),
        toml::Value::Boolean(enabled),
    );
    write_settings_doc(&path, &doc)
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

    #[test]
    fn task_type_preference_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("zagens-ui-settings-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let prev = std::env::var("ZAGENS_CONFIG_PATH").ok();
        // SAFETY: test-only env override; restored before return.
        unsafe {
            std::env::set_var("ZAGENS_CONFIG_PATH", dir.join("config.toml"));
        }
        write_task_type_preference_setting("office").expect("write");
        assert_eq!(
            read_task_type_preference_setting().expect("read"),
            Some("office".to_string())
        );
        write_onboarding_complete_setting(true).expect("write onboarding");
        assert!(read_onboarding_complete_setting().expect("read onboarding"));
        let (_path, mut doc) = load_settings_doc().expect("load");
        let table = doc.as_table_mut().expect("table");
        table.remove("onboarding_complete");
        write_settings_doc(&settings_path().expect("path"), &doc).expect("write");
        assert!(
            read_onboarding_complete_setting().expect("legacy task type"),
            "task_type_preference alone should imply onboarding complete"
        );
        // SAFETY: restores prior process env for other tests.
        unsafe {
            match prev {
                Some(v) => std::env::set_var("ZAGENS_CONFIG_PATH", v),
                None => std::env::remove_var("ZAGENS_CONFIG_PATH"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_session_use_worktree_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("zagens-ui-settings-wt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let prev = std::env::var("ZAGENS_CONFIG_PATH").ok();
        // SAFETY: test-only env override; restored before return.
        unsafe {
            std::env::set_var("ZAGENS_CONFIG_PATH", dir.join("config.toml"));
        }
        write_new_session_use_worktree_setting(true).expect("write");
        assert_eq!(
            read_new_session_use_worktree_setting().expect("read"),
            Some(true)
        );
        // SAFETY: restores prior process env for other tests.
        unsafe {
            match prev {
                Some(v) => std::env::set_var("ZAGENS_CONFIG_PATH", v),
                None => std::env::remove_var("ZAGENS_CONFIG_PATH"),
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lht_composer_mode_cycle_and_legacy_migration() {
        assert_eq!(LhtComposerMode::Auto.cycle(), LhtComposerMode::Strict);
        assert_eq!(LhtComposerMode::Strict.cycle(), LhtComposerMode::Off);
        assert_eq!(LhtComposerMode::Off.cycle(), LhtComposerMode::Auto);
        let legacy: toml::Value = toml::from_str("lht_strict = true").unwrap();
        assert_eq!(parse_lht_composer_mode(&legacy), LhtComposerMode::Strict);
        let legacy_off: toml::Value = toml::from_str("lht_strict = false").unwrap();
        assert_eq!(parse_lht_composer_mode(&legacy_off), LhtComposerMode::Auto);
        let modern: toml::Value = toml::from_str(r#"lht_composer_mode = "off""#).unwrap();
        assert_eq!(parse_lht_composer_mode(&modern), LhtComposerMode::Off);
    }
}
