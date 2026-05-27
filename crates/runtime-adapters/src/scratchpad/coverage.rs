//! Phase C1: coverage ratios, area quality gates, and P2 admission.

use crate::scratchpad::config::ScratchpadConfig;
use crate::scratchpad::schema::{AreaStatus, Inventory, NoteLine};
use crate::scratchpad::summary::compute_superseded_ids;

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

#[derive(Debug, Clone)]
pub enum CoverageGateOutcome {
    Allow { stats: CoverageStats },
    Warn {
        stats: CoverageStats,
        warning_text: String,
    },
    Block {
        stats: CoverageStats,
        reason: String,
    },
}

/// `done` area counts as reviewed/accounted when it has ≥1 finding or cleared note.
#[must_use]
pub fn area_meets_done_quality(area_id: &str, notes: &[NoteLine]) -> bool {
    notes.iter().any(|n| {
        n.area_id == area_id && (n.kind == "finding" || n.kind == "cleared")
    })
}

/// `deferred` area counts as accounted when it has ≥1 meta note with non-empty claim.
#[must_use]
pub fn area_meets_deferred_quality(area_id: &str, notes: &[NoteLine]) -> bool {
    notes.iter().any(|n| {
        n.area_id == area_id
            && n.kind == "meta"
            && n.claim
                .as_ref()
                .is_some_and(|c| !c.trim().is_empty())
    })
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
        let reason = format!(
            "accounted_ratio {:.0}% is below hard threshold {:.0}% ({} of {} areas meet quality gates); \
             finish pending areas or mark deferred with kind=meta reason before writing the report",
            stats.accounted_ratio * 100.0,
            config.coverage_hard_ratio * 100.0,
            stats.areas_accounted,
            stats.areas_total,
        );
        return CoverageGateOutcome::Block { stats, reason };
    }

    if stats.accounted_ratio < config.coverage_soft_ratio {
        let pending = stats.pending_area_ids.join(", ");
        let warning = format!(
            "WARNING: {} area(s) pending — continue review or scratchpad_set_area(deferred) with kind=meta reason.\n\
             pending_area_ids: [{pending}]",
            stats.pending_area_ids.len(),
        );
        return CoverageGateOutcome::Warn { stats, warning_text: warning };
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
            },
            InventoryArea {
                id: "a2".into(),
                path: "p".into(),
                status: AreaStatus::Done,
                notes: String::new(),
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
            },
            InventoryArea {
                id: "a2".into(),
                path: "p".into(),
                status: AreaStatus::Pending,
                notes: String::new(),
            },
        ]);
        let mut cfg = ScratchpadConfig::default();
        cfg.coverage_hard_ratio = 0.6;
        cfg.coverage_soft_ratio = 0.85;
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
        }]);
        let notes = vec![parse_note_line(
            &json!({"id":"n1","area_id":"a1","kind":"meta","claim":"out of scope for this sprint"}),
            1,
        )];
        let stats = compute_coverage_stats(&inv, &notes, &ScratchpadConfig::default());
        assert_eq!(stats.areas_accounted, 1);
        assert!((stats.accounted_ratio - 1.0).abs() < f64::EPSILON);
    }
}
