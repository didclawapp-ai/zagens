//! Hard citation auditor — cheap FS-free checks given line counts.
//!
//! Validates evidence citations (line ranges) and optional `total_matches` /
//! `match_count` facts against the ledger itself. Callers supply line counts
//! via a resolve callback when they have filesystem access.

use zagens_tools::EvidenceEnvelope;

/// One citation / fact problem found by the auditor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationAuditIssue {
    pub kind: CitationAuditKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CitationAuditKind {
    /// Cited path could not be resolved / does not exist.
    MissingPath,
    /// `start_line`/`end_line` outside file bounds.
    LineOutOfRange,
    /// Fact match count disagrees with citation count (or content markers).
    MatchCountMismatch,
}

/// Result of auditing an evidence envelope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CitationAuditReport {
    pub issues: Vec<CitationAuditIssue>,
}

impl CitationAuditReport {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.issues.is_empty()
    }

    /// Compact status line for tool-result annotation.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.ok() {
            "citation_audit=ok".to_string()
        } else {
            let kinds: Vec<&str> = self
                .issues
                .iter()
                .map(|i| match i.kind {
                    CitationAuditKind::MissingPath => "missing_path",
                    CitationAuditKind::LineOutOfRange => "line_oor",
                    CitationAuditKind::MatchCountMismatch => "match_mismatch",
                })
                .collect();
            format!("citation_audit=fail({})", kinds.join(","))
        }
    }
}

/// Audit citations using a line-count resolver (`None` = path missing).
///
/// `content_match_hint` is an optional count of matches visible in prose
/// (e.g. number of `path:line:` style hits) used when facts claim a total.
#[must_use]
pub fn audit_evidence_citations(
    envelope: &EvidenceEnvelope,
    mut resolve_line_count: impl FnMut(&str) -> Option<u64>,
    content_match_hint: Option<u64>,
) -> CitationAuditReport {
    let mut issues = Vec::new();

    for cite in &envelope.citations {
        let path = cite.path.trim();
        if path.is_empty() {
            continue;
        }
        match resolve_line_count(path) {
            None => {
                issues.push(CitationAuditIssue {
                    kind: CitationAuditKind::MissingPath,
                    detail: format!("path not found: {path}"),
                });
            }
            Some(total) => {
                if let Some(start) = cite.start_line
                    && start > total
                {
                    issues.push(CitationAuditIssue {
                        kind: CitationAuditKind::LineOutOfRange,
                        detail: format!("{path}:{start} > total_lines={total}"),
                    });
                }
                if let Some(end) = cite.end_line
                    && end > total
                {
                    issues.push(CitationAuditIssue {
                        kind: CitationAuditKind::LineOutOfRange,
                        detail: format!("{path}:{end} > total_lines={total}"),
                    });
                }
                if let (Some(start), Some(end)) = (cite.start_line, cite.end_line)
                    && start > end
                {
                    issues.push(CitationAuditIssue {
                        kind: CitationAuditKind::LineOutOfRange,
                        detail: format!("{path}: start_line={start} > end_line={end}"),
                    });
                }
            }
        }
    }

    let claimed = envelope
        .facts
        .iter()
        .find(|f| f.key == "total_matches" || f.key == "match_count")
        .and_then(|f| f.value.parse::<u64>().ok());

    if let Some(claimed) = claimed {
        let cite_span_count = envelope
            .citations
            .iter()
            .filter(|c| c.start_line.is_some())
            .count() as u64;
        // Prefer content hint when present; else require cite count ≤ claimed
        // and flag only when cites exist but claimed is zero (or vice versa for
        // content mode with many cites).
        if let Some(hint) = content_match_hint {
            if hint != claimed && !(hint == 0 && claimed == 0) {
                // Allow claimed ≥ hint when results were truncated (partial).
                let truncated = matches!(
                    envelope.uncertainty,
                    zagens_tools::UncertaintyKind::Truncated
                        | zagens_tools::UncertaintyKind::Partial
                );
                if !truncated || hint > claimed {
                    issues.push(CitationAuditIssue {
                        kind: CitationAuditKind::MatchCountMismatch,
                        detail: format!("fact match_count={claimed} vs content_hint={hint}"),
                    });
                }
            }
        } else if claimed == 0 && cite_span_count > 0 {
            issues.push(CitationAuditIssue {
                kind: CitationAuditKind::MatchCountMismatch,
                detail: format!("fact match_count=0 but {cite_span_count} line citations"),
            });
        }
    }

    CitationAuditReport { issues }
}

/// Build a one-shot user nudge when recent tool evidence fails hard audit.
#[must_use]
pub fn maybe_citation_audit_nudge(report: &CitationAuditReport) -> Option<String> {
    if report.ok() {
        return None;
    }
    let details: Vec<&str> = report.issues.iter().map(|i| i.detail.as_str()).collect();
    Some(format!(
        "[citation audit] Recent tool evidence failed cheap verification: {}.\n\
         Do not treat those citations as ground truth — re-read or re-search the paths.",
        details.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_tools::{EvidenceCitation, UncertaintyKind};

    #[test]
    fn flags_missing_path() {
        let env =
            EvidenceEnvelope::new().with_citation(EvidenceCitation::lines("missing.rs", 1, 2));
        let report = audit_evidence_citations(&env, |_| None, None);
        assert!(!report.ok());
        assert_eq!(report.issues[0].kind, CitationAuditKind::MissingPath);
    }

    #[test]
    fn flags_line_out_of_range() {
        let env =
            EvidenceEnvelope::new().with_citation(EvidenceCitation::lines("src/a.rs", 10, 99));
        let report = audit_evidence_citations(&env, |_| Some(20), None);
        assert!(!report.ok());
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.kind == CitationAuditKind::LineOutOfRange)
        );
    }

    #[test]
    fn ok_when_lines_in_range() {
        let env = EvidenceEnvelope::new()
            .with_citation(EvidenceCitation::lines("src/a.rs", 1, 10))
            .with_fact("total_matches", "1")
            .with_uncertainty(UncertaintyKind::None);
        let report = audit_evidence_citations(&env, |_| Some(100), Some(1));
        assert!(report.ok());
    }

    #[test]
    fn match_mismatch_with_hint() {
        let env = EvidenceEnvelope::new().with_fact("total_matches", "5");
        let report = audit_evidence_citations(&env, |_| Some(10), Some(2));
        assert!(!report.ok());
        assert_eq!(report.issues[0].kind, CitationAuditKind::MatchCountMismatch);
    }
}
