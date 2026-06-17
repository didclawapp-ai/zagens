//! `/locale` slash command — persist UI language and refresh TUI chrome.

use anyhow::Result;

use crate::localization::{MessageId, normalize_configured_locale, resolve_locale, tr};
use crate::settings::Settings;

use super::app::AppState;
use super::session_host::TuiSessionHost;

/// Persisted `settings.toml` values (includes `auto`).
pub const LOCALE_STORAGE_TAGS: &[&str] = &["auto", "en", "ja", "zh-Hans", "pt-BR"];

pub fn read_locale_storage() -> String {
    Settings::load()
        .map(|s| s.locale)
        .unwrap_or_else(|_| "auto".to_string())
}

pub fn cycle_locale_storage(current: &str) -> &'static str {
    let idx = LOCALE_STORAGE_TAGS
        .iter()
        .position(|tag| tag.eq_ignore_ascii_case(current))
        .unwrap_or(0);
    LOCALE_STORAGE_TAGS[(idx + 1) % LOCALE_STORAGE_TAGS.len()]
}

pub fn parse_locale_storage_arg(arg: &str) -> Option<String> {
    normalize_configured_locale(arg).map(str::to_string)
}

pub fn filter_locale_tags(composer_arg: &str) -> Vec<&'static str> {
    let query = composer_arg.trim().to_ascii_lowercase();
    LOCALE_STORAGE_TAGS
        .iter()
        .copied()
        .filter(|tag| {
            query.is_empty()
                || tag.to_ascii_lowercase().contains(&query)
                || tag.to_ascii_lowercase().starts_with(&query)
        })
        .collect()
}

pub async fn apply_locale_change(
    app: &mut AppState,
    host: &TuiSessionHost,
    storage_tag: &str,
) -> Result<()> {
    let mut settings = Settings::load().unwrap_or_default();
    settings.set("locale", storage_tag)?;
    settings.save()?;
    app.locale = resolve_locale(storage_tag);
    app.sessions = host
        .workspace_session_list(host.thread_id(), app.locale)
        .await;
    let msg = tr(app.locale, MessageId::TuiLocaleChanged).replace("{locale}", storage_tag);
    app.push_system_line(msg);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_locale_tags_wraps() {
        assert_eq!(cycle_locale_storage("pt-BR"), "auto");
        assert_eq!(cycle_locale_storage("auto"), "en");
    }

    #[test]
    fn parse_locale_arg_normalizes_aliases() {
        assert_eq!(
            parse_locale_storage_arg("zh-CN").as_deref(),
            Some("zh-Hans")
        );
        assert_eq!(parse_locale_storage_arg("auto").as_deref(), Some("auto"));
        assert!(parse_locale_storage_arg("ar").is_none());
    }
}
