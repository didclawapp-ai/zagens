//! Macro-loop panel JSON for LHT UI (Phase 4 — implement / craft / remediation).

use serde::Serialize;
use serde_json::Value;

use zagens_core::long_horizon::MacroPhase;

use super::nudge::LongHorizonSessionState;

/// Live macro-loop counters for the harness panel.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(default)]
pub struct MacroLoopPanelJson {
    /// `[long_horizon.macro_loop].enabled` in config.
    pub configured: bool,
    /// Macro loop is active for this thread (configured + strict + in-flight state).
    pub active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    pub macro_cycles_used: u32,
    pub craft_rounds_this_cycle: u32,
    pub awaiting_confirm: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_blockers_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macro_task_id: Option<String>,
}

/// Per-thread cache updated from `long_horizon.macro_*` status events.
#[derive(Debug, Clone, Default)]
pub struct MacroLoopPanelCache {
    pub phase: Option<String>,
    pub macro_cycles_used: u32,
    pub craft_rounds_this_cycle: u32,
    pub awaiting_confirm: bool,
    pub last_blockers_count: Option<u32>,
    pub macro_task_id: Option<String>,
}

impl MacroLoopPanelCache {
    pub fn apply_status(&mut self, message: &str, payload: Option<&Value>) {
        if let Some(rest) = message.strip_prefix("long_horizon.macro_phase:") {
            if let Some(p) = payload {
                if let Some(phase) = p.get("phase").and_then(Value::as_str) {
                    self.phase = Some(phase.to_string());
                }
                if let Some(v) = p.get("macro_cycle").and_then(Value::as_u64) {
                    self.macro_cycles_used = v as u32;
                }
                if let Some(v) = p.get("awaiting_confirm").and_then(Value::as_bool) {
                    self.awaiting_confirm = v;
                }
            } else if let Ok(p) = serde_json::from_str::<Value>(rest.trim()) {
                if let Some(phase) = p.get("phase").and_then(Value::as_str) {
                    self.phase = Some(phase.to_string());
                }
                if let Some(v) = p.get("macro_cycle").and_then(Value::as_u64) {
                    self.macro_cycles_used = v as u32;
                }
                if let Some(v) = p.get("awaiting_confirm").and_then(Value::as_bool) {
                    self.awaiting_confirm = v;
                }
            }
            return;
        }
        if message.starts_with("long_horizon.macro_craft_start") {
            self.phase = Some(MacroPhase::Craft.as_str().to_string());
            if let Some(p) = payload
                && let Some(id) = p.get("task_id").and_then(Value::as_str)
            {
                self.macro_task_id = Some(id.to_string());
            }
            return;
        }
        if message.starts_with("long_horizon.macro_craft_result") {
            if let Some(p) = payload
                && let Some(n) = p.get("blockers_count").and_then(Value::as_u64)
            {
                self.last_blockers_count = Some(n as u32);
            }
            return;
        }
        if message.starts_with("long_horizon.macro_unmet") {
            self.phase = Some("unmet".to_string());
            self.awaiting_confirm = false;
        }
    }
}

#[must_use]
pub fn merge_macro_loop_panel(
    configured: bool,
    cache: &MacroLoopPanelCache,
    session: Option<&LongHorizonSessionState>,
) -> MacroLoopPanelJson {
    let (phase, macro_cycles_used, craft_rounds, awaiting_confirm, task_id, blockers) =
        if let Some(s) = session {
            (
                Some(s.macro_phase.as_str().to_string()),
                s.macro_cycles_used,
                s.craft_rounds_this_cycle,
                s.macro_awaiting_confirm,
                s.macro_task_id.clone(),
                None,
            )
        } else {
            (
                cache.phase.clone(),
                cache.macro_cycles_used,
                cache.craft_rounds_this_cycle,
                cache.awaiting_confirm,
                cache.macro_task_id.clone(),
                cache.last_blockers_count,
            )
        };
    let in_flight = awaiting_confirm
        || macro_cycles_used > 0
        || craft_rounds > 0
        || phase
            .as_deref()
            .is_some_and(|p| p != MacroPhase::Implement.as_str() && p != "unmet")
        || cache.last_blockers_count.is_some();
    MacroLoopPanelJson {
        configured,
        active: configured && in_flight,
        phase,
        macro_cycles_used,
        craft_rounds_this_cycle: craft_rounds,
        awaiting_confirm,
        last_blockers_count: blockers.or(cache.last_blockers_count),
        macro_task_id: task_id,
    }
}
