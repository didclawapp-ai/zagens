//! Multi-line composer with cursor editing and prompt history browse.

use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};

const HISTORY_CAP: usize = 100;
/// Total composer buffer cap (chars); per-paste cap is separate in `composer_paste.rs`.
pub const MAX_COMPOSER_LEN: usize = 131_072;

/// Editable prompt buffer (`cursor` is a UTF-8 byte index on a char boundary).
#[derive(Debug, Clone, Default)]
pub struct ComposerEditor {
    text: String,
    cursor: usize,
}

impl ComposerEditor {
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn text_mut(&mut self) -> &mut String {
        &mut self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn has_room_for(&self, additional_chars: usize) -> bool {
        self.text.chars().count().saturating_add(additional_chars) <= MAX_COMPOSER_LEN
    }

    pub fn insert_char(&mut self, ch: char) {
        if ch == '\r' {
            return;
        }
        if !self.has_room_for(1) {
            return;
        }
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        let new_chars = s.chars().count();
        if !self.has_room_for(new_chars) {
            return;
        }
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn delete_backward(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = prev_char_boundary(&self.text, self.cursor);
        self.text.drain(prev..self.cursor);
        self.cursor = prev;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = next_char_boundary(&self.text, self.cursor);
        self.text.drain(self.cursor..next);
        true
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = prev_char_boundary(&self.text, self.cursor);
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = next_char_boundary(&self.text, self.cursor);
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Returns `true` if the cursor sits on the first line.
    pub fn on_first_line(&self) -> bool {
        !self.text[..self.cursor].contains('\n')
    }

    /// Returns `true` if the cursor sits on the last line.
    pub fn on_last_line(&self) -> bool {
        !self.text[self.cursor..].contains('\n')
    }

    /// Move cursor up one line, preserving column (byte offset from line start).
    /// Returns `false` when already on the first line.
    pub fn move_up_line(&mut self) -> bool {
        let before = &self.text[..self.cursor];
        let Some(cur_nl) = before.rfind('\n') else {
            return false;
        };
        let cur_line_start = cur_nl + 1;
        let col = self.cursor - cur_line_start;

        let prev_line_end = cur_nl;
        let prev_line_start = self.text[..prev_line_end]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let prev_line_len = prev_line_end - prev_line_start;

        let mut new = prev_line_start + col.min(prev_line_len);
        while new > prev_line_start && !self.text.is_char_boundary(new) {
            new -= 1;
        }
        self.cursor = new;
        true
    }

    /// Move cursor down one line, preserving column (byte offset from line start).
    /// Returns `false` when already on the last line.
    pub fn move_down_line(&mut self) -> bool {
        let rest = &self.text[self.cursor..];
        let Some(nl_rel) = rest.find('\n') else {
            return false;
        };
        let next_line_start = self.cursor + nl_rel + 1;

        let cur_line_start = self.text[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let col = self.cursor - cur_line_start;

        let next_line_end = self.text[next_line_start..]
            .find('\n')
            .map(|i| next_line_start + i)
            .unwrap_or(self.text.len());
        let next_line_len = next_line_end - next_line_start;

        let mut new = next_line_start + col.min(next_line_len);
        while new > next_line_start && !self.text.is_char_boundary(new) {
            new -= 1;
        }
        self.cursor = new;
        true
    }

    pub fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end();
        if trimmed.len() == before.len() {
            if let Some(i) = trimmed
                .char_indices()
                .rev()
                .skip_while(|(_, c)| c.is_whitespace())
                .find(|(_, c)| c.is_whitespace())
                .map(|(i, _)| i + 1)
            {
                self.cursor = i;
            } else {
                self.cursor = 0;
            }
        } else {
            self.cursor = trimmed.len();
        }
    }

    pub fn delete_word_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = {
            let before = &self.text[..self.cursor];
            let trimmed = before.trim_end();
            if trimmed.len() == before.len() {
                trimmed
                    .char_indices()
                    .rev()
                    .skip_while(|(_, c)| c.is_whitespace())
                    .find(|(_, c)| c.is_whitespace())
                    .map(|(i, c)| i + c.len_utf8())
                    .unwrap_or(0)
            } else {
                trimmed.len()
            }
        };
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn delete_to_start(&mut self) {
        self.text.drain(0..self.cursor);
        self.cursor = 0;
    }

    /// Replace buffer (e.g. history browse) and place cursor at end.
    pub fn set_text(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
    }
}

impl Deref for ComposerEditor {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl DerefMut for ComposerEditor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.text
    }
}

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    s[..pos]
        .char_indices()
        .next_back()
        .map(|(i, _)| i)
        .unwrap_or(0)
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    s[pos..]
        .char_indices()
        .nth(1)
        .map(|(i, _)| pos + i)
        .unwrap_or_else(|| s.len())
}

/// Ring buffer of sent prompts (newest last).
#[derive(Debug, Clone, Default)]
pub struct PromptHistory {
    entries: VecDeque<String>,
    browse: Option<usize>,
    draft: Option<String>,
}

impl PromptHistory {
    pub fn push_sent(&mut self, prompt: &str) {
        let prompt = prompt.trim();
        if prompt.is_empty() {
            return;
        }
        if self.entries.back().is_some_and(|last| last == prompt) {
            return;
        }
        self.entries.push_back(prompt.to_string());
        while self.entries.len() > HISTORY_CAP {
            self.entries.pop_front();
        }
        self.browse = None;
        self.draft = None;
    }

    pub fn browsing(&self) -> bool {
        self.browse.is_some()
    }

    pub fn browse_up(&mut self, current: &mut ComposerEditor) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        if self.browse.is_none() {
            self.draft = Some(current.text().to_string());
        }
        let idx = self.browse.unwrap_or(self.entries.len()).saturating_sub(1);
        if idx >= self.entries.len() {
            return false;
        }
        self.browse = Some(idx);
        current.set_text(self.entries[idx].clone());
        true
    }

    pub fn browse_down(&mut self, current: &mut ComposerEditor) -> bool {
        let Some(idx) = self.browse else {
            return false;
        };
        if idx + 1 >= self.entries.len() {
            self.browse = None;
            if let Some(draft) = self.draft.take() {
                current.set_text(draft);
            } else {
                current.clear();
            }
            return true;
        }
        let next = idx + 1;
        self.browse = Some(next);
        current.set_text(self.entries[next].clone());
        true
    }

    pub fn reset_browse(&mut self) {
        self.browse = None;
        self.draft = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_delete_at_cursor() {
        let mut ed = ComposerEditor::default();
        ed.insert_char('a');
        ed.insert_char('b');
        ed.move_left();
        ed.insert_char('X');
        assert_eq!(ed.text(), "aXb");
        assert!(ed.delete_forward());
        assert_eq!(ed.text(), "aX");
    }

    #[test]
    fn move_up_down_multiline() {
        let mut ed = ComposerEditor::default();
        ed.insert_str("hello\nworld\nfoo");
        // cursor at end of "foo" (col 3 on line 2)
        assert!(ed.on_last_line());
        assert!(!ed.on_first_line());

        assert!(ed.move_up_line()); // should land on "world" at col 3
        assert_eq!(&ed.text()[..ed.cursor()], "hello\nwor");

        assert!(ed.move_up_line()); // should land on "hello" at col 3
        assert_eq!(&ed.text()[..ed.cursor()], "hel");
        assert!(ed.on_first_line());
        assert!(!ed.move_up_line()); // already on first line

        assert!(ed.move_down_line()); // back to "world" col 3
        assert_eq!(&ed.text()[..ed.cursor()], "hello\nwor");

        assert!(ed.move_down_line()); // back to "foo" col 3
        assert!(ed.on_last_line());
        assert!(!ed.move_down_line()); // already on last line
    }

    #[test]
    fn move_up_clamps_short_line() {
        let mut ed = ComposerEditor::default();
        ed.insert_str("hi\nworld");
        // cursor at end of "world" (col 5), but "hi" has only 2 chars
        assert!(ed.move_up_line());
        assert_eq!(&ed.text()[..ed.cursor()], "hi"); // clamped to end of "hi"
    }

    #[test]
    fn history_browse_restores_draft() {
        let mut h = PromptHistory::default();
        h.push_sent("first");
        h.push_sent("second");
        let mut ed = ComposerEditor::default();
        ed.insert_str("draft");
        assert!(h.browse_up(&mut ed));
        assert_eq!(ed.text(), "second");
        assert!(h.browse_down(&mut ed));
        assert_eq!(ed.text(), "draft");
    }

    #[test]
    fn insert_stops_at_max_len() {
        let mut ed = ComposerEditor::default();
        ed.insert_str(&"x".repeat(MAX_COMPOSER_LEN));
        assert_eq!(ed.text().chars().count(), MAX_COMPOSER_LEN);
        ed.insert_char('y');
        assert_eq!(ed.text().chars().count(), MAX_COMPOSER_LEN);
    }
}
