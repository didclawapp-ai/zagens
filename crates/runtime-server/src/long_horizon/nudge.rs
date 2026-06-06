//! Nudge tracker and bilingual continue messages (LHT Phase 1).

use std::collections::{HashMap, HashSet};

use deepseek_core::long_horizon::{LongHorizonConfig, MacroPhase};
use regex::Regex;
use std::sync::LazyLock;

use super::graph::CodeTaskGraph;
use crate::tools::plan::StepStatus;
use crate::tools::todo::TodoStatus;

// Verification-class shell commands recorded for `[verify: cmd]` matching.
// Covers build/lint/format/test/run verbs across common toolchains plus
// script/binary acceptance invocations (`bash …`, `sh …`, `make …`, `./…`)
// so a Go project's `go build`/`go vet`/`gofmt`/`bash scripts/run_examples.sh`
// acceptances are recordable, not just `go test` (DEMO5 #2 — items 12–19 used
// build/vet/fmt/script commands the old narrow pattern could never record).
pub const VERIFICATION_CMD_RE: &str = r"(?i)(\b(cargo\s+(test|check|build|clippy)|go\s+(test|build|vet|run)|gofmt|npm\s+test|pnpm\s+test|yarn\s+test|pytest|make)\b|(?:^|[;&|]\s*)(bash|sh)\s+\S|(?:^|[;&|]\s*)\./\S)";

pub(crate) static VERIFICATION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(VERIFICATION_CMD_RE).expect("VERIFICATION_CMD_RE"));

/// Consecutive no-tool assistant turns on the same in-progress item before the
/// nudge switches to a "steer or update checklist" message (§4.3).
pub(crate) const STALE_ASSISTANT_TURNS: u32 = 8;

/// Hard cap on DEMO3 "unverified acceptance" continue nudges per session — when
/// the task graph is otherwise complete but a completed item is a runnable
/// acceptance never actually verified (`[verify:]`-less + no matching exec), the
/// gate nudges to force real verification. Bounded so a model that genuinely
/// can't (or won't) add `[verify:]` cannot loop the turn forever.
pub(crate) const MAX_UNVERIFIED_ACCEPTANCE_NUDGES: u32 = 2;

/// Hard cap on `[verify:]` **mismatch** continue nudges per session — when a
/// completed item carries a verify prefix but no matching recent exec was
/// recorded (P0-2: "tagged but didn't run"). Bounded like
/// [`MAX_UNVERIFIED_ACCEPTANCE_NUDGES`].
pub(crate) const MAX_VERIFY_MISMATCH_NUDGES: u32 = 2;

/// Hard cap on plan↔checklist drift nudges per session (P1-5).
pub(crate) const MAX_PLAN_CHECKLIST_DRIFT_NUDGES: u32 = 2;

/// P0-3: checklist must have at least this many completed items before the
/// "insufficient `[verify:]`" guard applies (avoids nudging tiny tasks).
pub(crate) const MIN_CHECKLIST_ITEMS_FOR_VERIFY_RATIO: usize = 5;

/// P0-3: at least one completed checklist item must carry `[verify:]` before
/// `graph_complete` when the checklist is large enough.
pub(crate) const MIN_VERIFY_TAGGED_ITEMS: usize = 1;

/// Hard cap on insufficient-verify nudges per session (P0-3).
pub(crate) const MAX_INSUFFICIENT_VERIFY_NUDGES: u32 = 2;

/// Strict-mode plan-bootstrap: max times the runtime forces "establish a plan"
/// while the graph is empty before giving up (honest stop) for this session.
pub(crate) const MAX_PLAN_GATE_ROUNDS: u32 = 3;

/// Strict-mode plan-bootstrap: only force a plan once the thread already shows
/// this many tool calls with no plan/checklist — so trivial single-step or
/// conversational turns are not forced to plan.
pub(crate) const MIN_TOOL_USES_FOR_PLAN_GATE: usize = 3;

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
    /// Session-scoped: checklist item ids already run through the verify gate.
    /// Lets the gate fire **once per completed item** regardless of whether the
    /// model marks it done via per-item `checklist_update` or a bulk
    /// `checklist_write` (DEMO6: items were completed via `checklist_write`, so
    /// the per-item-only gate never fired and emitted no `verify_gate` nodes).
    pub gated_completed_ids: HashSet<u32>,
    /// Session-scoped: number of DEMO3 "unverified acceptance" continue nudges
    /// fired this session (a completed runnable-acceptance item with no
    /// `[verify:]` and no matching recent exec). Persists across user messages
    /// and is bounded by [`MAX_UNVERIFIED_ACCEPTANCE_NUDGES`] so the false-green
    /// guard can't nudge forever when the model won't add a verify command.
    pub unverified_acceptance_nudges: u32,
    /// Session-scoped: number of `[verify:]` **mismatch** continue nudges fired
    /// (completed item has a verify prefix but no matching recent exec). Bounded
    /// by [`MAX_VERIFY_MISMATCH_NUDGES`] (P0-2).
    pub verify_mismatch_nudges: u32,
    /// Session-scoped: plan↔checklist drift nudges (P1-5).
    pub plan_checklist_drift_nudges: u32,
    /// Session-scoped: insufficient `[verify:]` tag nudges (P0-3).
    pub insufficient_verify_nudges: u32,
    /// Session-scoped: cross-layer integration enforce nudges (P1′ electron/ etc.).
    pub integration_gate_rounds: u32,
    /// Git working-tree signature captured when the last nudge was emitted
    /// (§4.8). Compared against the current signature to detect objective,
    /// language-agnostic progress. Reset on new user message.
    pub last_nudge_git_signature: Option<String>,
    /// True between emitting a nudge and observing the next qualified progress —
    /// drives the `converted` telemetry counter (§4.9).
    pub(crate) awaiting_nudge_outcome: bool,
    /// Session-scoped nudge effectiveness telemetry ("先量后调", §4.9).
    pub telemetry: NudgeTelemetry,
    /// Composable harness: layer-2 manifest gate rounds this session.
    pub manifest_gate_rounds: u32,
    /// Generic stub / incompleteness gate enforce rounds this session (bounded
    /// by `max_manifest_rounds` to avoid looping on a stub the model can't fix).
    pub stub_gate_rounds: u32,
    /// Strict-mode plan-bootstrap gate rounds this session: how many times the
    /// runtime has nudged the model to establish a plan/checklist while the task
    /// graph was still empty. Bounded by [`MAX_PLAN_GATE_ROUNDS`] so a model that
    /// refuses to plan stops honestly instead of looping forever.
    pub plan_gate_rounds: u32,
    /// Composable harness: layer-3 deliverable audit rounds this session.
    pub audit_rounds: u32,
    /// Prevents concurrent manifest gate evaluation on the same session (§6.4).
    pub completion_gate_evaluating: bool,
    /// Cached layer-2 result from the latest gate evaluation (same-round trust for layer 3).
    pub last_manifest_gate: Option<super::manifest_gate::ManifestGateResult>,
    /// Cached layer-3 audit from the latest gate evaluation.
    pub last_completion_audit: Option<super::completion_audit::CompletionAuditResult>,
    /// Git signature before harness gate side effects — excludes gate churn from progress (§6.4).
    pub harness_side_effect_signature: Option<String>,
    /// First gate evaluation that recorded any gap (observe/enforce telemetry).
    pub first_gate_gap_count: Option<u32>,
    /// Latest cross-layer integration gap count (P1′).
    pub last_integration_gap_count: Option<u32>,
    /// Enforce reinject while `NudgeTracker` is blocked (§7.8 telemetry).
    pub gate_reinject_while_blocked: u32,
    /// Observe mode: gaps recorded; turn may still complete at `graph_complete`.
    pub completion_gate_observe_pending: bool,
    /// Consecutive layer-2 rounds where all failures are infra-class (§6.4).
    pub consecutive_infra_gate_strikes: u32,
    /// Git signature after gate exec; suppress false progress until workspace moves past it.
    pub suppress_git_progress_baseline: Option<String>,
    /// Gate telemetry drained by `no_tool_uses` after `maybe_continue_incomplete_code_task`.
    pub pending_gate_events: Vec<super::gate_telemetry::CompletionGateEvent>,
    /// Phase 4 macro loop: implement / craft / remediation.
    pub macro_phase: MacroPhase,
    pub macro_cycles_used: u32,
    pub craft_rounds_this_cycle: u32,
    pub macro_task_id: Option<String>,
    /// Harness-spawned CRAFT review sub-agent (awaiting completion).
    pub macro_craft_agent_id: Option<String>,
    /// `user_confirm` mode: waiting for operator to approve CRAFT entry.
    pub macro_awaiting_confirm: bool,
    /// CRAFT was entered while micro gates were still red — remediation must
    /// re-pass manifest/toolchain gates before final completion.
    pub macro_after_audit_unmet: bool,
    /// Layer-2 manifest failures captured when CRAFT is entered after
    /// `audit_unmet` — merged into the remediation nudge alongside CRAFT gaps.
    pub macro_pending_manifest_hints: Vec<String>,
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

const MAX_RECENT_VERIFICATION_CMDS: usize = 24;

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

    /// Record that a completed checklist item has been run through the verify
    /// gate. Returns `true` only the **first** time a given id is seen, so the
    /// gate fires exactly once per completion even when a bulk `checklist_write`
    /// re-sends the same completed items on every call.
    pub fn mark_completion_gated(&mut self, id: u32) -> bool {
        self.gated_completed_ids.insert(id)
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

/// Nudge fired when the task graph is otherwise "complete" but one or more
/// completed checklist items read like a *runnable acceptance* (build / tests
/// pass / run examples) that was never actually verified — no `[verify: cmd]`
/// prefix **and** no matching recent exec. This is the DEMO3 false-green:
/// "create example scripts" marked done without ever running them. We do not
/// touch the completion percentage (it stays 100% — display only); we refuse to
/// let the turn end so the model must verify for real.
/// Layer-2 manifest gate failed — list gates that did not exit 0.
#[must_use]
pub fn build_manifest_failed_nudge(
    failing: &[&super::manifest_gate::VerifyRunResult],
    lang: &str,
) -> String {
    let lines: Vec<String> = failing
        .iter()
        .map(|r| {
            format!(
                "- [{}] {} (exit={}, class={:?})",
                r.id, r.command_display, r.exit_code, r.exit_class
            )
        })
        .collect();
    let list = lines.join("\n");
    let go_tail = go_manifest_failure_appendix(failing, lang);
    if is_zh(lang) {
        format!(
            "任务图已勾选完成，但**规格 manifest 硬验收门未全绿**（harness 主动执行，exit code 为法官）。下列门未通过：\n\n{list}\n\n\
             请逐项修复并**实际跑通**上述命令（看到 exit 0）后再结束本轮。不要仅凭文字声明完成。{go_tail}"
        )
    } else {
        format!(
            "The checklist graph is complete, but **manifest acceptance gates are not all green** \
             (harness actively executed them — exit code is the judge). Failed gates:\n\n{list}\n\n\
             Fix each item and **actually run** these commands to exit 0 before ending this turn. \
             Do not finish on prose alone.{go_tail}"
        )
    }
}

fn go_manifest_failure_appendix(
    failing: &[&super::manifest_gate::VerifyRunResult],
    lang: &str,
) -> String {
    let is_go_test = failing.iter().any(|r| {
        r.id == "toolchain_go_test"
            || r.command_display.contains("go test")
            || r.stderr_tail.contains("coverage")
            || r.stdout_tail.contains("coverage:")
    });
    if !is_go_test {
        return String::new();
    }
    if is_zh(lang) {
        "\n\n**Go 覆盖率提示：** harness 取 `go test -cover ./...` 各包**最低**覆盖率。\
         `cmd/todo`、`examples/*` 若 0% 或 `[no test files]` 会拖死全局——把业务逻辑抽到可测子包并写 `*_test.go`，\
         或直接在 `cmd/*` 下补测试。"
            .to_string()
    } else {
        "\n\n**Go coverage note:** the harness uses the **minimum per-package** coverage from \
         `go test -cover ./...`. A `cmd/todo` or `examples/*` package at 0% or `[no test files]` \
         fails the whole run — extract logic into testable packages or add `*_test.go` under `cmd/*`."
            .to_string()
    }
}

/// Layer-3 deliverable manifest reconciliation failed.
#[must_use]
pub fn build_deliverables_failed_nudge(
    audit: &super::completion_audit::CompletionAuditResult,
    lang: &str,
) -> String {
    let lines: Vec<String> = audit
        .missing_deliverables
        .iter()
        .map(|m| format!("- [{}] {} ({})", m.id, m.what, m.evidence))
        .collect();
    let list = lines.join("\n");
    if is_zh(lang) {
        format!(
            "硬验收门已通过，但**交付物 manifest 对账未通过**（机器路径/glob 命中，非 LLM 判断）。缺失项：\n\n{list}\n\n\
             请补齐上述交付物（生成对应文件/目录并满足 manifest），必要时补充 `[verify:]` 命令并真跑，再 checklist_update。"
        )
    } else {
        format!(
            "Acceptance gates passed, but the **deliverable manifest reconciliation failed** \
             (machine path/glob hits — not LLM judgment). Missing:\n\n{list}\n\n\
             Produce these deliverables in the workspace, add `[verify:]` commands where needed, \
             run them to exit 0, then checklist_update."
        )
    }
}

/// Generic stub / incompleteness gate (enforce): blocking-class markers found.
/// Task-agnostic — fires on the "compiles green but the feature is still a stub"
/// false-completion without any per-task manifest.
#[must_use]
pub fn build_stubs_found_nudge(hits: &[&super::stub_gate::StubHit], lang: &str) -> String {
    const MAX_SHOWN: usize = 12;
    let total = hits.len();
    let lines: Vec<String> = hits
        .iter()
        .take(MAX_SHOWN)
        .map(|h| format!("- {}:{}  `{}`", h.file, h.line, h.snippet))
        .collect();
    let mut list = lines.join("\n");
    if total > MAX_SHOWN {
        list.push_str(&format!("\n- … (+{} more)", total - MAX_SHOWN));
    }
    if is_zh(lang) {
        format!(
            "任务图已勾选完成、代码也能编译，但工作区里仍有 **{total} 处“半成品”标记**（`todo!()` / `unimplemented!()` / `NotImplementedError` / 抛出 \"not implemented\"）——这是典型的“编过但功能缺”的假完成：\n\n{list}\n\n\
             请把每一处真正实现掉（删除占位、补上真实逻辑并跑通对应验收），不要把 stub 留在交付里。如果某一项确属本任务范围之外，请在 checklist 里显式拆出并说明，而不是用占位糊弄过去。全部清零后再结束本轮。"
        )
    } else {
        format!(
            "The checklist graph is complete and the code compiles, but the workspace still contains **{total} \"stub\" markers** (`todo!()` / `unimplemented!()` / `NotImplementedError` / a \"not implemented\" throw) — the classic \"compiles but the feature is missing\" false completion:\n\n{list}\n\n\
             Actually implement each one (remove the placeholder, add the real logic, and run its acceptance to pass). Do not ship stubs. If an item is genuinely out of scope, split it out in the checklist with a reason instead of leaving a placeholder. Clear them all before ending this turn."
        )
    }
}

/// Strict-mode plan-bootstrap nudge: the model has done real work (multiple
/// tool calls) but never authored a plan/checklist, so the whole LHT net is
/// being bypassed. Force it to establish a visible plan before continuing.
#[must_use]
pub fn build_plan_required_nudge(lang: &str) -> String {
    let base = if is_zh(lang) {
        "[长程任务 · 强制模式] 你已经在动手做事,但**还没有建立任何计划/清单**——侧栏 Plan/Todos 是空的,\
         进度无法跟踪,完成门禁也无从校验。现在先停下来用 `checklist_write` 把这件事拆成几个**具体、可验证**的步骤\
         (把第一步标为 `in_progress`,其余 `pending`);复杂任务再用 `update_plan` 给出 3–6 个高层阶段。\
         建好计划后立刻继续推进,并随完成情况更新清单状态。不要在没有计划的情况下闷头往下做。"
            .to_string()
    } else {
        "[Long-horizon · strict mode] You are already doing real work but have **not established any \
         plan/checklist** — the Plan/Todos sidebar is empty, progress can't be tracked, and the \
         completion gate has nothing to verify. Stop now and use `checklist_write` to break this into \
         a few **concrete, verifiable** steps (mark the first `in_progress`, the rest `pending`); for a \
         complex task also lay out 3–6 high-level phases with `update_plan`. Then immediately continue, \
         updating checklist status as you go. Do not keep working without a plan."
            .to_string()
    };
    format!("{base}\n\n{}", build_go_harness_planning_appendix(lang))
}

/// Go code-task harness hints injected at plan-bootstrap (before large impl dumps).
#[must_use]
pub fn build_go_harness_planning_appendix(lang: &str) -> String {
    if is_zh(lang) {
        "**Go 工程门禁（规划时纳入，不要事后补）：**\n\
         - `go test -cover ./...` 按**各包最低**覆盖率 ≥60% 判定；`cmd/*`、`examples/*` 不能 0% 或 `[no test files]`。\n\
         - 示例/Todo 逻辑抽到可测子包（如 `internal/todo`），`main` 只做装配；或给 `cmd/*` 写 `*_test.go`。\n\
         - 关键验收写进 checklist：`[verify: go test -cover ./...]`、`[verify: gofmt -l .]`（输出为空）。\n\
         - manifest 轮次耗尽 ≠ 可收尾；checklist 勾完 ≠ 微观门全绿。"
            .to_string()
    } else {
        "**Go harness (plan for this up front — do not bolt on later):**\n\
         - `go test -cover ./...` uses the **minimum per-package** coverage (≥60%); `cmd/*` and `examples/*` cannot be 0% or `[no test files]`.\n\
         - Extract demo/Todo logic into testable packages (e.g. `internal/todo`); keep `main` as wiring, or add `*_test.go` under `cmd/*`.\n\
         - Put oracles on the checklist: `[verify: go test -cover ./...]`, `[verify: gofmt -l .]` (empty output).\n\
         - Manifest round exhaustion ≠ done; a 100% checklist ≠ micro gates green."
            .to_string()
    }
}

/// Snapshot failing manifest verify tails for macro-loop remediation (ms-5 gap).
pub fn capture_manifest_gate_hints(session: &mut LongHorizonSessionState) {
    let Some(ref gate) = session.last_manifest_gate else {
        session.macro_pending_manifest_hints.clear();
        return;
    };
    let mut hints = Vec::new();
    for r in &gate.results {
        if !gate.failing_ids.contains(&r.id) {
            continue;
        }
        let detail = format!(
            "[{}] {} — {}",
            r.id,
            r.command_display,
            r.stderr_tail.trim()
        );
        if !r.stderr_tail.trim().is_empty() {
            hints.push(detail);
        } else if !r.stdout_tail.trim().is_empty() {
            hints.push(format!(
                "[{}] {} — {}",
                r.id,
                r.command_display,
                r.stdout_tail.trim()
            ));
        }
    }
    session.macro_pending_manifest_hints = hints;
}

#[must_use]
pub fn build_unverified_acceptance_nudge(items: &[String], lang: &str) -> String {
    let list = items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    if is_zh(lang) {
        format!(
            "清单已全部勾选，但下面这些“可运行的验收”项并没有被真正验证过 —— 它们没有 `[verify: <命令>]` 前缀，也没有匹配的近期执行记录（创建文件 / 自述完成 ≠ 跑通）：\n\n{list}\n\n\
             请对每一项：① 改写为 `[verify: <命令>] <描述>`（如 `[verify: bash scripts/run_examples.sh] 全部示例跑通`）；② **实际运行该命令并看到通过输出**后再保持 completed。若确实没有可运行命令，请把它拆成有客观验收的子项。不要仅凭文字声明结束本轮。"
        )
    } else {
        format!(
            "The checklist is fully checked, but these \"runnable acceptance\" items were never actually verified — they have no `[verify: <command>]` prefix and no matching recent run (creating a file / self-declaring done is NOT the same as running it):\n\n{list}\n\n\
             For each: (1) rewrite as `[verify: <command>] <label>` (e.g. `[verify: bash scripts/run_examples.sh] all examples pass`); (2) **run that command and see it pass** before keeping it completed. If there is genuinely no runnable command, split it into sub-items with objective acceptance. Do not end this turn on a prose claim alone."
        )
    }
}

/// P0-3: checklist is complete but carries almost no `[verify:]` tags.
#[must_use]
pub fn build_insufficient_verify_nudge(completed_count: usize, lang: &str) -> String {
    if is_zh(lang) {
        format!(
            "清单已勾选 {completed_count} 项，但**没有任何一项**带 `[verify: <命令>]` 前缀。\
             仅靠 build/vet/自述完成无法证明任务真做完（MicroStack 类假绿）。\n\n\
             请至少为关键验收项补上可执行 oracle，例如：\n\
             - `[verify: go test -cover ./...] 测试覆盖率 ≥60%`\n\
             - `[verify: gofmt -l .] 格式检查（输出为空）`\n\
             - `[verify: bash scripts/e2e_todo.sh] Todo 端到端`\n\n\
             写好前缀后**实际运行命令、看到 exit 0**，再保持 completed。不要仅凭文字声明结束本轮。"
        )
    } else {
        format!(
            "The checklist has {completed_count} completed items but **none** carry a `[verify: <command>]` prefix. \
             Build/vet/prose claims alone do not prove the task is done (MicroStack-style false green).\n\n\
             Add executable oracles for key acceptance items, e.g.:\n\
             - `[verify: go test -cover ./...] tests with ≥60% coverage`\n\
             - `[verify: gofmt -l .] formatting clean (empty output)`\n\
             - `[verify: bash scripts/e2e_todo.sh] Todo e2e`\n\n\
             Then **run each command and see exit 0** before keeping items completed. Do not end this turn on prose alone."
        )
    }
}

/// P0-2: completed checklist items tagged `[verify: cmd]` but with no matching
/// recent exec — force a real run before the turn may end on `graph_complete`.
#[must_use]
pub fn build_verify_mismatch_nudge(items: &[(String, String)], lang: &str) -> String {
    let list = items
        .iter()
        .map(|(label, cmd)| format!("- `{cmd}` — {label}"))
        .collect::<Vec<_>>()
        .join("\n");
    if is_zh(lang) {
        format!(
            "清单已全部勾选，但下面这些项带有 `[verify: …]` 前缀，却**没有匹配的近期执行记录**（贴标签 ≠ 真跑过）：\n\n{list}\n\n\
             请对每一项：**实际运行 verify 命令并看到通过输出**（exit 0）后再保持 completed；或撤销 completed、先跑命令再勾选。不要仅凭文字声明结束本轮。"
        )
    } else {
        format!(
            "The checklist is fully checked, but these items have a `[verify: …]` prefix with **no matching recent run** (tagging is NOT the same as running):\n\n{list}\n\n\
             For each: **run the verify command and see it pass** (exit 0) before keeping it completed; or revert to pending, run, then mark done. Do not end this turn on a prose claim alone."
        )
    }
}

/// P1-5: checklist claims a plan phase is done while that plan step is still open.
#[must_use]
pub fn build_plan_checklist_drift_nudge(items: &[String], lang: &str) -> String {
    let list = items
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    if is_zh(lang) {
        format!(
            "清单已全部勾选，但 plan 与 checklist **不同步** —— 下列 plan 阶段仍为 pending/in_progress，却已有对应 checklist 完成项：\n\n{list}\n\n\
             请用 `update_plan` 把已完成阶段标为 completed，或撤销 checklist 中过早勾选的项。Plan 与 checklist 必须反映同一真实进度后再结束本轮。"
        )
    } else {
        format!(
            "The checklist is fully checked, but **plan and checklist are out of sync** — these plan phases are still pending/in_progress while matching checklist items are completed:\n\n{list}\n\n\
             Use `update_plan` to mark finished phases completed, or revert prematurely checked checklist items. Plan and checklist must reflect the same real progress before ending this turn."
        )
    }
}

/// P1′: strict/enforce cross-layer gaps (electron/ still present, missing adapter, …).
#[must_use]
pub fn build_integration_incomplete_nudge(gaps: &[String], lang: &str) -> String {
    let list = gaps
        .iter()
        .map(|s| format!("- {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    if is_zh(lang) {
        format!(
            "清单已全部勾选，但**跨层集成门禁**仍发现硬性缺口：\n\n{list}\n\n\
             请逐项修复（删 `electron/`、补前端 adapter、`[verify: npm run build]` 等）后再保持 completed。不要仅凭 prose 声明迁移完成。"
        )
    } else {
        format!(
            "The checklist is fully checked, but the **cross-layer integration gate** still reports hard gaps:\n\n{list}\n\n\
             Fix each item (remove `electron/`, add the frontend adapter, `[verify: npm run build]`, etc.) before keeping items completed. Do not end this turn on a prose-only migration claim."
        )
    }
}

/// "一推到底" auto-continue override (C2). Fired when the in-turn nudge gate has
/// already given up (tracker `blocked` / `max_nudges`) but the task graph is
/// still genuinely incomplete and `auto_continue` is enabled. Far more forceful
/// than the routine nudge: it spells out the only two valid stop conditions so
/// the model resumes the next phase instead of summarizing. Bounded per turn by
/// `max_auto_continue_rounds`.
#[must_use]
pub fn build_auto_continue_message(graph: &CodeTaskGraph, round: u32, lang: &str) -> String {
    let progress_bar = progress_bar(graph.completion_pct);
    let open_lines = format_open_items(graph);
    let pct = graph.completion_pct;
    if is_zh(lang) {
        format!(
            "[长程任务 — 自动续跑 #{round}] 任务尚未完成，不要停下来总结或等待确认。\n\n\
             进度：{progress_bar} {pct}%\n\
             仍待完成：\n{open_lines}\n\n\
             立刻继续推进下一阶段并用工具实际完成、验证（如 cargo check/test、运行验收命令看到 exit 0），\
             再 checklist_update / update_plan。写 handoff、cycle 切换、清单清空都是\"继续\"的信号，不是停下来的理由。\n\
             只有在以下两种情况才允许停下：①遇到必须由用户决策的真实阻断（互斥方案二选一、与业务规则冲突）；\
             ②全部阶段已完成且通过验证。除此之外，继续。"
        )
    } else {
        format!(
            "[Long-horizon — auto-continue #{round}] The task is not finished. Do not stop to \
             summarize or wait for confirmation.\n\n\
             Progress: {progress_bar} {pct}%\n\
             Still open:\n{open_lines}\n\n\
             Immediately resume the next phase: complete it with tools and verify for real \
             (e.g. cargo check/test, run the acceptance command and see exit 0), then \
             checklist_update / update_plan. Writing handoff, a cycle boundary, or an emptied \
             checklist are all signals to KEEP GOING, not to stop.\n\
             You may stop ONLY when: (1) a genuine blocker needs a user decision (two mutually \
             exclusive approaches, a conflict with business rules), or (2) every phase is done \
             and verified. Otherwise, continue."
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
    fn verification_cmd_matches_broadened_verbs() {
        // DEMO5 #2: build/vet/fmt/run + script/binary acceptances must record.
        assert!(VERIFICATION_RE.is_match("go build ./..."));
        assert!(VERIFICATION_RE.is_match("go vet ./..."));
        assert!(VERIFICATION_RE.is_match("go run ./cmd/monkey"));
        assert!(VERIFICATION_RE.is_match("gofmt -l ."));
        assert!(VERIFICATION_RE.is_match("go test -cover ./..."));
        assert!(VERIFICATION_RE.is_match("make build"));
        assert!(VERIFICATION_RE.is_match("bash scripts/run_examples.sh"));
        assert!(VERIFICATION_RE.is_match("sh scripts/conformance.sh"));
        assert!(VERIFICATION_RE.is_match("./monkey run examples/fibonacci.monkey --engine=vm"));
        assert!(VERIFICATION_RE.is_match("cd /tmp && bash run.sh"));
        // Non-verification noise stays out.
        assert!(!VERIFICATION_RE.is_match("ls -la"));
        assert!(!VERIFICATION_RE.is_match("echo hello"));
        assert!(!VERIFICATION_RE.is_match("cat go.mod"));
    }

    #[test]
    fn unverified_acceptance_nudge_lists_items_bilingual() {
        let items = vec![
            "全部 8 个示例跑通".to_string(),
            "go build / vet / test 全绿".to_string(),
        ];
        let zh = build_unverified_acceptance_nudge(&items, "zh-Hans");
        assert!(zh.contains("[verify:"));
        assert!(zh.contains("全部 8 个示例跑通"));
        assert!(zh.contains("go build / vet / test 全绿"));
        let en = build_unverified_acceptance_nudge(&items, "en");
        assert!(en.contains("[verify:"));
        assert!(en.contains("run that command and see it pass"));
    }

    #[test]
    fn verify_mismatch_nudge_lists_cmd_and_label() {
        let items = vec![(
            "cargo check".to_string(),
            "cargo check --manifest-path src-tauri/Cargo.toml".to_string(),
        )];
        let zh = build_verify_mismatch_nudge(&items, "zh-Hans");
        assert!(zh.contains("cargo check --manifest-path"));
        assert!(zh.contains("贴标签"));
        let en = build_verify_mismatch_nudge(&items, "en");
        assert!(en.contains("no matching recent run"));
    }

    #[test]
    fn auto_continue_message_is_forceful_and_bilingual() {
        use super::super::graph::{CodeTaskGraph, GraphChecklistItem};
        let graph = CodeTaskGraph {
            objective: "refactor".to_string(),
            objective_source: "test",
            phases: Vec::new(),
            checklist: vec![GraphChecklistItem {
                id: 2,
                content: "phase 2: extract module".to_string(),
                status: TodoStatus::InProgress,
            }],
            completion_pct: 40,
            open_items: 1,
            in_progress_id: Some(2),
        };
        let zh = build_auto_continue_message(&graph, 3, "zh-Hans");
        assert!(zh.contains("自动续跑 #3"));
        assert!(zh.contains("phase 2: extract module"));
        assert!(zh.contains("继续"));
        let en = build_auto_continue_message(&graph, 1, "en");
        assert!(en.contains("auto-continue #1"));
        assert!(en.contains("KEEP GOING"));
        assert!(en.contains("phase 2: extract module"));
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
        let cfg = LongHorizonConfig::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            mode: deepseek_core::long_horizon::LhtMode::Auto,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: cfg.reinject_every_steps,
            progress_via_git: true,
            auto_continue: cfg.auto_continue,
            max_auto_continue_rounds: cfg.max_auto_continue_rounds,
            completion_gate: cfg.completion_gate,
            macro_loop: cfg.macro_loop,
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
        assert_eq!(
            tracker.prepare_nudge(Some(1), &cfg, false),
            NudgeDecision::Blocked
        );
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
        let cfg = LongHorizonConfig::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            mode: deepseek_core::long_horizon::LhtMode::Auto,
            max_nudges_per_item: 5,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: cfg.reinject_every_steps,
            progress_via_git: true,
            auto_continue: cfg.auto_continue,
            max_auto_continue_rounds: cfg.max_auto_continue_rounds,
            completion_gate: cfg.completion_gate,
            macro_loop: cfg.macro_loop,
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
        let defaults = LongHorizonConfig::default();
        let cfg = LongHorizonConfig {
            enabled: true,
            mode: deepseek_core::long_horizon::LhtMode::Auto,
            max_nudges_per_item: 100,
            blocked_nudges_without_progress: 3,
            reinject_every_steps: 0,
            progress_via_git: true,
            auto_continue: defaults.auto_continue,
            max_auto_continue_rounds: defaults.max_auto_continue_rounds,
            completion_gate: defaults.completion_gate,
            macro_loop: defaults.macro_loop,
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
