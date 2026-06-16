//! Replay anchor-only gate — skip compaction/cycle IO when re-deriving effects from log.

use zagens_core::engine::KernelTurnHost;
use zagens_core::engine::turn_machine::{
    Effect, KernelResumeHints, replay_turn_effects, verify_resume_anchor_effect_alignment,
};

use super::effect_interpreter::EffectInterpreter;
use super::kernel_resume_replay_anchor_shadow::{
    record_resume_replay_anchor_alignment, record_resume_replay_anchor_run,
};
use super::*;

fn is_anchor_effect(effect: &Effect) -> bool {
    matches!(
        effect,
        Effect::RunCompaction | Effect::InjectSteer { .. } | Effect::NotifyLsp { .. }
    )
}

fn anchor_effects_in_chain(effects: &[Effect]) -> Vec<Effect> {
    effects
        .iter()
        .filter(|effect| is_anchor_effect(effect))
        .cloned()
        .collect()
}

impl Engine {
    #[must_use]
    pub(in crate::core::engine) fn effect_replay_anchor_only(&self) -> bool {
        self.runtime_ext().kernel_effect_replay_anchor_only
    }

    /// Run a closure with replay anchor-only enabled (no compaction/cycle IO).
    pub(in crate::core::engine) fn with_effect_replay_anchor_only<R>(
        &mut self,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let ext = self.runtime_ext_mut();
        let prev = ext.kernel_effect_replay_anchor_only;
        ext.kernel_effect_replay_anchor_only = true;
        let out = f(self);
        self.runtime_ext_mut().kernel_effect_replay_anchor_only = prev;
        out
    }

    /// Re-interpret anchor-class effects from a replay chain without session IO.
    pub(in crate::core::engine) async fn interpret_replay_anchor_effects(
        &mut self,
        effects: &[Effect],
    ) {
        let anchor_effects = anchor_effects_in_chain(effects);
        if anchor_effects.is_empty() {
            return;
        }
        let ext = self.runtime_ext_mut();
        let prev = ext.kernel_effect_replay_anchor_only;
        ext.kernel_effect_replay_anchor_only = true;
        let mut interpreter = EffectInterpreter::new(self);
        for effect in anchor_effects {
            let _ = interpreter.interpret(effect).await;
        }
        self.runtime_ext_mut().kernel_effect_replay_anchor_only = prev;
    }

    /// Restore turn frame from log hints and dry-run anchor effects for all thread turns.
    pub(in crate::core::engine) async fn apply_kernel_resume_with_replay(
        &mut self,
        hints: &KernelResumeHints,
    ) {
        KernelTurnHost::apply_kernel_resume_hints(self, hints);
        self.maybe_repair_session_from_kernel_log(hints).await;
        let mode = self.runtime_ext().kernel_machine_mode;
        if !mode.uses_replay_verification() && !mode.uses_v3_turn_loop() {
            return;
        }
        let turn_ids: Vec<String> = if !hints.thread_turn_ids_with_events.is_empty() {
            hints.thread_turn_ids_with_events.clone()
        } else if let Some(turn_id) = hints.latest_turn_id.clone() {
            vec![turn_id]
        } else {
            return;
        };
        let writer = self.runtime_ext().kernel_event_writer.clone();
        let Some(writer) = writer else {
            return;
        };
        let mut turns_interpreted = 0u64;
        let mut turns_skipped = 0u64;
        let mut anchors_interpreted = 0u64;
        for turn_id in turn_ids {
            let events: Vec<zagens_core::engine::kernel_event::KernelEvent> =
                match writer.load_turn_events_sync(&turn_id) {
                    Ok(events) => events,
                    Err(err) => {
                        tracing::warn!(
                            target: "kernel_resume",
                            turn_id = %turn_id,
                            error = %err,
                            "replay anchor-only skipped: could not load turn events"
                        );
                        turns_skipped += 1;
                        continue;
                    }
                };
            if events.is_empty() {
                turns_skipped += 1;
                continue;
            }
            let effects = replay_turn_effects(&events);
            let anchor_count = effects
                .iter()
                .filter(|effect| is_anchor_effect(effect))
                .count();
            if anchor_count == 0 {
                continue;
            }
            self.interpret_replay_anchor_effects(&effects).await;
            turns_interpreted += 1;
            anchors_interpreted += anchor_count as u64;
            tracing::info!(
                target: "kernel_resume",
                turn_id = %turn_id,
                anchor_count,
                "replay anchor-only effects interpreted on resume"
            );
        }
        if turns_interpreted > 0 || turns_skipped > 0 {
            record_resume_replay_anchor_run(turns_interpreted, turns_skipped, anchors_interpreted);
        }
        let expected = u64::from(hints.expected_anchor_effect_count);
        record_resume_replay_anchor_alignment(anchors_interpreted, expected);
        if let Some(summary) = verify_resume_anchor_effect_alignment(
            hints.expected_anchor_effect_count,
            anchors_interpreted,
        ) {
            tracing::warn!(
                target: "kernel_resume",
                %summary,
                anchors_interpreted,
                expected,
                turns_skipped,
                "resume anchor effect alignment diff"
            );
        }
    }
}
