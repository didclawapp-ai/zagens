//! Compaction configuration and artifact types — shared between core and runtime-server.

/// Default model used when no model is specified.
pub const DEFAULT_COMPACTION_MODEL: &str = "deepseek-chat";

/// Hard floor for automatic compaction in v0.8.11+.
///
/// Below this token count, `should_compact` returns `false` regardless of
/// `enabled` or `token_threshold`. The point of the floor is V4 prefix-cache
/// economics: compaction rewrites the stable prefix, which destroys the KV
/// cache.
///
/// Manual `/compact` slash command bypasses this floor with explicit user
/// agency.
pub const MINIMUM_AUTO_COMPACTION_TOKENS: usize = 500_000;

/// Configuration for conversation compaction behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionConfig {
    pub enabled: bool,
    pub token_threshold: usize,
    pub model: String,
    pub cache_summary: bool,
    /// Hard floor — `should_compact` returns `false` when total session
    /// tokens fall below this number, regardless of `enabled` or
    /// `token_threshold`.
    pub auto_floor_tokens: usize,
    /// When true (P3), compaction summary is injected as a `[COMPACTED_HISTORY]`
    /// user message; system prompt keeps only a short pointer.
    pub summary_in_messages: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            token_threshold: 800_000,
            model: DEFAULT_COMPACTION_MODEL.to_string(),
            cache_summary: true,
            auto_floor_tokens: MINIMUM_AUTO_COMPACTION_TOKENS,
            summary_in_messages: true,
        }
    }
}

// ── P2-C: CompactionArtifact ───────────────────────────────────────────────────

/// Opaque artifact identifier (UUID string).
pub type ArtifactId = String;

/// A compaction artifact — records a single compaction event non-destructively.
///
/// **Phase 2-C goal**: compaction no longer destroys message history.  Instead,
/// the replaced messages are stored here.  The active rendering path skips
/// `replaced_range` and injects `summary` via the `"memory.compaction"`
/// `ContextSource`; with a wider budget the original messages can be replayed
/// verbatim (reversibility gate).
///
/// Stored in the `compaction_artifacts` SQLite table (additive migration —
/// older binaries that do not know about this table simply ignore it).
///
/// # Fields
///
/// - `id`: stable UUID string, used as the SQLite primary key.
/// - `session_id`: foreign key into `sessions`.
/// - `created_at_ms`: wall-clock milliseconds since UNIX epoch at creation.
/// - `replaced_range`: half-open `[start, end)` index range into the
///   original `session.messages` vector.  Messages at these indices are
///   "virtually removed" in the compacted view.
/// - `replaced_messages_json`: JSON-serialised `Vec<Message>` from
///   `replaced_range`.  Preserves the original content so the session can be
///   replayed without the artifact (reversibility).
/// - `summary`: LLM-generated summary text injected by
///   `"memory.compaction"` source.
/// - `original_tokens`: raw token estimate for the replaced messages.
/// - `summary_tokens`: token estimate for `summary`.
#[derive(Debug, Clone)]
pub struct CompactionArtifact {
    pub id: ArtifactId,
    pub session_id: String,
    pub created_at_ms: i64,
    /// `[start, end)` index range in the original `session.messages` vector.
    pub replaced_start: usize,
    pub replaced_end: usize,
    /// Original messages preserved for reversibility.  Stored as JSON in SQLite.
    pub replaced_messages_json: String,
    pub summary: String,
    pub original_tokens: u32,
    pub summary_tokens: u32,
}

impl CompactionArtifact {
    /// Number of messages replaced by this artifact.
    #[must_use]
    pub fn replaced_count(&self) -> usize {
        self.replaced_end.saturating_sub(self.replaced_start)
    }

    /// Whether this artifact covers the given message index.
    #[must_use]
    pub fn covers_index(&self, idx: usize) -> bool {
        idx >= self.replaced_start && idx < self.replaced_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compaction_artifact_covers_index() {
        let art = CompactionArtifact {
            id: "test-id".to_string(),
            session_id: "sess".to_string(),
            created_at_ms: 0,
            replaced_start: 2,
            replaced_end: 5,
            replaced_messages_json: "[]".to_string(),
            summary: "summary".to_string(),
            original_tokens: 100,
            summary_tokens: 20,
        };
        assert!(!art.covers_index(1));
        assert!(art.covers_index(2));
        assert!(art.covers_index(4));
        assert!(!art.covers_index(5));
        assert_eq!(art.replaced_count(), 3);
    }
}
