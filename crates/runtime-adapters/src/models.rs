//! Chat/model re-exports shared by tools and persist layers.

pub use deepseek_core::chat::{
    CacheControl, ContentBlock, ContentBlockStart, Delta, Message, MessageDelta,
    MessageRequest, MessageResponse, StreamEvent, SystemBlock, SystemPrompt, Tool, ToolCaller,
    compaction_threshold_for_model, context_window_for_model,
    DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS, LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS,
};
pub use deepseek_core::models::{ServerToolUsage, Usage};
