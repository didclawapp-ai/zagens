//! Detect terminal multiline paste that arrives as char bursts + Enter (no bracketed paste).

use std::time::{Duration, Instant};

const CHAR_GAP: Duration = Duration::from_millis(120);
const ENTER_AFTER_CHAR: Duration = Duration::from_millis(150);
const ARMED_WINDOW: Duration = Duration::from_millis(1200);
const SESSION_IDLE: Duration = Duration::from_millis(350);
const PASTE_ARM_MIN_CHARS: u32 = 4;
const PASTE_EXTEND: Duration = Duration::from_millis(500);
const PASTE_MAX: Duration = Duration::from_secs(30);

/// Tracks rapid character bursts so Enter inserts `\n` instead of sending mid-paste.
#[derive(Debug, Clone, Default)]
pub struct ComposerPasteGuard {
    burst_chars: u32,
    last_char_at: Option<Instant>,
    armed_until: Option<Instant>,
    session_chars: u32,
    last_event_at: Option<Instant>,
    paste_until: Option<Instant>,
    paste_started_at: Option<Instant>,
}

impl ComposerPasteGuard {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn paste_active(&self) -> bool {
        let now = Instant::now();
        self.paste_until.is_some_and(|until| now < until)
    }

    pub fn note_char(&mut self) {
        let now = Instant::now();
        self.end_idle_session(now);
        if let Some(last) = self.last_char_at
            && now.duration_since(last) > CHAR_GAP
        {
            self.burst_chars = 0;
        }
        self.burst_chars = self.burst_chars.saturating_add(1);
        self.last_char_at = Some(now);
        self.session_chars = self.session_chars.saturating_add(1);
        self.last_event_at = Some(now);
        if self.session_chars >= PASTE_ARM_MIN_CHARS {
            self.arm_paste_session(now);
        }
    }

    pub fn note_paste_blob(&mut self) {
        self.reset();
    }

    pub fn note_manual_newline(&mut self) {
        self.arm_from(Instant::now());
    }

    pub fn note_send(&mut self) {
        if self.paste_until.is_some_and(|u| Instant::now() < u) {
            return;
        }
        self.reset();
    }

    /// True when Enter should insert a newline (terminal paste line break).
    pub fn enter_inserts_newline(&self, now: Instant) -> bool {
        if self.paste_until.is_some_and(|until| now < until) {
            return true;
        }
        if self.armed_until.is_some_and(|until| now < until) {
            return true;
        }
        matches!(
            (self.last_char_at, self.burst_chars),
            (Some(last), n) if n >= 1 && now.duration_since(last) < ENTER_AFTER_CHAR
        )
    }

    pub fn note_enter_as_newline(&mut self, now: Instant) {
        self.burst_chars = 0;
        self.arm_from(now);
        if self.paste_until.is_some() {
            self.extend_paste_session(now);
        }
    }

    fn arm_from(&mut self, now: Instant) {
        let until = now + ARMED_WINDOW;
        self.armed_until = Some(
            self.armed_until
                .map(|prev| prev.max(until))
                .unwrap_or(until),
        );
    }

    fn arm_paste_session(&mut self, now: Instant) {
        let started = *self.paste_started_at.get_or_insert(now);
        let max_until = started + PASTE_MAX;
        let extend_until = now + PASTE_EXTEND;
        let until = extend_until.min(max_until);
        self.paste_until = Some(
            self.paste_until
                .map(|prev| prev.max(until))
                .unwrap_or(until),
        );
    }

    fn extend_paste_session(&mut self, now: Instant) {
        self.arm_paste_session(now);
    }

    fn end_idle_session(&mut self, now: Instant) {
        let Some(last) = self.last_event_at else {
            return;
        };
        if now.duration_since(last) <= SESSION_IDLE {
            return;
        }
        self.session_chars = 0;
        self.paste_started_at = None;
        if self.paste_until.is_some_and(|until| now >= until) {
            self.paste_until = None;
        }
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
        assert!(!g.enter_inserts_newline(t0 + Duration::from_millis(400)));
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

    #[test]
    fn paste_session_survives_slow_char_gaps() {
        let mut g = ComposerPasteGuard::default();
        let t0 = Instant::now();
        for _ in 0..6 {
            g.note_char();
        }
        assert!(g.enter_inserts_newline(t0 + Duration::from_millis(80)));
        g.note_enter_as_newline(t0 + Duration::from_millis(80));
        assert!(g.enter_inserts_newline(t0 + Duration::from_millis(200)));
    }

    #[test]
    fn single_char_line_during_paste_session_is_newline() {
        let mut g = ComposerPasteGuard::default();
        let t0 = Instant::now();
        for _ in 0..8 {
            g.note_char();
        }
        g.note_enter_as_newline(t0);
        g.burst_chars = 1;
        g.last_char_at = Some(t0 + Duration::from_millis(60));
        assert!(g.enter_inserts_newline(t0 + Duration::from_millis(100)));
    }
}
