//! Context assembly report — per-source token spans from `ContextCompiler` (P2a).

use serde::{Deserialize, Serialize};

use super::context_compiler::{CompiledContext, SourceId};

/// Byte range within a concatenated assembly buffer (optional diagnostic).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByteRange {
    pub start: usize,
    pub end: usize,
}

/// One compiler-registered source contribution with optional text span metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub source_id: String,
    /// Explorer taxonomy bucket (`system`, `tools`, `summarized`, …).
    pub category: String,
    pub tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<ByteRange>,
}

/// Assembly-time report: compiler sources + optional message-layer span.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAssemblyReport {
    pub spans: Vec<SourceSpan>,
    /// Sum of compiler source spans (excludes conversation unless appended).
    pub compiler_source_tokens: u32,
    /// Conversation / message-layer estimate when provided by the caller.
    pub message_tokens: u32,
    /// Conservative total used for Explorer conservation checks.
    pub estimated_input_tokens: u32,
}

impl ContextAssemblyReport {
    /// Clone the compiler-produced report and append the message layer.
    #[must_use]
    pub fn from_compiled(compiled: &CompiledContext) -> Self {
        compiled.assembly_report.clone()
    }

    /// Append the message/conversation layer and refresh conservation total.
    #[must_use]
    pub fn with_message_tokens(mut self, message_tokens: u32) -> Self {
        if message_tokens > 0 {
            self.spans.push(SourceSpan {
                source_id: "conversation.messages".to_string(),
                category: "conversation".to_string(),
                tokens: message_tokens,
                byte_range: None,
            });
        }
        self.message_tokens = message_tokens;
        self.estimated_input_tokens = self.compiler_source_tokens.saturating_add(message_tokens);
        self
    }

    /// Sum of span token counts (should ≈ `estimated_input_tokens`).
    #[must_use]
    pub fn span_token_sum(&self) -> u32 {
        self.spans.iter().map(|s| s.tokens).sum()
    }
}

/// Map compiler source ids to Explorer category labels (§3.5.2).
#[must_use]
pub fn explorer_category_for_source_id(source_id: &SourceId) -> &'static str {
    match source_id.0 {
        "system.static" => "system",
        "tools.catalog" => "tools",
        "memory.compaction" => "summarized",
        "memory.cycle" => "summarized",
        "working_set" => "structured",
        "scratchpad.reminder" | "steer" => "conversation",
        _ => "system",
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{ByteRange, ContextAssemblyReport};
    use crate::engine::context_compiler::{
        BudgetPolicy, ContextCompiler, ContextLayer, ContextProjection, ContextSource,
        RenderedBlock, SourceId,
    };
    use crate::session::Session;
    use std::path::PathBuf;

    fn test_session() -> Session {
        Session::new(
            "deepseek-v4-pro".into(),
            PathBuf::from("/tmp"),
            false,
            false,
            PathBuf::from("/tmp/notes.txt"),
            PathBuf::from("/tmp/mcp.json"),
        )
    }

    #[test]
    fn assembly_report_categories_match_source_ids() {
        let compiler = ContextCompiler::new()
            .register(ContextSource {
                id: SourceId("system.static"),
                layer: ContextLayer::StaticPrefix,
                priority: 255,
                budget: BudgetPolicy::Fixed(1000),
                render: Arc::new(|_| vec![RenderedBlock::new("system body")]),
            })
            .register(ContextSource {
                id: SourceId("tools.catalog"),
                layer: ContextLayer::StaticPrefix,
                priority: 254,
                budget: BudgetPolicy::Fixed(500),
                render: Arc::new(|_| vec![RenderedBlock::placeholder(500)]),
            });

        let session = test_session();
        let compiled = compiler.compile(&ContextProjection::from_session(&session, 0));
        let report = ContextAssemblyReport::from_compiled(&compiled).with_message_tokens(1200);

        assert_eq!(report.spans.len(), 3);
        assert_eq!(report.spans[0].category, "system");
        assert_eq!(report.spans[1].category, "tools");
        assert_eq!(report.spans[2].category, "conversation");
        assert_eq!(report.span_token_sum(), report.estimated_input_tokens);
    }

    #[test]
    fn assembly_report_records_byte_range_for_text_sources() {
        let compiler = ContextCompiler::new().register(ContextSource {
            id: SourceId("system.static"),
            layer: ContextLayer::StaticPrefix,
            priority: 255,
            budget: BudgetPolicy::Fixed(100),
            render: Arc::new(|_| vec![RenderedBlock::new("hello")]),
        });
        let session = test_session();
        let compiled = compiler.compile(&ContextProjection::from_session(&session, 0));
        let span = &compiled.assembly_report.spans[0];
        assert_eq!(span.byte_range, Some(ByteRange { start: 0, end: 5 }));
    }
}
