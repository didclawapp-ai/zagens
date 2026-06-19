//! Cleared / deferred note claim quality gates (multi-dimension audit balance).

use crate::scratchpad::schema::{AreaStatus, Inventory, NoteLine};

const MIN_CLEARED_CLAIM_CHARS: usize = 20;

const TRIVIAL_CLEARED: &[&str] = &[
    "无",
    "无.",
    "无。",
    "ok",
    "none",
    "n/a",
    "na",
    "cleared",
    "no issues",
    "no issue",
    "nothing",
    "all clear",
    "无问题",
    "没有问题",
    "通过",
    "pass",
    "clean",
    "nothing found",
    "no findings",
    "lgtm",
];

/// `kind=cleared` must cite a dimension tag and concrete evidence — not a one-word stub.
pub fn validate_cleared_claim(claim: &str) -> Result<(), &'static str> {
    let trimmed = claim.trim();
    if trimmed.is_empty() {
        return Err("cleared claim must be non-empty");
    }
    if trimmed.chars().count() < MIN_CLEARED_CLAIM_CHARS {
        return Err(
            "cleared claim must be ≥20 characters with dimension tag [D1]–[D10] or D#: and concrete check evidence (grep/read_file/cargo output)",
        );
    }
    let lower = trimmed.to_lowercase();
    if TRIVIAL_CLEARED.iter().any(|t| lower == t.to_lowercase()) {
        return Err(
            "cleared claim cannot be a stub (无/ok/no issues); cite [D#] and what you checked",
        );
    }
    if !claim_has_dimension_tag(trimmed) {
        return Err(
            "cleared claim must include a dimension tag such as [D2] or D6: describing what was examined",
        );
    }
    Ok(())
}

/// Defer reasons must name scope/time/dimension gaps — not security-risk-only stubs.
pub fn validate_deferred_meta_claim(claim: &str) -> Result<(), &'static str> {
    let trimmed = claim.trim();
    if trimmed.is_empty() {
        return Err("deferred meta claim must be non-empty");
    }
    if is_security_only_defer_reason(trimmed) {
        return Err(
            "defer reason cannot be security-risk-only (e.g. 低安全风险); state unreviewed dimensions, time budget, or scope limit",
        );
    }
    Ok(())
}

#[must_use]
pub fn claim_has_dimension_tag(claim: &str) -> bool {
    let upper = claim.to_uppercase();
    (1..=10).any(|d| upper.contains(&format!("[D{d}]")) || upper.contains(&format!("D{d}:")))
}

#[must_use]
pub fn cleared_note_meets_quality(note: &NoteLine) -> bool {
    note.kind == "cleared"
        && note
            .claim
            .as_ref()
            .is_some_and(|c| validate_cleared_claim(c).is_ok())
}

#[must_use]
pub fn deferred_meta_meets_quality(note: &NoteLine) -> bool {
    note.kind == "meta"
        && note
            .claim
            .as_ref()
            .is_some_and(|c| !c.trim().is_empty() && validate_deferred_meta_claim(c).is_ok())
}

/// `done` counts only when finding or dimension-qualified cleared note exists.
#[must_use]
pub fn area_meets_done_quality(area_id: &str, notes: &[NoteLine]) -> bool {
    notes
        .iter()
        .any(|n| n.area_id == area_id && (n.kind == "finding" || cleared_note_meets_quality(n)))
}

/// `deferred` counts only when meta reason passes substance gate.
#[must_use]
pub fn area_meets_deferred_quality(area_id: &str, notes: &[NoteLine]) -> bool {
    notes
        .iter()
        .any(|n| n.area_id == area_id && deferred_meta_meets_quality(n))
}

/// Warn when reviewed areas skew entirely to D1 (security) with no other dimensions.
#[must_use]
pub fn build_dimension_balance_hint(inventory: &Inventory, notes: &[NoteLine]) -> Option<String> {
    let mut done_with_cleared = 0usize;
    let mut d1_only = 0usize;
    let mut non_d1 = 0usize;

    for area in &inventory.areas {
        if area.status != AreaStatus::Done {
            continue;
        }
        let cleared: Vec<_> = notes
            .iter()
            .filter(|n| n.area_id == area.id && n.kind == "cleared")
            .collect();
        if cleared.is_empty() {
            continue;
        }
        done_with_cleared += 1;
        let dims: std::collections::HashSet<u8> = cleared
            .iter()
            .flat_map(|n| {
                n.claim
                    .as_deref()
                    .map(extract_dimension_ids)
                    .unwrap_or_default()
            })
            .collect();
        if dims.is_empty() {
            continue;
        }
        if dims.len() == 1 && dims.contains(&1) {
            d1_only += 1;
        } else if dims.iter().any(|d| *d != 1) {
            non_d1 += 1;
        }
    }

    if done_with_cleared >= 3 && non_d1 == 0 && d1_only >= 2 {
        return Some(
            "dimension balance: done areas only cite D1 — examine D2/D3/D6 (tests, maintainability, release) before P2".into(),
        );
    }
    None
}

#[must_use]
pub fn extract_dimension_ids(claim: &str) -> Vec<u8> {
    let upper = claim.to_uppercase();
    (1..=10)
        .filter(|d| upper.contains(&format!("[D{d}]")) || upper.contains(&format!("D{d}:")))
        .collect()
}

fn is_security_only_defer_reason(claim: &str) -> bool {
    let t = claim.trim().to_lowercase();
    if t.is_empty() {
        return false;
    }

    const EXACT: &[&str] = &[
        "低安全风险",
        "低风险",
        "low security risk",
        "low risk layer",
        "security risk layer",
        "低安全",
    ];
    if EXACT.iter().any(|p| t == *p) {
        return true;
    }

    const SECURITY_MARKERS: &[&str] = &[
        "低安全风险",
        "low security risk",
        "security risk layer",
        "低风险层",
    ];
    let has_security = SECURITY_MARKERS.iter().any(|m| t.contains(m));
    if !has_security {
        return false;
    }

    const SUBSTANCE: &[&str] = &[
        "d2",
        "d3",
        "d4",
        "d5",
        "d6",
        "d7",
        "d8",
        "d9",
        "d10",
        "[d",
        "时间",
        "time",
        "scope",
        "范围",
        "测试",
        "test",
        "架构",
        "maintain",
        "文档",
        "doc",
        "session",
        "budget",
        "out of scope",
        "未审",
        "维度",
    ];
    let has_substance = SUBSTANCE.iter().any(|m| t.contains(m));
    has_security && !has_substance && t.chars().count() < 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_trivial_cleared() {
        assert!(validate_cleared_claim("无").is_err());
        assert!(validate_cleared_claim("ok").is_err());
    }

    #[test]
    fn accepts_dimension_cleared() {
        let claim = "[D2] read_file core/src/engine/turn_loop.rs — no data races in mutex usage; grep confirmed Arc<Mutex> at all shared state sites";
        assert!(validate_cleared_claim(claim).is_ok());
    }

    #[test]
    fn rejects_security_only_defer() {
        assert!(validate_deferred_meta_claim("低安全风险").is_err());
        assert!(validate_deferred_meta_claim("low security risk").is_err());
    }

    #[test]
    fn accepts_substantive_defer() {
        assert!(
            validate_deferred_meta_claim(
                "deferred: D3/D5 tests not run — session time budget; 12 files unread"
            )
            .is_ok()
        );
    }
}
