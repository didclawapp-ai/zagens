//! Phase C1: coverage ratios, area quality gates, and P2 admission.

use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::schema::{AreaStatus, Inventory, NoteLine};
use crate::scratchpad::summary::compute_superseded_ids;

pub use crate::scratchpad::note_quality::{area_meets_deferred_quality, area_meets_done_quality};

/// Coverage metrics for inventory + notes (§6.12.4).
#[derive(Debug, Clone)]
pub struct CoverageStats {
    pub areas_total: usize,
    pub areas_accounted: usize,
    pub areas_reviewed: usize,
    pub accounted_ratio: f64,
    pub reviewed_ratio: f64,
    pub pending_area_ids: Vec<String>,
    pub deferred_areas: Vec<DeferredAreaSummary>,
    pub verified_findings: usize,
}

#[derive(Debug, Clone)]
pub struct DeferredAreaSummary {
    pub id: String,
    pub reason_excerpt: String,
}

/// One inventory row that is `done`/`deferred` but fails C1 quality gates.
#[derive(Debug, Clone)]
pub struct AreaQualityGap {
    pub id: String,
    pub status: String,
    pub fix: String,
}

#[derive(Debug, Clone)]
pub enum CoverageGateOutcome {
    Allow {
        stats: CoverageStats,
    },
    Warn {
        stats: CoverageStats,
        warning_text: String,
    },
    Block {
        stats: CoverageStats,
        reason: String,
    },
}

#[must_use]
pub fn compute_coverage_stats(
    inventory: &Inventory,
    notes: &[NoteLine],
    config: &ScratchpadConfig,
) -> CoverageStats {
    let superseded = compute_superseded_ids(notes);
    let verified_findings = notes
        .iter()
        .filter(|n| {
            n.kind == "finding"
                && n.status.eq_ignore_ascii_case("verified")
                && !superseded.contains(&n.id)
        })
        .count();

    let areas_total = inventory.areas.len();
    let mut areas_accounted = 0usize;
    let mut areas_reviewed = 0usize;
    let mut pending_area_ids = Vec::new();
    let mut deferred_areas = Vec::new();

    for area in &inventory.areas {
        match area.status {
            AreaStatus::Pending | AreaStatus::InProgress => {
                pending_area_ids.push(area.id.clone());
            }
            AreaStatus::Done => {
                if area_meets_done_quality(&area.id, notes) {
                    areas_accounted += 1;
                    areas_reviewed += 1;
                }
            }
            AreaStatus::Deferred => {
                if config.coverage_count_deferred_as_accounted
                    && area_meets_deferred_quality(&area.id, notes)
                {
                    areas_accounted += 1;
                    if let Some(reason) = deferred_reason_excerpt(&area.id, notes) {
                        deferred_areas.push(DeferredAreaSummary {
                            id: area.id.clone(),
                            reason_excerpt: reason,
                        });
                    }
                }
            }
        }
    }

    let accounted_ratio = ratio(areas_accounted, areas_total);
    let reviewed_ratio = ratio(areas_reviewed, areas_total);

    CoverageStats {
        areas_total,
        areas_accounted,
        areas_reviewed,
        accounted_ratio,
        reviewed_ratio,
        pending_area_ids,
        deferred_areas,
        verified_findings,
    }
}

fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        1.0
    } else {
        num as f64 / den as f64
    }
}

fn deferred_reason_excerpt(area_id: &str, notes: &[NoteLine]) -> Option<String> {
    notes
        .iter()
        .filter(|n| n.area_id == area_id && n.kind == "meta")
        .find_map(|n| n.claim.as_ref().filter(|c| !c.trim().is_empty()))
        .map(|c| {
            let t = c.trim();
            if t.chars().count() > 120 {
                let head: String = t.chars().take(120).collect();
                format!("{head}…")
            } else {
                t.to_string()
            }
        })
}

/// L0-only line for compaction handoff (§6.12.3) — not full layered summary.
#[must_use]
pub fn build_l0_status_line(run_id: &str, stats: &CoverageStats, resume_area_id: &str) -> String {
    let accounted_pct = (stats.accounted_ratio * 100.0).round() as u32;
    format!(
        "run_id={run_id} areas {}/{} accounted ({}%), {} reviewed; resume_area_id={}; verified_findings={}",
        stats.areas_accounted,
        stats.areas_total,
        accounted_pct,
        stats.areas_reviewed,
        resume_area_id,
        stats.verified_findings,
    )
}

#[must_use]
pub fn resume_area_id_from_inventory(inventory: &Inventory) -> String {
    inventory
        .areas
        .iter()
        .find(|a| matches!(a.status, AreaStatus::Pending | AreaStatus::InProgress))
        .map(|a| a.id.as_str())
        .unwrap_or("none")
        .to_string()
}

/// Inventory rows marked closed (`done`/`deferred`) that do not count toward `areas_accounted`.
#[must_use]
pub fn areas_failing_quality_gate(
    inventory: &Inventory,
    notes: &[NoteLine],
    config: &ScratchpadConfig,
) -> Vec<AreaQualityGap> {
    let mut gaps = Vec::new();
    for area in &inventory.areas {
        match area.status {
            AreaStatus::Done if !area_meets_done_quality(&area.id, notes) => {
                gaps.push(AreaQualityGap {
                    id: area.id.clone(),
                    status: "done".into(),
                    fix: "scratchpad_append kind=finding or kind=cleared with [D#] evidence (meta-only notes do not count for done)".into(),
                });
            }
            AreaStatus::Deferred
                if config.coverage_count_deferred_as_accounted
                    && !area_meets_deferred_quality(&area.id, notes) =>
            {
                gaps.push(AreaQualityGap {
                    id: area.id.clone(),
                    status: "deferred".into(),
                    fix: "scratchpad_append kind=meta with non-empty defer reason (not security-risk-only stub)".into(),
                });
            }
            _ => {}
        }
    }
    gaps
}

#[must_use]
pub fn format_quality_gate_block_reason(
    stats: &CoverageStats,
    gaps: &[AreaQualityGap],
    config: &ScratchpadConfig,
) -> String {
    let mut reason = format!(
        "accounted_ratio {:.0}% is below hard threshold {:.0}% ({} of {} areas meet quality gates)",
        stats.accounted_ratio * 100.0,
        config.coverage_hard_ratio * 100.0,
        stats.areas_accounted,
        stats.areas_total,
    );
    if !gaps.is_empty() {
        reason.push_str("; areas failing quality gates:");
        for gap in gaps.iter().take(12) {
            reason.push_str(&format!("\n- {} ({}) — {}", gap.id, gap.status, gap.fix));
        }
        if gaps.len() > 12 {
            reason.push_str(&format!("\n- … and {} more", gaps.len() - 12));
        }
    } else if !stats.pending_area_ids.is_empty() {
        reason.push_str("; finish pending areas or mark deferred with kind=meta reason");
    } else {
        reason.push_str(
            "; for each done area without findings use kind=cleared, not meta-only summaries",
        );
    }
    reason
}

/// User-approved partial close-out (`_global` meta) bypasses the reviewed-ratio hard gate.
#[must_use]
pub fn partial_closeout_approved(notes: &[NoteLine]) -> bool {
    notes.iter().any(|n| {
        n.area_id == "_global"
            && n.kind == "meta"
            && n.claim.as_ref().is_some_and(|c| {
                let lower = c.to_lowercase();
                lower.contains("partial_closeout")
                    || lower.contains("部分收口")
                    || lower.contains("partial audit close")
            })
    })
}

#[must_use]
pub fn format_reviewed_gate_block_reason(
    stats: &CoverageStats,
    config: &ScratchpadConfig,
) -> String {
    format!(
        "reviewed_ratio {:.0}% is below hard threshold {:.0}% ({} of {} areas actually examined with finding/cleared; deferred-only does not count). \
         Completion over cost: continue P1 (more done+finding/cleared) — do not stop for token/time/budget. \
         Only append `_global` meta `partial_closeout` / `部分收口` if the user explicitly approved a partial report.",
        stats.reviewed_ratio * 100.0,
        config.coverage_reviewed_hard_ratio * 100.0,
        stats.areas_reviewed,
        stats.areas_total,
    )
}

#[must_use]
pub fn coverage_gate(
    inventory: &Inventory,
    notes: &[NoteLine],
    config: &ScratchpadConfig,
) -> CoverageGateOutcome {
    let stats = compute_coverage_stats(inventory, notes, config);

    if stats.areas_total == 0 {
        return CoverageGateOutcome::Allow { stats };
    }

    if stats.accounted_ratio < config.coverage_hard_ratio && config.coverage_hard_block_enabled {
        let gaps = areas_failing_quality_gate(inventory, notes, config);
        let reason = format_quality_gate_block_reason(&stats, &gaps, config);
        return CoverageGateOutcome::Block { stats, reason };
    }

    if config.coverage_reviewed_hard_block_enabled
        && stats.reviewed_ratio < config.coverage_reviewed_hard_ratio
        && !partial_closeout_approved(notes)
    {
        let reason = format_reviewed_gate_block_reason(&stats, config);
        return CoverageGateOutcome::Block { stats, reason };
    }

    if stats.accounted_ratio < config.coverage_soft_ratio {
        let pending = stats.pending_area_ids.join(", ");
        let warning = format!(
            "WARNING: {} area(s) pending — continue review or scratchpad_set_area(deferred) with kind=meta reason.\n\
             pending_area_ids: [{pending}]",
            stats.pending_area_ids.len(),
        );
        return CoverageGateOutcome::Warn {
            stats,
            warning_text: warning,
        };
    }

    CoverageGateOutcome::Allow { stats }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scratchpad::schema::{InventoryArea, parse_note_line};
    use serde_json::json;

    fn inv_with_areas(areas: Vec<InventoryArea>) -> Inventory {
        Inventory {
            run_id: "r".into(),
            created_at: String::new(),
            completed_at: None,
            scope: None,
            areas,
        }
    }

    #[test]
    fn empty_deferred_without_meta_not_accounted() {
        let inv = inv_with_areas(vec![
            InventoryArea {
                id: "a1".into(),
                path: "p".into(),
                status: AreaStatus::Deferred,
                notes: String::new(),
                high_complexity: false,
            },
            InventoryArea {
                id: "a2".into(),
                path: "p".into(),
                status: AreaStatus::Done,
                notes: String::new(),
                high_complexity: false,
            },
        ]);
        let notes = vec![parse_note_line(
            &json!({"id":"n1","area_id":"a2","kind":"finding","status":"verified"}),
            1,
        )];
        let stats = compute_coverage_stats(&inv, &notes, &ScratchpadConfig::default());
        assert_eq!(stats.areas_accounted, 1);
        assert!((stats.accounted_ratio - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn coverage_gate_blocks_low_accounted() {
        let inv = inv_with_areas(vec![
            InventoryArea {
                id: "a1".into(),
                path: "p".into(),
                status: AreaStatus::Pending,
                notes: String::new(),
                high_complexity: false,
            },
            InventoryArea {
                id: "a2".into(),
                path: "p".into(),
                status: AreaStatus::Pending,
                notes: String::new(),
                high_complexity: false,
            },
        ]);
        let cfg = ScratchpadConfig {
            coverage_hard_ratio: 0.6,
            coverage_soft_ratio: 0.85,
            ..Default::default()
        };
        let outcome = coverage_gate(&inv, &[], &cfg);
        assert!(matches!(outcome, CoverageGateOutcome::Block { .. }));
    }

    #[test]
    fn deferred_with_meta_counts_accounted() {
        let inv = inv_with_areas(vec![InventoryArea {
            id: "a1".into(),
            path: "p".into(),
            status: AreaStatus::Deferred,
            notes: String::new(),
            high_complexity: false,
        }]);
        let notes = vec![parse_note_line(
            &json!({"id":"n1","area_id":"a1","kind":"meta","claim":"out of scope for this sprint"}),
            1,
        )];
        let stats = compute_coverage_stats(&inv, &notes, &ScratchpadConfig::default());
        assert_eq!(stats.areas_accounted, 1);
        assert!((stats.accounted_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn done_with_meta_only_not_accounted() {
        let inv = inv_with_areas(vec![InventoryArea {
            id: "area-types".into(),
            path: "frontend/src/types".into(),
            status: AreaStatus::Done,
            notes: String::new(),
            high_complexity: false,
        }]);
        let notes = vec![parse_note_line(
            &json!({"id":"n1","area_id":"area-types","kind":"meta","claim":"audit complete summary"}),
            1,
        )];
        let cfg = ScratchpadConfig::default();
        let stats = compute_coverage_stats(&inv, &notes, &cfg);
        assert_eq!(stats.areas_accounted, 0);
        let gaps = areas_failing_quality_gate(&inv, &notes, &cfg);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].id, "area-types");
        let outcome = coverage_gate(&inv, &notes, &cfg);
        if let CoverageGateOutcome::Block { reason, .. } = outcome {
            assert!(reason.contains("area-types"));
            assert!(reason.contains("kind=cleared"));
        } else {
            panic!("expected block for meta-only done area");
        }
    }

    #[test]
    fn coverage_gate_blocks_mass_defer_despite_full_accounted() {
        let mut areas = Vec::new();
        for i in 0..9 {
            areas.push(InventoryArea {
                id: format!("done-{i}"),
                path: "p".into(),
                status: AreaStatus::Done,
                notes: String::new(),
                high_complexity: false,
            });
        }
        for i in 0..28 {
            areas.push(InventoryArea {
                id: format!("def-{i}"),
                path: "p".into(),
                status: AreaStatus::Deferred,
                notes: String::new(),
                high_complexity: false,
            });
        }
        let inv = inv_with_areas(areas);
        let mut notes = Vec::new();
        for i in 0..9 {
            notes.push(parse_note_line(
                &json!({"id":format!("f-{i}"),"area_id":format!("done-{i}"),"kind":"finding","status":"verified"}),
                i + 1,
            ));
        }
        for i in 0..28 {
            notes.push(parse_note_line(
                &json!({"id":format!("m-{i}"),"area_id":format!("def-{i}"),"kind":"meta","claim":"deferred: session limit"}),
                100 + i,
            ));
        }
        let cfg = ScratchpadConfig::default();
        let stats = compute_coverage_stats(&inv, &notes, &cfg);
        assert!((stats.accounted_ratio - 1.0).abs() < f64::EPSILON);
        assert!(stats.reviewed_ratio < 0.40);
        let outcome = coverage_gate(&inv, &notes, &cfg);
        assert!(matches!(outcome, CoverageGateOutcome::Block { .. }));
    }

    #[test]
    fn partial_closeout_bypasses_reviewed_hard_gate() {
        let inv = inv_with_areas(vec![
            InventoryArea {
                id: "done-0".into(),
                path: "p".into(),
                status: AreaStatus::Done,
                notes: String::new(),
                high_complexity: false,
            },
            InventoryArea {
                id: "def-0".into(),
                path: "p".into(),
                status: AreaStatus::Deferred,
                notes: String::new(),
                high_complexity: false,
            },
        ]);
        let notes = vec![
            parse_note_line(
                &json!({"id":"f1","area_id":"done-0","kind":"finding","status":"verified"}),
                1,
            ),
            parse_note_line(
                &json!({"id":"m1","area_id":"def-0","kind":"meta","claim":"deferred: time"}),
                2,
            ),
            parse_note_line(
                &json!({"id":"pc","area_id":"_global","kind":"meta","claim":"partial_closeout: user approved partial report"}),
                3,
            ),
        ];
        let outcome = coverage_gate(&inv, &notes, &ScratchpadConfig::default());
        assert!(matches!(outcome, CoverageGateOutcome::Allow { .. }));
    }
}
