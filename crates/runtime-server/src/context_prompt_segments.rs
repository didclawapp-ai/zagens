//! Static system-prompt decomposition for Context Explorer categories (P2b+).

use zagens_core::chat::Tool;
use zagens_core::engine::dispatch::is_mcp_tool_name;
use zagens_core::engine::token_estimate::estimate_text_tokens;

/// Partition of the static system prefix (before compaction tail) by Explorer taxonomy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StaticPromptSegments {
    /// Mode, environment, memory/topic injects, context-management, compact template.
    pub system_core: String,
    /// Project context + configured `instructions = [...]` blocks.
    pub rules: String,
    /// `## Skills` catalog block.
    pub skills: String,
}

const RULES_MARKERS: &[&str] = &[
    "<project_instructions",
    "### Project Structure (Automatic Map)",
    "<instructions source=\"",
];

const SKILLS_MARKER: &str = "## Skills\n";

const SYSTEM_MARKERS: &[&str] = &[
    "## Environment\n",
    "## Current Session Goal\n",
    "## Context Management\n",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum SegmentKind {
    System = 0,
    Rules = 1,
    Skills = 2,
}

#[derive(Debug, Clone, Copy)]
struct MarkerHit {
    pos: usize,
    kind: SegmentKind,
}

fn collect_marker_hits(text: &str) -> Vec<MarkerHit> {
    let mut hits: Vec<MarkerHit> = Vec::new();
    for marker in RULES_MARKERS {
        if let Some(pos) = text.find(marker) {
            hits.push(MarkerHit {
                pos,
                kind: SegmentKind::Rules,
            });
        }
    }
    if let Some(pos) = text.find(SKILLS_MARKER) {
        hits.push(MarkerHit {
            pos,
            kind: SegmentKind::Skills,
        });
    }
    for marker in SYSTEM_MARKERS {
        if let Some(pos) = text.find(marker) {
            hits.push(MarkerHit {
                pos,
                kind: SegmentKind::System,
            });
        }
    }
    hits.sort_by_key(|h| h.pos);
    hits
}

/// Split composed static system text into Explorer buckets using stable markers.
#[must_use]
pub fn decompose_static_system_text(text: &str) -> StaticPromptSegments {
    if text.is_empty() {
        return StaticPromptSegments::default();
    }

    let hits = collect_marker_hits(text);
    if hits.is_empty() {
        return StaticPromptSegments {
            system_core: text.to_string(),
            ..Default::default()
        };
    }

    let mut system_core = String::new();
    let mut rules = String::new();
    let mut skills = String::new();
    let mut cursor = 0usize;

    for (i, hit) in hits.iter().enumerate() {
        if hit.pos > cursor {
            system_core.push_str(&text[cursor..hit.pos]);
        }
        let end = hits.get(i + 1).map(|h| h.pos).unwrap_or(text.len());
        let chunk = &text[hit.pos..end];
        match hit.kind {
            SegmentKind::Rules => rules.push_str(chunk),
            SegmentKind::Skills => skills.push_str(chunk),
            SegmentKind::System => system_core.push_str(chunk),
        }
        cursor = end;
    }

    StaticPromptSegments {
        system_core,
        rules,
        skills,
    }
}

/// Estimate builtin vs MCP portions of the active tool catalog JSON.
#[must_use]
pub fn split_tool_catalog_tokens(tools: &[Tool]) -> (u32, u32) {
    let (builtin, mcp): (Vec<_>, Vec<_>) = tools
        .iter()
        .cloned()
        .partition(|tool| !is_mcp_tool_name(&tool.name));

    let builtin_tokens = if builtin.is_empty() {
        0
    } else {
        estimate_text_tokens(&serde_json::to_string(&builtin).unwrap_or_default()) as u32
    };
    let mcp_tokens = if mcp.is_empty() {
        0
    } else {
        estimate_text_tokens(&serde_json::to_string(&mcp).unwrap_or_default()) as u32
    };
    (builtin_tokens, mcp_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decompose_splits_rules_and_skills_from_static_prefix() {
        let text = "mode base\n\n\
            <project_instructions source=\"AGENTS.md\">\nagent rules\n</project_instructions>\n\n\
            ## Environment\n\n- lang: en\n\n\
            <instructions source=\"/tmp/rules.md\">\ncustom rule\n</instructions>\n\n\
            ## Skills\n\n### Available skills\n- audit: review\n\n\
            ## Context Management\n\nkeep cache hot\n\n\
            COMPACT_MARKER";
        let segments = decompose_static_system_text(text);
        assert!(segments.system_core.contains("mode base"));
        assert!(segments.system_core.contains("## Environment"));
        assert!(segments.system_core.contains("## Context Management"));
        assert!(segments.rules.contains("<project_instructions"));
        assert!(segments.rules.contains("<instructions source="));
        assert!(segments.skills.contains("## Skills"));
        assert!(!segments.skills.contains("## Context Management"));
    }

    #[test]
    fn decompose_empty_markers_yield_single_system_core() {
        let text = "plain mode prompt only";
        let segments = decompose_static_system_text(text);
        assert_eq!(segments.system_core, text);
        assert!(segments.rules.is_empty());
        assert!(segments.skills.is_empty());
    }

    #[test]
    fn split_tool_catalog_tokens_partitions_mcp_prefix() {
        let tools = vec![
            Tool {
                tool_type: None,
                name: "read_file".into(),
                description: "read".into(),
                input_schema: serde_json::json!({}),
                allowed_callers: None,
                defer_loading: None,
                input_examples: None,
                strict: None,
                cache_control: None,
            },
            Tool {
                tool_type: None,
                name: "mcp_github_search".into(),
                description: "mcp".into(),
                input_schema: serde_json::json!({}),
                allowed_callers: None,
                defer_loading: None,
                input_examples: None,
                strict: None,
                cache_control: None,
            },
        ];
        let (builtin, mcp) = split_tool_catalog_tokens(&tools);
        assert!(builtin > 0);
        assert!(mcp > 0);
    }
}
