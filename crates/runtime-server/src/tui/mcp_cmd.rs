//! `/mcp` slash command — edit `mcp.json` in a TUI overlay.

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::cli::mcp_config::load_mcp_config;
use crate::config::Config;
use crate::localization::{MessageId, tr};
use crate::mcp::{McpConfig, save_config};

use super::app::AppState;
use super::overlay::McpConfigUiState;
use super::session_host::TuiSessionHost;

/// Open the MCP JSON editor overlay, seeding from disk or a minimal template.
pub fn open_mcp_overlay(app: &mut AppState, config: &Config) {
    let path = config.mcp_config_path();
    let text = load_editor_text(&path).unwrap_or_else(|err| {
        app.push_system_line(format!("mcp: {err:#}"));
        default_mcp_template()
    });
    app.mcp_ui = McpConfigUiState::new(path.display().to_string(), text);
    app.show_mcp = true;
}

pub async fn handle_mcp_key(
    key: KeyEvent,
    host: &mut TuiSessionHost,
    app: &mut AppState,
) -> Result<bool> {
    if app.mcp_ui.busy {
        return Ok(false);
    }

    match key.code {
        KeyCode::Esc => {
            close_mcp_overlay(app);
        }
        KeyCode::Tab => {
            app.mcp_ui
                .next_focus(key.modifiers.contains(KeyModifiers::SHIFT));
        }
        KeyCode::BackTab => {
            app.mcp_ui.prev_focus();
        }
        KeyCode::Enter => match app.mcp_ui.focus {
            super::overlay::McpOverlayFocus::Save => {
                save_mcp_overlay(host, app).await?;
            }
            super::overlay::McpOverlayFocus::Cancel => {
                close_mcp_overlay(app);
            }
            super::overlay::McpOverlayFocus::Editor => {
                app.mcp_ui.editor.insert_char('\n');
                app.mcp_ui.error = None;
            }
        },
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            save_mcp_overlay(host, app).await?;
        }
        KeyCode::Backspace => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.delete_backward();
                app.mcp_ui.error = None;
            }
        }
        KeyCode::Delete => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.delete_forward();
                app.mcp_ui.error = None;
            }
        }
        KeyCode::Left => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.move_left();
            }
        }
        KeyCode::Right => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.move_right();
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                if !app.mcp_ui.editor.move_up_line() {
                    app.mcp_ui.scroll = app.mcp_ui.scroll.saturating_sub(1);
                }
            } else {
                app.mcp_ui.prev_focus();
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                if !app.mcp_ui.editor.move_down_line() {
                    app.mcp_ui.scroll = app.mcp_ui.scroll.saturating_add(1);
                }
            } else {
                app.mcp_ui.next_focus(false);
            }
        }
        KeyCode::Home => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.move_home();
            }
        }
        KeyCode::End => {
            if app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor {
                app.mcp_ui.editor.move_end();
            }
        }
        KeyCode::Char(ch)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT)
                && app.mcp_ui.focus == super::overlay::McpOverlayFocus::Editor =>
        {
            app.mcp_ui.editor.insert_char(ch);
            app.mcp_ui.error = None;
        }
        _ => {}
    }
    Ok(false)
}

pub fn mcp_paste(app: &mut AppState, text: &str) {
    if !app.show_mcp || app.mcp_ui.focus != super::overlay::McpOverlayFocus::Editor {
        return;
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    app.mcp_ui.editor.insert_str(&normalized);
    app.mcp_ui.error = None;
}

async fn save_mcp_overlay(host: &mut TuiSessionHost, app: &mut AppState) -> Result<()> {
    let path = host.config().mcp_config_path();
    let raw = app.mcp_ui.editor.text().trim();
    if raw.is_empty() {
        app.mcp_ui.error = Some(tr(app.locale, MessageId::TuiMcpEmptyError).to_string());
        return Ok(());
    }

    let parsed: McpConfig = match serde_json::from_str(raw) {
        Ok(cfg) => cfg,
        Err(e) => {
            app.mcp_ui.error = Some(format!(
                "{}: {e}",
                tr(app.locale, MessageId::TuiMcpParseError)
            ));
            return Ok(());
        }
    };

    app.mcp_ui.busy = true;
    let save_result = save_config(&path, &parsed);
    app.mcp_ui.busy = false;

    if let Err(err) = save_result {
        app.mcp_ui.error = Some(format!("{err:#}"));
        return Ok(());
    }

    if let Err(err) = host.reload_mcp_config().await {
        app.mcp_ui.error = Some(format!("{err:#}"));
        return Ok(());
    }

    let workspace = host.thread.workspace.clone();
    app.inspector.refresh_static(&workspace, host.config());
    app.push_system_line(tr(app.locale, MessageId::TuiMcpSaved).to_string());
    close_mcp_overlay(app);
    Ok(())
}

fn close_mcp_overlay(app: &mut AppState) {
    app.show_mcp = false;
    app.mcp_ui.error = None;
}

fn load_editor_text(path: &std::path::Path) -> Result<String> {
    let cfg = load_mcp_config(path)?;
    if !path.exists() && cfg.servers.is_empty() {
        return Ok(default_mcp_template());
    }
    serde_json::to_string_pretty(&cfg).context("serialize MCP config for editor")
}

fn default_mcp_template() -> String {
    r#"{
  "mcpServers": {
    "example": {
      "command": "node",
      "args": ["./path/to/your-mcp-server.js"],
      "disabled": true
    }
  }
}"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_parses_as_mcp_config() {
        let cfg: McpConfig = serde_json::from_str(&default_mcp_template()).expect("parse");
        assert!(cfg.servers.contains_key("example"));
    }
}
