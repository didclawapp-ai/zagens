//! Onboarding key handling and persistence.

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::super::app::AppState;
use super::super::onboarding::OnboardingPhase;
use super::super::session_host::TuiSessionHost;
use crate::cli::context::CliContext;
use crate::config::save_api_key;
use crate::localization::{MessageId, tr};

pub async fn handle_onboarding_key(
    key: KeyEvent,
    ctx: &mut CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<bool> {
    let ui = &mut app.onboarding;
    if ui.busy {
        return Ok(false);
    }

    match key.code {
        KeyCode::Esc if matches!(ui.phase(), Some(OnboardingPhase::ApiKey)) => {
            ui.api_key.clear();
            ui.advance();
            Ok(false)
        }
        KeyCode::Esc => {
            if !ui.is_first_step() {
                ui.back();
            }
            Ok(false)
        }
        KeyCode::Enter => {
            if ui.is_last_step() {
                complete_onboarding(ctx, host, app).await?;
            } else {
                advance_step(ctx, host, app).await?;
            }
            Ok(false)
        }
        KeyCode::Up | KeyCode::Char('k')
            if matches!(ui.phase(), Some(OnboardingPhase::TaskType)) =>
        {
            ui.move_mode_up();
            Ok(false)
        }
        KeyCode::Down | KeyCode::Char('j')
            if matches!(ui.phase(), Some(OnboardingPhase::TaskType)) =>
        {
            ui.move_mode_down();
            Ok(false)
        }
        KeyCode::Backspace => {
            ui.delete_backward();
            Ok(false)
        }
        KeyCode::Char(ch)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            ui.insert_char(ch);
            Ok(false)
        }
        _ => Ok(false),
    }
}

async fn advance_step(
    ctx: &mut CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<()> {
    if matches!(app.onboarding.phase(), Some(OnboardingPhase::ApiKey)) {
        let key = app.onboarding.api_key.text().trim().to_string();
        if !key.is_empty() {
            app.onboarding.busy = true;
            match save_api_key(&key) {
                Ok(saved) => {
                    ctx.config.api_key = Some(key.clone());
                    if let Err(err) = host.sync_runtime_api_key(Some(key)).await {
                        app.onboarding.error = Some(format!("{err:#}"));
                        app.onboarding.busy = false;
                        return Ok(());
                    }
                    app.push_system_line(format!(
                        "{} ({})",
                        tr(app.locale, MessageId::TuiOnboardingKeySaved),
                        saved.describe()
                    ));
                }
                Err(err) => {
                    app.onboarding.error = Some(format!("{err:#}"));
                    app.onboarding.busy = false;
                    return Ok(());
                }
            }
            app.onboarding.busy = false;
        }
    }
    app.onboarding.advance();
    if app.onboarding.phase().is_none() {
        complete_onboarding(ctx, host, app).await?;
    }
    Ok(())
}

async fn complete_onboarding(
    ctx: &mut CliContext,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<()> {
    let has_key_step = app.onboarding.phases.contains(&OnboardingPhase::ApiKey);
    let has_mode_step = app.onboarding.phases.contains(&OnboardingPhase::TaskType);
    let key_text = app.onboarding.api_key.text().trim().to_string();
    let task_type = app.onboarding.selected_task_type();

    if has_key_step && !key_text.is_empty() && ctx.config.api_key.is_none() {
        match save_api_key(&key_text) {
            Ok(saved) => {
                ctx.config.api_key = Some(key_text.clone());
                host.sync_runtime_api_key(Some(key_text))
                    .await
                    .context("sync API key to runtime after onboarding")?;
                app.push_system_line(format!(
                    "{} ({})",
                    tr(app.locale, MessageId::TuiOnboardingKeySaved),
                    saved.describe()
                ));
            }
            Err(err) => {
                app.onboarding.error = Some(format!("{err:#}"));
                return Ok(());
            }
        }
    }
    if has_mode_step {
        zagens_config::write_task_type_preference_setting(task_type)?;
        host.apply_task_type(task_type).await?;
        app.sync_thread_meta(host);
    }
    zagens_config::write_onboarding_complete_setting(true)?;
    app.show_onboarding = false;
    app.push_system_line(tr(app.locale, MessageId::TuiOnboardingComplete).to_string());
    Ok(())
}

pub fn onboarding_paste(app: &mut AppState, text: &str) {
    app.onboarding.paste(text);
}
