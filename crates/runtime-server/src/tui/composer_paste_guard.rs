//! Detect terminal multiline paste that arrives as char bursts + Enter (no bracketed paste).

use std::time::{Duration, Instant};

const CHAR_GAP: Duration = Duration::from_millis(45);
const ENTER_AFTER_CHAR: Duration = Duration::from_millis(55);
const ARMED_WINDOW: Duration = Duration::from_millis(900);

/// Tracks rapid character bursts so Enter inserts `\n` instead of sending mid-paste.
#[derive(Debug, Clone, Default)]
pub struct ComposerPasteGuard {
    burst_chars: u32,
    last_char_at: Option<Instant>,
    armed_until: Option<Instant>,
}

impl ComposerPasteGuard {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn note_char(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last_char_at {
            if now.duration_since(last) > CHAR_GAP {
                self.burst_chars = 0;
            }
        }
        self.burst_chars = self.burst_chars.saturating_add(1);
        self.last_char_at = Some(now);
    }

    pub fn note_paste_blob(&mut self) {
        self.reset();
    }

    pub fn note_manual_newline(&mut self) {
        self.arm_from(Instant::now());
    }

    pub fn note_send(&mut self) {
        self.reset();
    }

    /// True when Enter should insert a newline (terminal paste line break).
    pub fn enter_inserts_newline(&self, now: Instant) -> bool {
        if self.armed_until.is_some_and(|until| now < until) {
            return true;
        }
        match (self.last_char_at, self.burst_chars) {
            (Some(last), n) if n >= 2 && now.duration_since(last) < ENTER_AFTER_CHAR => true,
            _ => false,
        }
    }

    pub fn note_enter_as_newline(&mut self, now: Instant) {
        self.burst_chars = 0;
        self.arm_from(now);
    }

    fn arm_from(&mut self, now: Instant) {
        let until = now + ARMED_WINDOW;
        self.armed_until = Some(
            self.armed_until
                .map(|prev| prev.max(until))
                .unwrap_or(until),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_enter_after_burst_is_newline() {
        let mut g = ComposerPasteGuard::default();
        let t0 = Instant::now();
        g.last_char_at = Some(t0);
        g.burst_chars = 5;
        assert!(g.enter_inserts_newline(t0 + Duration::from_millis(20)));
    }

    #[test]
    fn slow_enter_after_typing_is_send() {
        let mut g = ComposerPasteGuard::default();
        let t0 = Instant::now();
        g.last_char_at = Some(t0);
        g.burst_chars = 5;
        assert!(!g.enter_inserts_newline(t0 + Duration::from_millis(200)));
    }

    #[test]
    fn armed_window_covers_short_next_line() {
        let mut g = ComposerPasteGuard::default();
        let t0 = Instant::now();
        g.note_enter_as_newline(t0);
        assert!(g.enter_inserts_newline(t0 + Duration::from_millis(100)));
        g.note_send();
        assert!(!g.enter_inserts_newline(t0 + Duration::from_millis(100)));
    }
}
