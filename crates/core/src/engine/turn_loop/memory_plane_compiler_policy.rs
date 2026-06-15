//! ContextCompiler source mapping for memory-plane queries (Phase 3b batch 4 / 8e).

use super::memory_plane_episodic_policy::QUERY_TOPIC_EPISODIC;
use super::memory_plane_query_policy::{
    QUERY_COMPACTION_SUMMARY, QUERY_SCRATCHPAD_REMINDER, QUERY_SCRATCHPAD_SUMMARY,
    QUERY_WORKING_SET,
};

/// Map a symbolic memory query key to a ContextCompiler source id.
#[must_use]
pub fn compiler_source_for_query_key(query_key: &str) -> &'static str {
    match query_key {
        QUERY_SCRATCHPAD_SUMMARY => "memory.scratchpad_summary",
        QUERY_SCRATCHPAD_REMINDER => "memory.scratchpad_reminder",
        QUERY_COMPACTION_SUMMARY => "memory.compaction",
        QUERY_WORKING_SET => "working_set",
        QUERY_TOPIC_EPISODIC => "topic_memory",
        _ => "memory.unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_query_keys() {
        assert_eq!(
            compiler_source_for_query_key(QUERY_WORKING_SET),
            "working_set"
        );
        assert_eq!(
            compiler_source_for_query_key(QUERY_TOPIC_EPISODIC),
            "topic_memory"
        );
    }
}
