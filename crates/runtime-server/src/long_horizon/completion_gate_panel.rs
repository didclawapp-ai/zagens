//! Completion-gate panel JSON for LHT UI (P2 — composable harness observability).

use serde::Serialize;
use serde_json::Value;

/// Live completion-gate counters for the harness panel (fed by status events + engine session).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(default)]
pub struct CompletionGatePanelJson {
    /// True when `[long_horizon.completion_gate]` manifest is non-empty.
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    /// Generic layer-2: model `[verify:]` replay mode (`off`/`observe`/`enforce`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_verify_replay: Option<String>,
    /// Generic layer-2: toolchain build/test gate mode (`off`/`observe`/`enforce`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain_gate: Option<String>,
    pub manifest_round: u32,
    pub audit_round: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_gap_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_gap_count: Option<u32>,
    pub gate_reinject_while_blocked: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_manifest_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_audit_pass: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_unmet_reason: Option<String>,
}

/// Merge event-derived cache fields with optional live engine session counters.
#[must_use]
pub fn merge_completion_gate_panel(
    cache: &CompletionGatePanelCache,
    session: Option<CompletionGateSessionSnapshot>,
) -> CompletionGatePanelJson {
    let mut out = CompletionGatePanelJson {
        active: cache.active,
        mode: cache.mode.clone(),
        auto_verify_replay: None,
        toolchain_gate: None,
        manifest_round: cache.manifest_round,
        audit_round: cache.audit_round,
        first_gap_count: cache.first_gap_count,
        integration_gap_count: cache.integration_gap_count,
        gate_reinject_while_blocked: cache.gate_reinject_while_blocked,
        last_manifest_passed: cache.last_manifest_passed,
        last_audit_pass: cache.last_audit_pass,
        last_unmet_reason: cache.last_unmet_reason.clone(),
    };
    if let Some(s) = session {
        out.manifest_round = out.manifest_round.max(s.manifest_gate_rounds);
        out.audit_round = out.audit_round.max(s.audit_rounds);
        out.gate_reinject_while_blocked = out
            .gate_reinject_while_blocked
            .max(s.gate_reinject_while_blocked);
        if out.first_gap_count.is_none() {
            out.first_gap_count = s.first_gap_count;
        }
        if out.integration_gap_count.is_none() {
            out.integration_gap_count = s.integration_gap_count;
        }
    }
    out
}

/// Per-thread cache updated from `long_horizon.manifest_gate_*` / `completion_audit` / `audit_unmet`.
#[derive(Debug, Clone, Default)]
pub struct CompletionGatePanelCache {
    pub active: bool,
    pub mode: Option<String>,
    pub manifest_round: u32,
    pub audit_round: u32,
    pub first_gap_count: Option<u32>,
    pub integration_gap_count: Option<u32>,
    pub gate_reinject_while_blocked: u32,
    pub last_manifest_passed: Option<bool>,
    pub last_audit_pass: Option<bool>,
    pub last_unmet_reason: Option<String>,
}

/// Live engine session snapshot when the op loop is reachable.
#[derive(Debug, Clone, Copy)]
pub struct CompletionGateSessionSnapshot {
    pub manifest_gate_rounds: u32,
    pub audit_rounds: u32,
    pub first_gap_count: Option<u32>,
    pub integration_gap_count: Option<u32>,
    pub gate_reinject_while_blocked: u32,
}

impl CompletionGatePanelCache {
    pub fn apply_status(&mut self, message: &str, payload: Option<&Value>) {
        if message.starts_with("long_horizon.manifest_gate_start:") {
            self.active = true;
            if let Some(p) = payload {
                if let Some(m) = p.get("mode").and_then(Value::as_str) {
                    self.mode = Some(m.to_string());
                }
            }
            return;
        }
        if message.starts_with("long_horizon.manifest_gate_result:") {
            if let Some(p) = payload {
                if let Some(v) = p.get("passed").and_then(Value::as_bool) {
                    self.last_manifest_passed = Some(v);
                }
                if let Some(r) = p.get("manifest_round").and_then(Value::as_u64) {
                    self.manifest_round = r as u32;
                }
            }
            return;
        }
        if message.starts_with("long_horizon.completion_audit:") {
            if let Some(p) = payload {
                if let Some(v) = p.get("pass").and_then(Value::as_bool) {
                    self.last_audit_pass = Some(v);
                }
                if let Some(r) = p.get("manifest_round").and_then(Value::as_u64) {
                    self.manifest_round = r as u32;
                }
            }
            return;
        }
        if message.starts_with("long_horizon.audit_unmet:") {
            if let Some(p) = payload {
                if let Some(r) = p.get("reason").and_then(Value::as_str) {
                    self.last_unmet_reason = Some(r.to_string());
                }
                if let Some(r) = p.get("manifest_round").and_then(Value::as_u64) {
                    self.manifest_round = r as u32;
                }
                if let Some(r) = p.get("audit_round").and_then(Value::as_u64) {
                    self.audit_round = r as u32;
                }
                if let Some(g) = p.get("first_gap_count").and_then(Value::as_u64) {
                    self.first_gap_count = Some(g as u32);
                }
            }
            return;
        }
        if message.starts_with("long_horizon.manifest_gate:") {
            if let Some(p) = payload {
                if let Some(r) = p.get("manifest_round").and_then(Value::as_u64) {
                    self.manifest_round = r as u32;
                }
                if let Some(r) = p.get("audit_round").and_then(Value::as_u64) {
                    self.audit_round = r as u32;
                }
                if let Some(g) = p.get("first_gap_count").and_then(Value::as_u64) {
                    self.first_gap_count = Some(g as u32);
                }
                if let Some(g) = p
                    .get("gate_reinject_while_blocked")
                    .and_then(Value::as_u64)
                {
                    self.gate_reinject_while_blocked = g as u32;
                }
                if let Some(m) = p.get("mode").and_then(Value::as_str) {
                    self.mode = Some(m.to_string());
                } else if p.get("enforce").and_then(Value::as_bool) == Some(true) {
                    self.mode = Some("enforce".to_string());
                } else if p.get("observe").and_then(Value::as_bool) == Some(true) {
                    self.mode = Some("observe".to_string());
                }
            }
            return;
        }
        if message.starts_with("long_horizon.integration_gate:") {
            if let Some(p) = payload {
                if let Some(g) = p.get("gap_count").and_then(Value::as_u64) {
                    self.integration_gap_count = Some(g as u32);
                }
                if p.get("enforce").and_then(Value::as_bool) == Some(true) {
                    self.mode = Some("enforce".to_string());
                }
            }
        }
    }
}
