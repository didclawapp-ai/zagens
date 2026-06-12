//! Approval policy helpers — aligned with desktop `web-ui/src/lib/approvalPolicy.ts`.

use anyhow::Result;

use crate::config::save_approval_policy;

/// Config values for `approval_policy` (Settings → 审批策略).
pub const POLICIES: &[&str] = &["on-request", "untrusted", "never", "auto"];

pub fn normalize_policy(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => "auto",
        "never" | "deny" | "denied" => "never",
        "untrusted" => "untrusted",
        "suggest" | "suggested" | "on-request" => "on-request",
        _ => "on-request",
    }
}

pub fn policy_display_label(policy: &str) -> &'static str {
    match normalize_policy(policy) {
        "auto" => "Auto",
        "never" => "Never",
        "untrusted" => "Untrusted",
        _ => "OnRequest",
    }
}

pub fn next_policy(current: &str) -> &'static str {
    let current = normalize_policy(current);
    let idx = POLICIES.iter().position(|p| *p == current).unwrap_or(0);
    POLICIES[(idx + 1) % POLICIES.len()]
}

/// Mirror `autoApproveFromPolicy` — only explicit `auto` policy auto-approves tools.
pub fn auto_approve_for_turn(policy: &str, yolo: bool) -> bool {
    yolo || normalize_policy(policy) == "auto"
}

pub fn policy_cyclable(yolo: bool) -> bool {
    !yolo
}

pub fn persist_to_config(policy: &str) -> Result<()> {
    let policy = normalize_policy(policy);
    save_approval_policy(policy)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycles_four_policies_in_order() {
        assert_eq!(next_policy("on-request"), "untrusted");
        assert_eq!(next_policy("untrusted"), "never");
        assert_eq!(next_policy("never"), "auto");
        assert_eq!(next_policy("auto"), "on-request");
    }

    #[test]
    fn only_auto_implies_auto_approve() {
        assert!(!auto_approve_for_turn("on-request", false));
        assert!(!auto_approve_for_turn("untrusted", false));
        assert!(!auto_approve_for_turn("never", false));
        assert!(auto_approve_for_turn("auto", false));
        assert!(auto_approve_for_turn("on-request", true));
    }

    #[test]
    fn labels_match_desktop_settings_keys() {
        assert_eq!(policy_display_label("on-request"), "OnRequest");
        assert_eq!(policy_display_label("untrusted"), "Untrusted");
        assert_eq!(policy_display_label("never"), "Never");
        assert_eq!(policy_display_label("auto"), "Auto");
    }
}
