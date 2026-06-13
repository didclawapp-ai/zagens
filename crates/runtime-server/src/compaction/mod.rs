//! Context compaction for long conversations.

#![allow(
    unused_imports,
    reason = "tests.inc uses `super::*` for compaction helpers"
)]

mod execute;
mod plan;
mod prompt;
mod prune;
mod tokens;

pub use execute::{CompactionResult, compact_messages, compact_messages_safe};
pub use plan::{CompactionPlan, plan_compaction};
pub use prompt::merge_system_prompts;
pub use prune::prune_tool_results;
pub use tokens::{estimate_input_tokens_conservative, estimate_tokens, should_compact};
pub use zagens_core::compaction::{CompactionConfig, MINIMUM_AUTO_COMPACTION_TOKENS};

pub const KEEP_RECENT_MESSAGES: usize = 4;
pub(crate) const RECENT_WORKING_SET_WINDOW: usize = 12;
pub(crate) const MAX_WORKING_SET_PATHS: usize = 24;
pub(crate) const MIN_SUMMARIZE_MESSAGES: usize = 6;
pub(crate) const SUMMARY_TEXT_SNIPPET_CHARS: usize = 800;
pub(crate) const SUMMARY_TOOL_RESULT_SNIPPET_CHARS: usize = 240;
pub(crate) const SUMMARY_INPUT_MAX_CHARS: usize = 24_000;
pub(crate) const SUMMARY_INPUT_HEAD_CHARS: usize = 14_000;
pub(crate) const SUMMARY_INPUT_TAIL_CHARS: usize = 6_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_TEXT_SNIPPET_CHARS: usize = 2_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_TOOL_RESULT_SNIPPET_CHARS: usize = 4_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_INPUT_MAX_CHARS: usize = 120_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_INPUT_HEAD_CHARS: usize = 72_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_INPUT_TAIL_CHARS: usize = 36_000;
pub(crate) const LARGE_CONTEXT_SUMMARY_MAX_TOKENS: u32 = 2_048;
pub(crate) const LARGE_CONTEXT_WINDOW_TOKENS: u32 = 500_000;
pub(crate) const CACHE_ALIGNED_SUMMARY_CONTEXT_BUDGET_PERCENT: usize = 85;

#[cfg(test)]
pub(crate) use execute::{
    anchor_summary_section, build_cache_aligned_summary_request, is_transient_error,
    should_use_cache_aligned_summary, summary_cache_hit_percent, summary_input_limits_for_model,
};
#[cfg(test)]
pub(crate) use plan::{
    enforce_tool_call_pairs, extract_paths_from_text, extract_paths_from_tool_input, message_text,
    normalize_path_candidate,
};
#[cfg(test)]
pub(crate) use prune::truncate_chars;

#[cfg(test)]
include!("tests.inc.rs");
