//! Phase 4 — LHT↔CRAFT macro loop orchestration (implement → review → remediation).

use std::path::Path;

use zagens_core::chat::{ContentBlock, Message};
use zagens_core::long_horizon::{AutoEnterCraft, LhtMode, MacroLoopConfig, MacroPhase};
use zagens_core::subagent::VerdictItem;

use crate::tools::subagent::blackboard;
use crate::tools::todo::{TodoItem, TodoListSnapshot, TodoStatus};

use super::LhtGateOutcome;
use super::nudge::LongHorizonSessionState;

/// What caused macro-loop evaluation (micro pass vs graph done but gates red).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroTrigger {
    /// Checklist/plan graph complete; micro gates may still be red.
    GraphComplete,
    MicroPass,
    AuditUnmet {
        reason: &'static str,
    },
}

/// Outcome of macro-loop evaluation after a eligible trigger fires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MacroLoopOutcome {
    /// Macro loop inactive — caller may end the turn on `graph_complete`.
    Inactive,
    /// Wait for the user to confirm entering CRAFT (turn may end).
    AwaitUserConfirm,
    /// Harness will spawn (or has spawned) a CRAFT Review sub-agent.
    CraftSpawnRequested { task_id: String },
    /// Macro cycles exhausted — honest stop with remaining gaps listed.
    Unmet {
        remaining_blockers: Vec<String>,
        macro_cycles_used: u32,
    },
}

pub struct MacroLoopInput<'a> {
    pub config: &'a MacroLoopConfig,
    pub effective_mode: LhtMode,
    pub workspace: &'a Path,
    pub checklist: &'a TodoListSnapshot,
    pub session: &'a mut LongHorizonSessionState,
    pub lang: &'a str,
    pub thread_id: &'a str,
    pub latest_user_text: Option<&'a str>,
}

/// Stable dedup key for a CRAFT blocker → checklist mapping.
#[must_use]
pub fn blocker_stable_key(item: &VerdictItem) -> String {
    let line = item.line.map(|l| l.to_string()).unwrap_or_default();
    let rule = item.rule.as_deref().unwrap_or("");
    format!("craft:{}:{}:{}:{}", item.file, line, rule, item.description)
}

/// Convert CRAFT blockers into new checklist row contents (idempotent vs existing).
#[must_use]
pub fn blockers_to_checklist_contents(
    blockers: &[VerdictItem],
    existing: &[TodoItem],
) -> Vec<String> {
    let existing_keys: std::collections::HashSet<String> = existing
        .iter()
        .filter(|i| i.status != TodoStatus::Completed)
        .map(|i| i.content.trim().to_string())
        .collect();

    let mut out = Vec::new();
    for b in blockers {
        let content = format_blocker_checklist_content(b);
        if existing_keys.contains(&content) {
            continue;
        }
        let key = blocker_stable_key(b);
        if existing
            .iter()
            .any(|i| i.content.contains(&key) || i.content == content)
        {
            continue;
        }
        out.push(content);
    }
    out
}

fn format_blocker_checklist_content(item: &VerdictItem) -> String {
    let line = item.line.map(|l| format!(":{l}")).unwrap_or_default();
    let key = blocker_stable_key(item);
    format!(
        "CRAFT gap [{key}]: {desc} — `{file}{line}`",
        desc = item.description.trim(),
        file = item.file.trim(),
    )
}

/// Append new gap items to the in-memory checklist (pending).
pub async fn apply_blockers_to_checklist(
    todos: &crate::tools::todo::SharedTodoList,
    blockers: &[VerdictItem],
) -> u32 {
    let mut list = todos.lock().await;
    let snapshot = list.snapshot();
    let contents = blockers_to_checklist_contents(blockers, &snapshot.items);
    let count = contents.len() as u32;
    for content in contents {
        list.add(content, TodoStatus::Pending);
    }
    count
}

#[must_use]
pub fn is_macro_confirm_steer(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.eq_ignore_ascii_case("/lht-craft-go")
        || t.eq_ignore_ascii_case("lht-craft-go")
        || t.contains("开始审查")
        || t.contains("start craft review")
        || t.contains("start review round")
    {
        return true;
    }
    false
}

#[must_use]
pub fn is_macro_skip_steer(text: &str) -> bool {
    let t = text.trim();
    t.contains("跳过审查")
        || t.contains("skip craft")
        || t.contains("skip review")
        || t.eq_ignore_ascii_case("/lht-craft-skip")
}

#[must_use]
pub fn macro_task_id(thread_id: &str) -> String {
    let stem: String = thread_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(48)
        .collect();
    if stem.is_empty() {
        "lht-macro".to_string()
    } else {
        format!("lht-macro-{stem}")
    }
}

pub fn build_craft_review_prompt(lang: &str) -> String {
    if lang.starts_with("zh") {
        "LHT 宏观审查轮：对照当前 plan/checklist 与代码变更，枚举可检验的缺口（BLOCKER 级）。
只作缺口枚举，不要宣布任务完成。输出 `<!-- craft-verdict -->` JSON。
优先对照 IPC/架构清单与 `[verify:]` 可执行项。"
            .to_string()
    } else {
        "LHT macro review round: compare plan/checklist vs code changes. Enumerate testable gaps as BLOCKER items only — do not declare the task complete. Emit `<!-- craft-verdict -->` JSON. Prioritize IPC/architecture manifests and runnable `[verify:]` items.".to_string()
    }
}

pub fn build_remediation_nudge(
    lang: &str,
    added: u32,
    micro_gates_pending: bool,
    manifest_hints: &[String],
) -> String {
    let manifest_block = if manifest_hints.is_empty() {
        String::new()
    } else if lang.starts_with("zh") {
        format!(
            "\n\n**微观门仍未绿（须与 CRAFT 缺口一并处理）：**\n{}",
            manifest_hints
                .iter()
                .map(|h| format!("- {h}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        format!(
            "\n\n**Micro gates still red (fix alongside CRAFT gaps):**\n{}",
            manifest_hints
                .iter()
                .map(|h| format!("- {h}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    if lang.starts_with("zh") {
        let tail = if micro_gates_pending {
            " 补全后须重新通过微观验收门（gofmt/build/test 等）；微观门未绿不得最终收尾。"
        } else {
            ""
        };
        format!(
            "LHT 补全段：仅完成下列新增的 {added} 项 CRAFT 缺口（checklist 中 `CRAFT gap` 前缀）。\
             勿重复标记已完成项；每项完成后尽量附带 `[verify: …]`。{tail}{manifest_block}"
        )
    } else {
        let tail = if micro_gates_pending {
            " After remediation, micro completion gates (gofmt/build/test, etc.) must pass again before the task can finish."
        } else {
            ""
        };
        format!(
            "LHT remediation segment: complete only the {added} new CRAFT gap checklist item(s) (`CRAFT gap` prefix). \
             Do not re-mark completed items; add `[verify: …]` where possible.{tail}{manifest_block}"
        )
    }
}

pub fn build_confirm_prompt(lang: &str, micro_passed: bool) -> String {
    if lang.starts_with("zh") {
        if micro_passed {
            "LHT 微观完成门已通过。是否开始 CRAFT 审查轮？\
             回复「开始审查」或 `/lht-craft-go` 启动；回复「跳过审查」结束本 turn。"
                .to_string()
        } else {
            "LHT checklist 已勾完，但微观验收门未全绿。是否开始 CRAFT 审查轮以枚举缺口并进入补全段？\
             回复「开始审查」或 `/lht-craft-go` 启动；回复「跳过审查」结束本 turn。"
                .to_string()
        }
    } else if micro_passed {
        "LHT micro completion gates passed. Start a CRAFT review round? \
         Reply `start review round` or `/lht-craft-go` to begin; `skip craft` to end this turn."
            .to_string()
    } else {
        "LHT checklist is complete but micro gates are not green. Start CRAFT to enumerate gaps and enter remediation? \
         Reply `start review round` or `/lht-craft-go` to begin; `skip craft` to end this turn."
            .to_string()
    }
}

#[must_use]
pub fn should_auto_enter_craft(mode: AutoEnterCraft, trigger: MacroTrigger) -> bool {
    match mode {
        AutoEnterCraft::Off | AutoEnterCraft::UserConfirm => false,
        AutoEnterCraft::OnMicroPass => matches!(trigger, MacroTrigger::MicroPass),
        AutoEnterCraft::OnGraphComplete => matches!(
            trigger,
            MacroTrigger::GraphComplete | MacroTrigger::MicroPass | MacroTrigger::AuditUnmet { .. }
        ),
        AutoEnterCraft::OnManifestExhausted => {
            matches!(
                trigger,
                MacroTrigger::AuditUnmet {
                    reason: "manifest_rounds_exhausted"
                }
            )
        }
    }
}

fn should_skip_small_task(config: &MacroLoopConfig, checklist: &TodoListSnapshot) -> bool {
    !config.craft_on_small_tasks
        && (checklist.items.len() as u32) < config.min_checklist_items_for_craft
}

/// Evaluate macro loop after micro pass or checklist-complete audit unmet.
pub fn evaluate_macro_loop(input: MacroLoopInput<'_>, trigger: MacroTrigger) -> MacroLoopOutcome {
    if !input.config.enabled || !input.effective_mode.is_strict() {
        return MacroLoopOutcome::Inactive;
    }
    if should_skip_small_task(input.config, input.checklist) {
        return MacroLoopOutcome::Inactive;
    }

    let task_id = input
        .session
        .macro_task_id
        .clone()
        .unwrap_or_else(|| macro_task_id(input.thread_id));
    input.session.macro_task_id = Some(task_id.clone());

    if input.session.macro_awaiting_confirm {
        if let Some(user) = input.latest_user_text {
            if is_macro_skip_steer(user) {
                input.session.macro_awaiting_confirm = false;
                return MacroLoopOutcome::Inactive;
            }
            if is_macro_confirm_steer(user) {
                input.session.macro_awaiting_confirm = false;
                input.session.macro_phase = MacroPhase::Craft;
                if matches!(trigger, MacroTrigger::AuditUnmet { .. }) {
                    input.session.macro_after_audit_unmet = true;
                }
                return MacroLoopOutcome::CraftSpawnRequested { task_id };
            }
        }
        return MacroLoopOutcome::AwaitUserConfirm;
    }

    match input.session.macro_phase {
        MacroPhase::Implement => {
            if input.session.macro_cycles_used >= input.config.max_macro_cycles {
                let remaining = read_remaining_blocker_summaries(input.workspace, &task_id);
                return MacroLoopOutcome::Unmet {
                    remaining_blockers: remaining,
                    macro_cycles_used: input.session.macro_cycles_used,
                };
            }
            input.session.macro_after_audit_unmet = !matches!(trigger, MacroTrigger::MicroPass);
            match input.config.auto_enter_craft {
                AutoEnterCraft::Off => MacroLoopOutcome::Inactive,
                AutoEnterCraft::UserConfirm => {
                    input.session.macro_awaiting_confirm = true;
                    MacroLoopOutcome::AwaitUserConfirm
                }
                mode if should_auto_enter_craft(mode, trigger) => {
                    input.session.macro_phase = MacroPhase::Craft;
                    MacroLoopOutcome::CraftSpawnRequested { task_id }
                }
                _ => MacroLoopOutcome::Inactive,
            }
        }
        MacroPhase::Craft | MacroPhase::Remediation => {
            // Craft/remediation phases are advanced by sub-agent completion or remediation nudge.
            MacroLoopOutcome::Inactive
        }
    }
}

/// Evaluate macro loop after micro gates return `graph_complete`.
#[must_use]
pub fn evaluate_after_micro_pass(input: MacroLoopInput<'_>) -> MacroLoopOutcome {
    evaluate_macro_loop(input, MacroTrigger::MicroPass)
}

/// When the checklist/plan graph is complete, offer CRAFT after verify-hygiene
/// nudges but before machine micro-gate nudges (`on_graph_complete`) or user
/// confirm (`user_confirm`).
#[must_use]
pub fn maybe_evaluate_macro_at_graph_complete(input: MacroLoopInput<'_>) -> Option<LhtGateOutcome> {
    if !input.config.enabled || !input.effective_mode.is_strict() {
        return None;
    }
    if should_skip_small_task(input.config, input.checklist) {
        return None;
    }
    if !matches!(
        input.config.auto_enter_craft,
        AutoEnterCraft::OnGraphComplete | AutoEnterCraft::UserConfirm
    ) {
        return None;
    }
    if !matches!(input.session.macro_phase, MacroPhase::Implement) {
        return None;
    }
    let outcome = evaluate_macro_loop(input, MacroTrigger::GraphComplete);
    let gate = macro_outcome_to_gate(outcome);
    if matches!(gate, LhtGateOutcome::Skip(_)) {
        None
    } else {
        Some(gate)
    }
}

/// CRAFT finished but remediation was never injected (turn ended while review ran).
pub async fn try_resume_pending_macro_remediation(
    workspace: &Path,
    session: &mut LongHorizonSessionState,
    config: &MacroLoopConfig,
    todos: &crate::tools::todo::SharedTodoList,
    lang: &str,
) -> Option<LhtGateOutcome> {
    if !config.enabled || session.macro_craft_agent_id.is_some() {
        return None;
    }
    let task_id = session.macro_task_id.clone()?;
    if session.macro_phase != MacroPhase::Craft {
        return None;
    }
    let blockers = blackboard::read_reviewer_blocker_items(workspace, &task_id);
    if blockers.is_empty() {
        return None;
    }
    on_craft_review_complete(workspace, &task_id, session, config, todos, lang).await
}

/// After a harness-spawned CRAFT review sub-agent completes, convert blockers → checklist.
pub async fn on_craft_review_complete(
    workspace: &Path,
    task_id: &str,
    session: &mut LongHorizonSessionState,
    config: &MacroLoopConfig,
    todos: &crate::tools::todo::SharedTodoList,
    lang: &str,
) -> Option<LhtGateOutcome> {
    session.macro_craft_agent_id.as_ref()?;
    session.macro_craft_agent_id = None;
    session.craft_rounds_this_cycle = session.craft_rounds_this_cycle.saturating_add(1);

    let blockers = blackboard::read_reviewer_blocker_items(workspace, task_id);
    if blockers.is_empty() {
        // No gaps — macro cycle complete for this round.
        session.macro_phase = MacroPhase::Implement;
        session.macro_cycles_used = session.macro_cycles_used.saturating_add(1);
        return None;
    }

    let added = apply_blockers_to_checklist(todos, &blockers).await;
    if added == 0 {
        if session.craft_rounds_this_cycle >= config.max_craft_rounds_per_cycle {
            let remaining: Vec<String> = blockers
                .iter()
                .map(|b| format!("{} — {}", b.file, b.description))
                .collect();
            return Some(LhtGateOutcome::MacroUnmet {
                remaining_blockers: remaining,
                macro_cycles_used: session.macro_cycles_used,
            });
        }
        session.macro_phase = MacroPhase::Craft;
        return Some(LhtGateOutcome::MacroCraftSpawn {
            task_id: task_id.to_string(),
        });
    }

    session.macro_phase = MacroPhase::Remediation;
    let micro_pending = session.macro_after_audit_unmet;
    let hints = session.macro_pending_manifest_hints.clone();
    let text = build_remediation_nudge(lang, added, micro_pending, &hints);
    Some(LhtGateOutcome::MacroRemediation {
        blockers_added: added,
        message: Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text,
                cache_control: None,
            }],
        },
    })
}

fn read_remaining_blocker_summaries(workspace: &Path, task_id: &str) -> Vec<String> {
    blackboard::read_reviewer_blocker_items(workspace, task_id)
        .iter()
        .map(|b| format!("{} — {}", b.file, b.description))
        .collect()
}

/// Map macro outcome to gate outcome for the LHT continue path.
#[must_use]
pub fn macro_outcome_to_gate(outcome: MacroLoopOutcome) -> LhtGateOutcome {
    match outcome {
        MacroLoopOutcome::Inactive => LhtGateOutcome::Skip("graph_complete"),
        MacroLoopOutcome::AwaitUserConfirm => LhtGateOutcome::Skip("macro_await_confirm"),
        MacroLoopOutcome::CraftSpawnRequested { task_id } => {
            LhtGateOutcome::MacroCraftSpawn { task_id }
        }
        MacroLoopOutcome::Unmet {
            remaining_blockers,
            macro_cycles_used,
        } => LhtGateOutcome::MacroUnmet {
            remaining_blockers,
            macro_cycles_used,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_core::subagent::VerdictItem;

    fn sample_blocker() -> VerdictItem {
        VerdictItem {
            severity: "BLOCKER".into(),
            file: "src/lib.rs".into(),
            line: Some(10),
            description: "IPC handler missing".into(),
            rule: Some("IPC".into()),
            suggestion: None,
        }
    }

    #[test]
    fn blockers_to_checklist_is_idempotent() {
        let b = sample_blocker();
        let content = format_blocker_checklist_content(&b);
        let existing = vec![TodoItem {
            id: 1,
            content: content.clone(),
            status: TodoStatus::Pending,
        }];
        assert!(blockers_to_checklist_contents(&[b.clone()], &existing).is_empty());
        assert_eq!(blockers_to_checklist_contents(&[b], &[]).len(), 1);
    }

    #[test]
    fn confirm_and_skip_steer_parsing() {
        assert!(is_macro_confirm_steer("开始审查"));
        assert!(is_macro_confirm_steer("/lht-craft-go"));
        assert!(is_macro_skip_steer("跳过审查"));
    }

    #[test]
    fn macro_task_id_sanitizes_thread_id() {
        assert_eq!(macro_task_id("thread/abc"), "lht-macro-threadabc");
    }

    #[test]
    fn remediation_nudge_includes_manifest_hints_when_micro_pending() {
        let text = build_remediation_nudge(
            "zh",
            3,
            true,
            &["[toolchain_go_test] go test -cover ./... — cmd/todo 0%".into()],
        );
        assert!(text.contains("CRAFT 缺口"));
        assert!(text.contains("微观门仍未绿"));
        assert!(text.contains("cmd/todo"));
    }

    #[test]
    fn auto_enter_craft_trigger_matrix() {
        use zagens_core::long_horizon::AutoEnterCraft;

        assert!(should_auto_enter_craft(
            AutoEnterCraft::OnMicroPass,
            MacroTrigger::MicroPass
        ));
        assert!(!should_auto_enter_craft(
            AutoEnterCraft::OnMicroPass,
            MacroTrigger::AuditUnmet {
                reason: "manifest_rounds_exhausted"
            }
        ));
        assert!(should_auto_enter_craft(
            AutoEnterCraft::OnGraphComplete,
            MacroTrigger::GraphComplete
        ));
        assert!(should_auto_enter_craft(
            AutoEnterCraft::OnGraphComplete,
            MacroTrigger::AuditUnmet {
                reason: "manifest_rounds_exhausted"
            }
        ));
        assert!(should_auto_enter_craft(
            AutoEnterCraft::OnGraphComplete,
            MacroTrigger::MicroPass
        ));
        assert!(!should_auto_enter_craft(
            AutoEnterCraft::OnMicroPass,
            MacroTrigger::GraphComplete
        ));
        assert!(should_auto_enter_craft(
            AutoEnterCraft::OnManifestExhausted,
            MacroTrigger::AuditUnmet {
                reason: "manifest_rounds_exhausted"
            }
        ));
        assert!(!should_auto_enter_craft(
            AutoEnterCraft::OnManifestExhausted,
            MacroTrigger::AuditUnmet {
                reason: "gate_infra_error"
            }
        ));
    }
}
