//! Nudge tracker and bilingual continue messages (LHT Phase 1).

use std::collections::HashMap;

use deepseek_core::long_horizon::LongHorizonConfig;
use regex::Regex;
use std::sync::LazyLock;

use super::graph::CodeTaskGraph;
use crate::tools::plan::StepStatus;
use crate::tools::todo::TodoStatus;

pub const VERIFICATION_CMD_RE: &str =
    r"(?i)\b(cargo\s+(test|check|build|clippy)|npm\s+test|pnpm\s+test|yarn\s+test|pytest|go\s+test)\b";

pub(crate) static VERIFICATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(VERIFICATION_CMD_RE).expect("VERIFICATION_CMD_RE"));

/// Consecutive no-tool assistant turns on the same in-progress item before the
/// nudge switches to a "steer or update checklist" message (§4.3).
pub(crate) const STALE_ASSISTANT_TURNS: u32 = 8;

/// Per-session LHT state.
///
/// Lifetime conventions (see [`Self::on_new_user_message`]):
/// - **Turn/request-scoped** — reset on every new user message: [`Self::tracker`]
///   counters, [`Self::paused`], [`Self::stale_assistant_turns`],
///   `progress_since_last_nudge`, [`Self::pending_tool_result_suffix`].
/// - **Session-scoped** — intentionally persist across user messages for the
///   whole conversation (only reset on a fresh session / cycle rebuild):
///   [`Self::assistant_steps`], [`Self::recent_verification_cmds`],
///   [`Self::pending_cycle_at_checkpoint`], [`Self::last_warning_band_emitted`].
#[derive(Debug, Clone, Default)]
pub struct LongHorizonSessionState {
    pub tracker: NudgeTracker,
    pub paused: bool,
    pub stale_assistant_turns: u32,
    pub(crate) progress_since_last_nudge: bool,
    /// Session-scoped: assistant steps since session start, paces
    /// `reinject_every_steps`. Persists across user messages so the re-inject
    /// cadence is steady over a long conversation (not reset per turn).
    pub assistant_steps: u32,
    /// Set when checklist/plan marks an item completed in the warning band.
    pub pending_cycle_at_checkpoint: bool,
    /// Last emitted context pressure band (avoid duplicate warning events).
    pub last_warning_band_emitted: bool,
    /// Session-scoped: recent verification-class shell commands (normalized),
    /// newest last, capped at `MAX_RECENT_VERIFICATION_CMDS` (LRU). Persists
    /// across user messages so a later `[verify:]` completed-check can match an
    /// earlier run; the LRU cap bounds staleness.
    pub recent_verification_cmds: Vec<String>,
    /// Appended to the next tool result body (e.g. verify mismatch warning).
    pub pending_tool_result_suffix: Option<String>,
    /// Git working-tree signature captured when the last nudge was emitted
    /// (§4.8). Compared against the current signature to detect objective,
    /// language-agnostic progress. Reset on new user message.
    pub last_nudge_git_signature: Option<String>,
    /// True between emitting a nudge and observing the next qualified progress —
    /// drives the `converted` telemetry counter (§4.9).
    pub(crate) awaiting_nudge_outcome: bool,
    /// Session-scoped nudge effectiveness telemetry ("先量后调", §4.9).
    pub telemetry: NudgeTelemetry,
}

/// In-memory nudge effectiveness counters (§4.9 — evidence for tuning, not yet
/// persisted across sessions).
#[derive(Debug, Clone, Default)]
pub struct NudgeTelemetry {
    /// Continue nudges actually injected this session.
    pub emitted: u32,
    /// Nudges followed by qualified progress before the next nudge.
    pub converted: u32,
    /// Times the tracker entered the `blocked` (gave-up) state.
    pub blocked: u32,
}

impl NudgeTelemetry {
    /// `converted / emitted` as a clamped percentage (0 when nothing emitted).
    #[must_use]
    pub fn conversion_pct(&self) -> u8 {
        if self.emitted == 0 {
            0
        } else {
            ((u64::from(self.converted) * 100) / u64::from(self.emitted)).min(100) as u8
        }
    }
}

const MAX_RECENT_VERIFICATION_CMDS: usize = 12;

impl LongHorizonSessionState {
    pub fn on_new_user_message(&mut self) {
        self.tracker.clear_blocked();
        self.paused = false;
        self.stale_assistant_turns = 0;
        self.progress_since_last_nudge = false;
        self.pending_tool_result_suffix = None;
        // "Since last nudge" baseline resets per user turn; the first nudge in a
        // fresh turn therefore has no git baseline and never false-positives.
        self.last_nudge_git_signature = None;
        self.awaiting_nudge_outcome = false;
    }

    pub fn record_verification_exec(&mut self, command: &str) {
        let norm = super::verify::normalize_cmd(command);
        if norm.is_empty() {
            return;
        }
        self.recent_verification_cmds.retain(|c| c != &norm);
        self.recent_verification_cmds.push(norm);
        if self.recent_verification_cmds.len() > MAX_RECENT_VERIFICATION_CMDS {
            let drop = self.recent_verification_cmds.len() - MAX_RECENT_VERIFICATION_CMDS;
            self.recent_verification_cmds.drain(0..drop);
        }
    }

    pub fn take_tool_result_suffix(&mut self) -> Option<String> {
        self.pending_tool_result_suffix.take()
    }

    pub fn on_steer(&mut self, text: &str) {
        if is_stop_steer(text) {
            self.paused = true;
        }
    }

    pub fn on_assistant_no_tools(&mut self) {
        self.stale_assistant_turns = self.stale_assistant_turns.saturating_add(1);
    }

    pub fn on_assistant_with_tools(&mut self) {
        self.stale_assistant_turns = 0;
    }
}

/// Heuristic: does this steer express "stop / pause the long-horizon loop"?
///
/// Relaxed beyond exact equality (§ review): matches common Chinese stop
/// phrases as substrings and English stop verbs as whole words (so `stopwatch`
/// or `pauses` inside a larger word do not trigger a false pause).
fn is_stop_steer(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    const ZH_STOP: [&str; 6] = ["暂停", "先停", "停一下", "停一停", "停下", "停止"];
    if ZH_STOP.iter().any(|kw| trimmed.contains(kw)) {
        return true;
    }
    const EN_STOP: [&str; 4] = ["stop", "pause", "halt", "abort"];
    let lower = trimmed.to_ascii_lowercase();
    lower
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| EN_STOP.contains(&word))
}

#[derive(Debug, Clone, Default)]
pub struct NudgeTracker {
    /// Consecutive no-progress nudge streak per item — reset on qualified
    /// progress; drives the `blocked` (gave-up) signal (§4.3).
    no_progress_streak: HashMap<u32, u32>,
    /// Total nudges ever emitted per item — NOT reset by progress; drives the
    /// absolute `max_nudges_per_item` hard cap so the knob is reachable even
    /// when the model dodges `blocked` with intermittent progress (§4.3 #5).
    total_per_item: HashMap<u32, u32>,
    last_in_progress_id: Option<u32>,
    blocked: bool,
}

impl NudgeTracker {
    pub fn clear_blocked(&mut self) {
        self.blocked = false;
        self.no_progress_streak.clear();
        self.total_per_item.clear();
        self.last_in_progress_id = None;
    }

    #[must_use]
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }

    #[must_use]
    pub fn max_item_nudge_count(&self) -> u32 {
        self.total_per_item.values().copied().max().unwrap_or(0)
    }

    /// Returns whether a nudge may be sent; updates counters when `true`.
    pub fn prepare_nudge(
        &mut self,
        in_progress_id: Option<u32>,
        config: &LongHorizonConfig,
        had_progress: bool,
    ) -> NudgeDecision {
        if self.blocked {
            return NudgeDecision::Blocked;
        }
        let Some(id) = in_progress_id else {
            return NudgeDecision::Skip;
        };

        if self.last_in_progress_id != Some(id) {
            self.last_in_progress_id = Some(id);
            self.no_progress_streak.remove(&id);
            self.total_per_item.remove(&id);
        }

        // Qualified progress only protects against the `blocked` give-up state by
        // clearing the no-progress streak. It does **not** skip the nudge: the
        // gate only fires when the model stopped (no tool calls) with the task
        // still incomplete, and "did some work, then quit mid-task" is exactly
        // the cognitive early-stop LHT exists to catch. The absolute
        // `max_nudges_per_item` cap (below) still bounds total nudges so a model
        // making intermittent progress cannot be nudged forever.
        if had_progress {
            self.no_progress_streak.remove(&id);
        }

        // Absolute ceiling on total nudges for this item, checked before
        // emitting another nudge (reachable independent of `blocked`).
        let total = self.total_per_item.entry(id).or_insert(0);
        if *total >= config.max_nudges_per_item {
            return NudgeDecision::MaxReached;
        }
        *total = total.saturating_add(1);
        let total_now = *total;

        // Only no-progress nudges accumulate the streak toward `blocked`; a turn
        // with qualified progress must never push the item closer to give-up.
        if !had_progress {
            let streak = self.no_progress_streak.entry(id).or_insert(0);
            *streak = streak.saturating_add(1);
            if *streak > config.blocked_nudges_without_progress {
                self.blocked = true;
                return NudgeDecision::Blocked;
            }
        }

        NudgeDecision::Nudge {
            nudge_count: total_now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NudgeDecision {
    Nudge { nudge_count: u32 },
    Blocked,
    MaxReached,
    Skip,
}

#[must_use]
pub(crate) fn result_contains_success(result: &str) -> bool {
    let lower = result.to_ascii_lowercase();
    lower.contains("exit code: 0")
        || lower.contains("exit_code\":0")
        || lower.contains("\"exit_code\": 0")
        || lower.contains("success\": true")
        || lower.contains("success: true")
}

#[must_use]
pub fn build_nudge_message(
    graph: &CodeTaskGraph,
    objective: &str,
    lang: &str,
    turn_limit_warning: bool,
    stale: bool,
) -> String {
    if stale {
        return build_stale_message(lang);
    }
    let progress_bar = progress_bar(graph.completion_pct);
    let open_lines = format_open_items(graph);
    let limit = if turn_limit_warning {
        turn_limit_line(lang)
    } else {
        String::new()
    };
    let pct = graph.completion_pct;
    let plan_total = graph.phases.len();
    let plan_done = graph
        .phases
        .iter()
        .filter(|p| p.status == StepStatus::Completed)
        .count();
    let todo_open = graph
        .checklist
        .iter()
        .filter(|c| c.status != TodoStatus::Completed)
        .count();

    if is_zh(lang) {
        format!(
            "长程代码任务尚未完成 — 请勿仅用文字总结结束本轮。\n\n\
             目标：{objective}\n\
             进度：{progress_bar} {pct}%（plan {plan_done}/{plan_total} 阶段；checklist {todo_open} 项未完成）\n\n\
             仍待完成：\n{open_lines}\n\n\
             请继续用工具完成当前 in_progress 项，验证（如 cargo check/test），再 checklist_update / update_plan。{limit}"
        )
    } else {
        format!(
            "Long-horizon code task incomplete — do **not** end this turn with prose-only output.\n\n\
             Objective: {objective}\n\
             Progress: {progress_bar} {pct}% (plan {plan_done}/{plan_total} phases done; checklist {todo_open} items open)\n\n\
             Still open:\n{open_lines}\n\n\
             Continue with tools: complete the current in-progress item, verify (e.g. cargo check/test), \
             then checklist_update / update_plan before summarizing again.{limit}"
        )
    }
}

fn build_stale_message(lang: &str) -> String {
    if is_zh(lang) {
        "长程任务 checklist 项长时间无工具进展 — 请 steer 调整目标，或用 checklist_update 更新状态；勿重复 prose 收尾。"
            .to_string()
    } else {
        "Long-horizon checklist item stale — steer to reprioritize or checklist_update; \
         do not end with prose-only output again."
            .to_string()
    }
}

fn progress_bar(pct: u8) -> String {
    let filled = ((pct as usize) / 10).min(10);
    let mut bar = String::new();
    for i in 0..10 {
        bar.push(if i < filled { '█' } else { '░' });
    }
    bar
}

fn format_open_items(graph: &CodeTaskGraph) -> String {
    let mut lines = Vec::new();
    for phase in &graph.phases {
        if phase.status == StepStatus::Completed {
            continue;
        }
        let sym = match phase.status {
            StepStatus::InProgress => "◎",
            StepStatus::Pending => "○",
            StepStatus::Completed => "●",
        };
        lines.push(format!("- [plan {sym}] {}", phase.step));
    }
    for item in &graph.checklist {
        if item.status == TodoStatus::Completed {
            continue;
        }
        let sym = match item.status {
            TodoStatus::InProgress => "◎",
            TodoStatus::Pending => "○",
            TodoStatus::Completed => "●",
        };
        lines.push(format!("- [todo {sym}] {}", item.content));
    }
    if lines.is_empty() {
        "- (none)".to_string()
    } else {
        lines.join("\n")
    }
}

fn turn_limit_line(lang: &str) -> String {
    if is_zh(lang) {
        "\n\n接近 turn 步数上限 — 考虑 cycle 刷新或 steer。".to_string()
    } else {
        "\n\nApproaching turn step limit — consider cycle refresh or steer.".to_string()
    }
}

fn is_zh(lang: &str) -> bool {
    lang.trim().to_ascii_lowercase().starts_with("zh")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_cmd_matches_cargo_test() {
        assert!(VERIFICATION_RE.is_match("cargo test -p auth"));
        assert!(!VERIFICATION_RE.is_match("ls -la"));
    }

    #[test]
    fn stop_steer_matches_phrases_not_substrings() {
        assert!(is_stop_steer("stop"));
        assert!(is_stop_steer("please stop here"));
        assert!(is_stop_steer("Pause for now"));
        assert!(is_stop_steer("先停一下"));
        assert!(is_stop_steer("暂停"));
        assert!(is_stop_steer("停一下，我想想"));
        assert!(!is_stop_steer("keep going"));
        assert!(!is_stop_steer("check the stopwatch value"));
        assert!(!is_stop_steer(""));
    }

    #[test]
    fn progress_bar_fills_proportionally() {
        assert_eq!(progress_bar(0).matches('█').count(), 0);
        assert_eq!(progress_bar(42).matches('█').count(), 4);
        assert_eq!(progress_bar(100).matches('█').count(), 10);
        assert_eq!(progress_bar(100).chars().count(), 10);
    }

    #[test]
    fn blocked_after_three_nudges_without_progress() {
        let mut tracker = NudgeTracker::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
        };
        for _ in 0..3 {
            assert!(matches!(
                tracker.prepare_nudge(Some(1), &cfg, false),
                NudgeDecision::Nudge { .. }
            ));
        }
        // Fourth attempt: over blocked_nudges_without_progress (3).
        assert_eq!(
            tracker.prepare_nudge(Some(1), &cfg, false),
            NudgeDecision::Blocked
        );
        // Fourth attempt stays blocked without injecting.
        assert_eq!(tracker.prepare_nudge(Some(1), &cfg, false), NudgeDecision::Blocked);
    }

    #[test]
    fn in_progress_change_resets_count() {
        let mut tracker = NudgeTracker::default();
        let cfg = LongHorizonConfig::default();
        let _ = tracker.prepare_nudge(Some(1), &cfg, false);
        let _ = tracker.prepare_nudge(Some(2), &cfg, false);
        assert_eq!(tracker.no_progress_streak.get(&2), Some(&1));
        assert_eq!(tracker.total_per_item.get(&2), Some(&1));
    }

    #[test]
    fn progress_nudges_but_never_blocks() {
        // A model that makes qualified progress every turn but keeps stopping
        // (no tool calls, task incomplete) must STILL be nudged each turn — that
        // is the "did work, then quit mid-task" early-stop LHT catches. Progress
        // only protects against `blocked`; the hard total cap (5) is what stops
        // the nudging, never give-up.
        let mut tracker = NudgeTracker::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
        };
        let mut nudges = 0;
        for _ in 0..20 {
            match tracker.prepare_nudge(Some(7), &cfg, true) {
                NudgeDecision::Nudge { .. } => nudges += 1,
                NudgeDecision::MaxReached => break,
                other => panic!("unexpected {other:?}"),
            }
        }
        assert!(!tracker.is_blocked(), "progress must never block");
        assert_eq!(nudges, 5, "hard cap stops after max_nudges_per_item");
        assert_eq!(
            tracker.prepare_nudge(Some(7), &cfg, true),
            NudgeDecision::MaxReached
        );
    }

    #[test]
    fn progress_resets_no_progress_streak() {
        // Interleaving a progress turn resets the streak so `blocked` is avoided,
        // while no-progress turns still accumulate toward give-up.
        let mut tracker = NudgeTracker::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            max_nudges_per_item: 100,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
        };
        // Two no-progress nudges (streak = 2), then a progress turn resets it.
        let _ = tracker.prepare_nudge(Some(1), &cfg, false);
        let _ = tracker.prepare_nudge(Some(1), &cfg, false);
        assert_eq!(tracker.no_progress_streak.get(&1), Some(&2));
        let _ = tracker.prepare_nudge(Some(1), &cfg, true);
        assert_eq!(
            tracker.no_progress_streak.get(&1),
            None,
            "progress clears streak"
        );
        assert!(!tracker.is_blocked());
    }
}
