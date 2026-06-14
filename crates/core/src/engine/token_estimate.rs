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
//! **Phase 2-B**: [`TokenEstimator`] is the canonical message-level walker.
//! `context.rs` and `compaction/tokens.rs` both delegate to it, so the three
//! reading points (compiler budget, capacity controller, compaction trigger)
//! produce the same numbers for the same input.
//!
//! Approach informed by upstream CodeWhale's `TokenEstimateCache` work (MIT,
//! see `NOTICE.md`); the content-versioned memoization layer is a follow-up
//! that requires a `messages` revision counter on `Session`.

use crate::chat::{ContentBlock, Message, SystemPrompt};

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

// ── TokenEstimator ────────────────────────────────────────────────────────────

/// Framing overhead added per-message at the API wire level
/// (role token + JSON structural bytes). Conservative constant from DeepSeek
/// empirical measurements; tuned against the same corpus as `estimate_text_tokens`.
pub const MESSAGE_FRAMING_TOKENS: usize = 12;

/// Session-level framing overhead (BOS, request JSON scaffolding, etc.).
pub const SESSION_FRAMING_TOKENS: usize = 48;

/// Multiplier applied to raw per-message token sums to account for JSON
/// serialization overhead and role/turn boundaries.  Value: 3/2 = 1.5×.
const MESSAGE_TOKENS_NUMERATOR: usize = 3;
const MESSAGE_TOKENS_DENOMINATOR: usize = 2;

/// Canonical message-level token estimator (kernel-v2 Phase 2-B).
///
/// **Single source of truth** for all three reading points in the request
/// pipeline:
/// 1. `ContextCompiler` budget accounting (`context_compiler.rs`).
/// 2. Capacity-controller input estimate (`context.rs` →
///    `estimate_input_tokens_conservative`).
/// 3. Compaction trigger threshold (`compaction/tokens.rs` →
///    `estimate_input_tokens_conservative`).
///
/// Zero-cost struct: all methods are free functions over `estimate_text_tokens`.
/// Future work: add content-versioned memoization (requires `Session` revision
/// counter, tracked in upstream `NOTICE.md` CodeWhale reference).
#[derive(Debug, Clone, Copy, Default)]
pub struct TokenEstimator;

impl TokenEstimator {
    /// Estimate tokens for a single text fragment.
    ///
    /// Delegates to [`estimate_text_tokens`]; exists so callers can be
    /// parameterised on `TokenEstimator` without importing the free function.
    #[must_use]
    #[inline]
    pub fn estimate_text(self, text: &str) -> usize {
        estimate_text_tokens(text)
    }

    /// Estimate tokens for one content block.
    ///
    /// `include_thinking`: when `true`, the `Thinking` block's token cost is
    /// counted (relevant for context-trim / capacity paths where all historical
    /// reasoning is conservatively counted).  When `false`, `Thinking` blocks
    /// contribute 0 (relevant when reasoning replay depends on message context,
    /// as in the compaction path).
    #[must_use]
    pub fn estimate_block(self, block: &ContentBlock, include_thinking: bool) -> usize {
        match block {
            ContentBlock::Text { text, .. } => estimate_text_tokens(text),
            ContentBlock::Thinking { thinking } => {
                if include_thinking {
                    estimate_text_tokens(thinking)
                } else {
                    0
                }
            }
            ContentBlock::ToolUse { input, .. } => estimate_text_tokens(&input.to_string()),
            ContentBlock::ToolResult { content, .. } => estimate_text_tokens(content),
            ContentBlock::ServerToolUse { input, .. } => estimate_text_tokens(&input.to_string()),
            ContentBlock::ToolSearchToolResult { content, .. } => {
                estimate_text_tokens(&content.to_string())
            }
            ContentBlock::CodeExecutionToolResult { content, .. } => {
                estimate_text_tokens(&content.to_string())
            }
        }
    }

    /// Estimate tokens for one message.
    ///
    /// `include_thinking`: see [`estimate_block`][Self::estimate_block].
    #[must_use]
    pub fn estimate_message(self, message: &Message, include_thinking: bool) -> usize {
        message
            .content
            .iter()
            .map(|block| self.estimate_block(block, include_thinking))
            .sum()
    }

    /// Estimate tokens for a system prompt.
    #[must_use]
    pub fn estimate_system(self, system: Option<&SystemPrompt>) -> usize {
        match system {
            Some(SystemPrompt::Text(text)) => estimate_text_tokens(text),
            Some(SystemPrompt::Blocks(blocks)) => {
                blocks.iter().map(|b| estimate_text_tokens(&b.text)).sum()
            }
            None => 0,
        }
    }

    /// Conservative full-request input token estimate.
    ///
    /// Formula: `ceil(raw_message_tokens × 1.5) + system_tokens + framing`.
    ///
    /// `include_thinking`: passed through to [`estimate_message`][Self::estimate_message]
    /// for every message.  Callers that always include thinking (capacity
    /// controller, context trimmer) pass `true`; callers that replicate
    /// DeepSeek's replay semantics (compaction trigger) pass `false` (or use
    /// [`estimate_request_input_with_selective_thinking`][Self::estimate_request_input_with_selective_thinking]).
    #[must_use]
    pub fn estimate_request_input(
        self,
        messages: &[Message],
        system: Option<&SystemPrompt>,
        include_thinking: bool,
    ) -> usize {
        let raw: usize = messages
            .iter()
            .map(|m| self.estimate_message(m, include_thinking))
            .sum();
        let message_tokens = raw
            .saturating_mul(MESSAGE_TOKENS_NUMERATOR)
            .div_ceil(MESSAGE_TOKENS_DENOMINATOR);
        let system_tokens = self.estimate_system(system);
        let framing = messages
            .len()
            .saturating_mul(MESSAGE_FRAMING_TOKENS)
            .saturating_add(SESSION_FRAMING_TOKENS);
        message_tokens
            .saturating_add(system_tokens)
            .saturating_add(framing)
    }

    /// Conservative full-request estimate using DeepSeek thinking-replay
    /// semantics: include thinking tokens only for messages that contain tool
    /// calls (because those are the only messages whose reasoning is replayed
    /// at inference time).
    ///
    /// Used by the compaction trigger to match actual API input costs.
    #[must_use]
    pub fn estimate_request_input_with_selective_thinking(
        self,
        messages: &[Message],
        system: Option<&SystemPrompt>,
    ) -> usize {
        let raw: usize = messages
            .iter()
            .map(|m| {
                let has_tool_use = m
                    .content
                    .iter()
                    .any(|b| matches!(b, ContentBlock::ToolUse { .. }));
                self.estimate_message(m, has_tool_use)
            })
            .sum();
        let message_tokens = raw
            .saturating_mul(MESSAGE_TOKENS_NUMERATOR)
            .div_ceil(MESSAGE_TOKENS_DENOMINATOR);
        let system_tokens = self.estimate_system(system);
        let framing = messages
            .len()
            .saturating_mul(MESSAGE_FRAMING_TOKENS)
            .saturating_add(SESSION_FRAMING_TOKENS);
        message_tokens
            .saturating_add(system_tokens)
            .saturating_add(framing)
    }
}

// ── free-function helpers ─────────────────────────────────────────────────────

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

    // ── TokenEstimator tests ──────────────────────────────────────────────────

    #[test]
    fn token_estimator_estimate_text_matches_free_fn() {
        let est = TokenEstimator;
        for text in ["hello", "世界", "mixed 中英 text", ""] {
            assert_eq!(est.estimate_text(text), estimate_text_tokens(text));
        }
    }

    #[test]
    fn token_estimator_exclude_thinking_when_flag_false() {
        let est = TokenEstimator;
        let block = ContentBlock::Thinking {
            thinking: "lots of reasoning".to_string(),
        };
        assert_eq!(est.estimate_block(&block, false), 0);
        assert!(est.estimate_block(&block, true) > 0);
    }

    #[test]
    fn token_estimator_request_input_formula() {
        use crate::chat::Message;
        let est = TokenEstimator;
        let messages = vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hello world".to_string(),
                cache_control: None,
            }],
        }];
        let raw = est.estimate_message(&messages[0], true);
        let expected_msg_tokens = raw.saturating_mul(3).div_ceil(2);
        let framing = MESSAGE_FRAMING_TOKENS + SESSION_FRAMING_TOKENS;
        let expected = expected_msg_tokens + framing;
        assert_eq!(est.estimate_request_input(&messages, None, true), expected);
    }

    /// **P2-B golden corpus consistency gate.**
    ///
    /// For text messages with no Thinking blocks, all three reading paths
    /// must agree byte-for-byte (they all delegate to the same
    /// `estimate_text_tokens` implementation through `TokenEstimator`).
    ///
    /// Acceptance criterion: max relative deviation < 1% for every sample.
    /// Currently trivially 0% because they share the same code path.
    #[test]
    fn three_path_consistency_no_thinking_blocks() {
        use crate::chat::Message;

        let est = TokenEstimator;

        // Representative golden corpus: ASCII code, CJK, mixed, empty.
        let samples: &[&str] = &[
            "fn main() { println!(\"Hello, world!\"); }",
            "use std::collections::HashMap;",
            "这是一段中文内容，用于测试 CJK 字符计费。",
            "mixed 中英 content: struct Foo { bar: u32 }",
            "",
            &"x".repeat(1000),
            &"汉".repeat(100),
        ];

        for text in samples {
            let messages = vec![Message {
                role: "user".to_string(),
                content: vec![ContentBlock::Text {
                    text: text.to_string(),
                    cache_control: None,
                }],
            }];

            // Path 1: TokenEstimator (always include_thinking = true,
            //         matches core/context.rs behavior).
            let path1 = est.estimate_request_input(&messages, None, true);

            // Path 2: TokenEstimator with selective thinking
            //         (matches compaction/tokens.rs behavior; no Thinking
            //         blocks present so result must equal path1).
            let path2 = est.estimate_request_input_with_selective_thinking(&messages, None);

            // Path 3: TokenEstimator (include_thinking = false, no Thinking
            //         blocks → same as path1 and path2).
            let path3 = est.estimate_request_input(&messages, None, false);

            // Without Thinking blocks all three paths are identical.
            assert_eq!(path1, path2, "path1 vs path2 diverge for: {text:?}");
            assert_eq!(path1, path3, "path1 vs path3 diverge for: {text:?}");

            // Sanity: deviation percentage is 0%.
            let max_val = path1.max(path2).max(path3) as f64;
            let min_val = path1.min(path2).min(path3) as f64;
            if max_val > 0.0 {
                let deviation_pct = (max_val - min_val) / max_val * 100.0;
                assert!(
                    deviation_pct < 1.0,
                    "deviation {deviation_pct:.2}% >= 1% for: {text:?}"
                );
            }
        }
    }
}
