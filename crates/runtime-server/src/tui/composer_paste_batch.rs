//! Coalesce rapid Char+Enter bursts (legacy conhost paste) into one composer blob.

use crossterm::event::{Event, KeyCode, KeyModifiers};

use super::terminal::is_key_press;

/// Minimum printable chars in one frame to treat the burst as a terminal paste.
const MIN_CHARS: u32 = 16;
/// Shorter burst when multiple line breaks arrive in the same batch.
const MIN_CHARS_MULTILINE: u32 = 6;
const MIN_ENTERS_MULTILINE: u32 = 2;
/// Many short lines (e.g. empty lines between bullets).
const MIN_ENTERS_MANY: u32 = 4;
const MIN_CHARS_MANY: u32 = 4;

/// When cmd.exe / conhost injects clipboard text as key events, one `recv` batch
/// often holds the whole paste. Merge into a single multiline composer insert.
pub fn try_coalesce_terminal_paste(batch: &[Event]) -> Option<String> {
    if batch.len() < 2 {
        return None;
    }
    let mut text = String::with_capacity(batch.len());
    let mut char_count = 0u32;
    let mut enter_count = 0u32;

    for event in batch {
        let Event::Key(key) = event else {
            return None;
        };
        if !is_key_press(key) {
            return None;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return None;
        }
        match key.code {
            KeyCode::Char(ch) => {
                char_count = char_count.saturating_add(1);
                text.push(ch);
            }
            KeyCode::Enter => {
                enter_count = enter_count.saturating_add(1);
                text.push('\n');
            }
            _ => return None,
        }
    }

    let looks_like_paste = char_count >= MIN_CHARS
        || (char_count >= MIN_CHARS_MULTILINE && enter_count >= MIN_ENTERS_MULTILINE)
        || (char_count >= MIN_CHARS_MANY && enter_count >= MIN_ENTERS_MANY);
    if looks_like_paste && !text.is_empty() {
        Some(text)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    fn press(code: KeyCode) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    fn char_event(ch: char) -> Event {
        press(KeyCode::Char(ch))
    }

    #[test]
    fn coalesces_multiline_burst() {
        let mut batch = Vec::new();
        for _ in 0..8 {
            batch.push(char_event('a'));
        }
        batch.push(press(KeyCode::Enter));
        for _ in 0..8 {
            batch.push(char_event('b'));
        }
        batch.push(press(KeyCode::Enter));
        let merged = try_coalesce_terminal_paste(&batch).expect("paste");
        assert_eq!(merged, "aaaaaaaa\nbbbbbbbb\n");
    }

    #[test]
    fn ignores_short_manual_line() {
        let batch = vec![char_event('h'), char_event('i'), press(KeyCode::Enter)];
        assert!(try_coalesce_terminal_paste(&batch).is_none());
    }

    #[test]
    fn ignores_mixed_batch() {
        let batch = vec![
            char_event('a'),
            press(KeyCode::Enter),
            Event::Resize(80, 24),
        ];
        assert!(try_coalesce_terminal_paste(&batch).is_none());
    }
}
