//! Evidence envelope for tool results (facts + citations + uncertainty).
//!
//! Structured machine-checkable claims ride in `ToolResult.metadata["evidence"]`
//! so context compaction can keep the ledger while truncating prose.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Metadata key under which the envelope is stored.
pub const EVIDENCE_METADATA_KEY: &str = "evidence";

/// How complete / reliable the tool observation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyKind {
    /// Observation is complete for the requested scope.
    #[default]
    None,
    /// Explicit negative result (e.g. zero matches, missing path).
    NotFound,
    /// Partial coverage (bounded read, max_results, etc.).
    Partial,
    /// Content was truncated for size / paging.
    Truncated,
}

impl UncertaintyKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::NotFound => "not_found",
            Self::Partial => "partial",
            Self::Truncated => "truncated",
        }
    }

    /// Prefer the more severe of two kinds (for merging envelopes).
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        use UncertaintyKind::*;
        match (self, other) {
            (Truncated, _) | (_, Truncated) => Truncated,
            (Partial, _) | (_, Partial) => Partial,
            (NotFound, _) | (_, NotFound) => NotFound,
            (None, None) => None,
        }
    }
}

/// A single machine-oriented fact (key/value strings).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceFact {
    pub key: String,
    pub value: String,
}

impl EvidenceFact {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Source span the model may cite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceCitation {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u64>,
}

impl EvidenceCitation {
    #[must_use]
    pub fn path(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            start_line: None,
            end_line: None,
        }
    }

    #[must_use]
    pub fn lines(path: impl Into<String>, start: u64, end: u64) -> Self {
        Self {
            path: path.into(),
            start_line: Some(start),
            end_line: Some(end),
        }
    }

    #[must_use]
    pub fn display(&self) -> String {
        match (self.start_line, self.end_line) {
            (Some(s), Some(e)) if s == e => format!("{}:{s}", self.path),
            (Some(s), Some(e)) => format!("{}:{s}-{e}", self.path),
            (Some(s), None) => format!("{}:{s}", self.path),
            _ => self.path.clone(),
        }
    }
}

/// Structured evidence attached to a tool result.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EvidenceEnvelope {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<EvidenceFact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<EvidenceCitation>,
    #[serde(default, skip_serializing_if = "is_none_uncertainty")]
    pub uncertainty: UncertaintyKind,
}

fn is_none_uncertainty(u: &UncertaintyKind) -> bool {
    matches!(u, UncertaintyKind::None)
}

impl EvidenceEnvelope {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_fact(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.facts.push(EvidenceFact::new(key, value));
        self
    }

    #[must_use]
    pub fn with_citation(mut self, citation: EvidenceCitation) -> Self {
        self.citations.push(citation);
        self
    }

    #[must_use]
    pub fn with_uncertainty(mut self, uncertainty: UncertaintyKind) -> Self {
        self.uncertainty = uncertainty;
        self
    }

    /// Serialize for `ToolResult.metadata["evidence"]`.
    #[must_use]
    pub fn to_metadata_value(&self) -> Value {
        json!(self)
    }

    /// Parse from tool metadata if present.
    #[must_use]
    pub fn from_metadata(metadata: Option<&Value>) -> Option<Self> {
        let raw = metadata?.get(EVIDENCE_METADATA_KEY)?;
        serde_json::from_value(raw.clone()).ok()
    }

    /// Compact, always-kept ledger for context rebuild.
    #[must_use]
    pub fn format_ledger(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!(
            "[evidence uncertainty={}]",
            self.uncertainty.as_str()
        ));
        for fact in &self.facts {
            lines.push(format!("- fact: {}={}", fact.key, fact.value));
        }
        for cite in &self.citations {
            lines.push(format!("- cite: {}", cite.display()));
        }
        if self.facts.is_empty() && self.citations.is_empty() {
            lines.push("- (no facts/citations)".to_string());
        }
        lines.join("\n")
    }

    /// Merge another envelope (union facts/citations; worse uncertainty).
    pub fn merge_from(&mut self, other: &Self) {
        self.uncertainty = self.uncertainty.merge(other.uncertainty);
        for fact in &other.facts {
            if !self
                .facts
                .iter()
                .any(|f| f.key == fact.key && f.value == fact.value)
            {
                self.facts.push(fact.clone());
            }
        }
        for cite in &other.citations {
            if !self.citations.iter().any(|c| c == cite) {
                self.citations.push(cite.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ledger_includes_facts_and_citations() {
        let env = EvidenceEnvelope::new()
            .with_fact("match_count", "3")
            .with_citation(EvidenceCitation::lines("src/a.rs", 10, 12))
            .with_uncertainty(UncertaintyKind::Partial);
        let ledger = env.format_ledger();
        assert!(ledger.contains("uncertainty=partial"));
        assert!(ledger.contains("match_count=3"));
        assert!(ledger.contains("src/a.rs:10-12"));
    }

    #[test]
    fn roundtrip_metadata() {
        let env = EvidenceEnvelope::new()
            .with_fact("path", "foo.rs")
            .with_uncertainty(UncertaintyKind::NotFound);
        let meta = json!({ EVIDENCE_METADATA_KEY: env.to_metadata_value() });
        let parsed = EvidenceEnvelope::from_metadata(Some(&meta)).expect("parse");
        assert_eq!(parsed.uncertainty, UncertaintyKind::NotFound);
        assert_eq!(parsed.facts[0].value, "foo.rs");
    }
}
