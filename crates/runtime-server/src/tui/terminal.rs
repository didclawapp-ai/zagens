//! Terminal setup and teardown.

use std::io::{Stdout, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::cursor::{Hide, Show};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
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
