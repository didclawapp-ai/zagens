//! Clipboard paste into the Composer input.

const PASTE_MAX_CHARS: usize = 32_768;

/// Normalize clipboard text for the Composer (line endings, length cap).
pub fn normalize_paste_text(raw: &str) -> String {
    let mut normalized = String::with_capacity(raw.len().min(PASTE_MAX_CHARS));
    for ch in raw.chars().take(PASTE_MAX_CHARS) {
        if ch == '\0' {
            continue;
        }
        normalized.push(ch);
    }
    normalized.replace("\r\n", "\n").replace('\r', "\n")
}

pub fn read_clipboard_text() -> Option<String> {
    arboard::Clipboard::new()
        .ok()?
        .get_text()
        .ok()
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_nul_and_unifies_line_endings() {
        assert_eq!(normalize_paste_text("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn normalize_preserves_internal_newlines() {
        assert_eq!(
            normalize_paste_text("line one\nline two\nline three"),
            "line one\nline two\nline three"
        );
    }
}
