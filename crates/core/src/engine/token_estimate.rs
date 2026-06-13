//! Single text→token estimation entry point (kernel-v2 Tier-1, M0.3).
//!
//! Before this module the workspace carried **two divergent calibrations**:
//!
//! - `crates/core/src/engine/context.rs` — `ceil(chars / 3)` (later ×1.5 at
//!   the message level), blind to CJK density;
//! - `crates/runtime-server/src/compaction/tokens.rs` — DeepSeek doc ratios
//!   (~0.3 token/ASCII char, ~0.6 token/CJK char,
//!   <https://api-docs.deepseek.com/zh-cn/quick_start/token_usage>).
//!
//! UI usage, capacity ratios, and compaction thresholds read different
//! numbers depending on which side they queried. [`estimate_text_tokens`] is
//! now the only text-level calibration in the workspace; both message-level
//! walkers consume it. The estimate is the **conservative union** of the two
//! legacy calibrations (never lower than either), so consumers only become
//! more cautious, never less.
//!
//! Approach informed by upstream CodeWhale's `TokenEstimateCache` work (MIT,
//! see `NOTICE.md`); the content-versioned memoization layer is a follow-up
//! that requires a `messages` revision counter on `Session`.

/// Conservative token estimate for a text fragment.
///
/// Single entry point for text→token calibration (M0.3 acceptance gate:
/// every `estimate_text_tokens*` call site in the workspace resolves here).
/// Returns `max(DeepSeek doc ratio, ceil(chars / 3))`.
#[must_use]
pub fn estimate_text_tokens(text: &str) -> usize {
    // Fast path: for pure-ASCII text `chars/3` always dominates the DeepSeek
    // ratio (1/3 > 0.3/char), and `is_ascii` is a vectorized byte scan. This
    // keeps repeated whole-session estimates (context trim, capacity
    // checkpoints) from regressing on large ASCII-heavy histories.
    if text.is_ascii() {
        return text.len().div_ceil(3);
    }
    let (cjk, other) = count_cjk_and_other_chars(text);
    let deepseek_ratio = other
        .saturating_mul(3)
        .div_ceil(10)
        .saturating_add(cjk.saturating_mul(6).div_ceil(10));
    let legacy_chars_third = cjk.saturating_add(other).div_ceil(3);
    deepseek_ratio.max(legacy_chars_third)
}

fn count_cjk_and_other_chars(text: &str) -> (usize, usize) {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk_char(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    (cjk, other)
}

fn is_cjk_char(ch: char) -> bool {
    matches!(
        ch,
        '\u{4e00}'..='\u{9fff}'
            | '\u{3400}'..='\u{4dbf}'
            | '\u{3000}'..='\u{303f}'
            | '\u{ff00}'..='\u{ffef}'
            | '\u{2e80}'..='\u{2fdf}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_zero() {
        assert_eq!(estimate_text_tokens(""), 0);
    }

    #[test]
    fn ascii_uses_chars_third_envelope() {
        // 30 ASCII chars: DeepSeek ratio 9, chars/3 = 10 → envelope picks 10.
        let text = "a".repeat(30);
        assert_eq!(estimate_text_tokens(&text), 10);
    }

    #[test]
    fn cjk_uses_deepseek_envelope() {
        // 10 CJK chars: DeepSeek ratio 6, chars/3 = 4 → envelope picks 6.
        let text = "汉".repeat(10);
        assert_eq!(estimate_text_tokens(&text), 6);
    }

    #[test]
    fn never_below_either_legacy_calibration() {
        for text in [
            "hello world",
            "纯中文内容若干字符",
            "mixed 中英 content with 标点。",
            "fn main() { println!(\"hi\"); }",
        ] {
            let estimate = estimate_text_tokens(text);
            let chars = text.chars().count();
            let (cjk, other) = count_cjk_and_other_chars(text);
            let deepseek =
                other.saturating_mul(3).div_ceil(10) + cjk.saturating_mul(6).div_ceil(10);
            assert!(estimate >= chars.div_ceil(3), "below legacy core: {text}");
            assert!(estimate >= deepseek, "below DeepSeek ratio: {text}");
        }
    }

    #[test]
    fn cjk_classification_covers_fullwidth_punctuation() {
        assert!(is_cjk_char('。'));
        assert!(is_cjk_char('，'));
        assert!(is_cjk_char('汉'));
        assert!(!is_cjk_char('a'));
        assert!(!is_cjk_char(' '));
    }
}
