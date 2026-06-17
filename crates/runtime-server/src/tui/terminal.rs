//! Terminal setup and teardown.

use std::io::{Stdout, Write, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::prelude::*;

pub struct TerminalGuard {
    pub inline_mode: bool,
    pub mouse_capture: bool,
}

impl TerminalGuard {
    pub fn enter(inline_mode: bool, mouse_capture: bool) -> Result<Self> {
        enable_raw_mode()?;
        let mut out = stdout();
        out.execute(Hide)?;
        if !inline_mode {
            out.execute(EnterAlternateScreen)?;
        }
        if mouse_capture {
            out.execute(EnableMouseCapture)?;
        }
        // Multiline clipboard paste arrives as `Event::Paste` instead of per-char keys
        // (without this, Windows injects `\r` as Enter and only the first line sticks).
        out.execute(EnableBracketedPaste)?;
        out.flush()?;
        Ok(Self {
            inline_mode,
            mouse_capture,
        })
    }

    pub fn leave(&self) -> Result<()> {
        let mut out = stdout();
        if self.mouse_capture {
            let _ = out.execute(DisableMouseCapture);
        }
        let _ = out.execute(DisableBracketedPaste);
        if !self.inline_mode {
            let _ = out.execute(LeaveAlternateScreen);
        }
        let _ = out.execute(Show);
        disable_raw_mode()?;
        Ok(())
    }
}

pub struct TuiTerminal {
    pub terminal: Terminal<CrosstermBackend<Stdout>>,
    pub guard: TerminalGuard,
    _stderr_log: super::stderr_log::StderrLogGuard,
}

impl TuiTerminal {
    pub fn new(inline_mode: bool, mouse_capture: bool) -> Result<Self> {
        let _stderr_log = super::stderr_log::StderrLogGuard::install()?;
        let guard = TerminalGuard::enter(inline_mode, mouse_capture)?;
        let backend = CrosstermBackend::new(stdout());
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self {
            terminal,
            guard,
            _stderr_log,
        })
    }

    pub fn shutdown(mut self) -> Result<()> {
        self.terminal.show_cursor()?;
        self.guard.leave()
    }
}

/// Sync layout with the live terminal size and drain queued resize events.
///
/// On Windows the first `Resize` may arrive only after the next input event; without
/// this pass the initial paint can leave stale cells until the user interacts.
pub fn sync_terminal_geometry(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    layout: &mut super::layout::LayoutEngine,
) -> Result<()> {
    let (cols, rows) = terminal::size()?;
    layout.last_terminal_width = cols;
    layout.apply_auto_collapse(cols);
    terminal.resize(ratatui::layout::Rect::new(0, 0, cols, rows))?;

    while event::poll(Duration::ZERO)? {
        if let Event::Resize(w, h) = event::read()? {
            layout.last_terminal_width = w;
            layout.apply_auto_collapse(w);
            terminal.resize(ratatui::layout::Rect::new(0, 0, w, h))?;
        }
    }
    Ok(())
}

pub fn poll_event(timeout: Duration) -> Result<Option<Event>> {
    if event::poll(timeout)? {
        Ok(Some(event::read()?))
    } else {
        Ok(None)
    }
}

/// Crossterm on Windows emits separate press and release key events; handle press only.
pub fn is_key_press(key: &KeyEvent) -> bool {
    matches!(key.kind, KeyEventKind::Press)
}

pub fn is_ctrl_c(event: &Event) -> bool {
    matches!(
        event,
        Event::Key(key)
            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
    )
}

/// True when Enter should insert a newline (Shift+Enter) rather than send / activate.
pub fn enter_inserts_newline(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::SHIFT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn shift_enter_inserts_newline() {
        let key = press(KeyCode::Enter, KeyModifiers::SHIFT);
        assert!(enter_inserts_newline(&key));
    }

    #[test]
    fn plain_enter_sends() {
        let key = press(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!enter_inserts_newline(&key));
    }
}
