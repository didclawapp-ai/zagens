//! `/api-key`, `/login`, and `/logout` slash commands.

use anyhow::Result;

use crate::cli::context::CliContext;
use crate::config::{clear_api_key, save_api_key};
use crate::localization::{MessageId, tr};

use super::app::AppState;

pub fn is_clear_arg(arg: &str) -> bool {
    matches!(
        arg.trim().to_ascii_lowercase().as_str(),
        "clear" | "remove" | "delete" | "unset" | "logout"
    )
}

pub fn apply_save_api_key(ctx: &mut CliContext, app: &mut AppState, key: &str) -> Result<()> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        app.push_system_line(tr(app.locale, MessageId::TuiApiKeyUsage).to_string());
        return Ok(());
    }
    match save_api_key(trimmed) {
        Ok(saved) => {
            ctx.config.api_key = Some(trimmed.to_string());
            app.push_system_line(format!(
                "{} ({})",
                tr(app.locale, MessageId::TuiOnboardingKeySaved),
                saved.describe()
            ));
        }
        Err(err) => app.push_system_line(format!("api-key: {err:#}")),
    }
    Ok(())
}

pub fn apply_clear_api_key(ctx: &mut CliContext, app: &mut AppState) -> Result<()> {
    match clear_api_key() {
        Ok(()) => {
            ctx.config.api_key = None;
            app.push_system_line(tr(app.locale, MessageId::TuiApiKeyCleared).to_string());
        }
        Err(err) => app.push_system_line(format!("api-key: {err:#}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_arg_aliases() {
        assert!(is_clear_arg("clear"));
        assert!(is_clear_arg("REMOVE"));
        assert!(is_clear_arg(" logout "));
        assert!(!is_clear_arg("sk-test"));
    }
}
