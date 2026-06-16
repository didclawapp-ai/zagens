//! v3 system prompt refresh — full effect chain via [`EffectInterpreter`].

use zagens_core::engine::turn_loop::system_prompt_refresh_policy::plan_system_prompt_refresh;
use zagens_core::engine::turn_machine::Effect;
use zagens_core::turn::TurnLoopMode;

use super::effect_interpreter::EffectInterpreter;
use super::turn_loop::host_impl::turn_loop_to_app_mode;
use super::*;

impl Engine {
    fn log_v3_system_prompt_effect_slot(turn_id: &str, step: u32, slot: usize, effect: &Effect) {
        tracing::info!(
            target: "kernel_v3",
            turn_id = %turn_id,
            step,
            slot,
            effect = effect.kind_str(),
            "v3 system prompt refresh effect (EffectInterpreter)"
        );
    }

    /// Run planned `QueryMemory` chain (v3 only).
    pub(in crate::core::engine) async fn run_v3_system_prompt_refresh_queries(
        &mut self,
        turn_id: &str,
        step: u32,
    ) {
        let plan = plan_system_prompt_refresh();
        for (slot, effect) in plan
            .effects
            .iter()
            .filter(|effect| matches!(effect, Effect::QueryMemory { .. }))
            .enumerate()
        {
            Self::log_v3_system_prompt_effect_slot(turn_id, step, slot, effect);
            let mut interpreter = EffectInterpreter::new(self);
            let _ = interpreter.interpret(effect.clone()).await;
        }
    }

    /// Assembly tail for [`Effect::RefreshSystemPrompt`] (mode supplied by turn loop).
    pub(in crate::core::engine) fn run_refresh_system_prompt_effect(&mut self, mode: TurnLoopMode) {
        Engine::refresh_system_prompt(self, turn_loop_to_app_mode(mode));
    }

    /// Full v3 refresh: interpret planned `QueryMemory` chain + `RefreshSystemPrompt`.
    pub(in crate::core::engine) async fn run_v3_system_prompt_refresh(
        &mut self,
        turn_id: &str,
        step: u32,
        mode: TurnLoopMode,
    ) {
        let plan = plan_system_prompt_refresh();
        for (slot, effect) in plan.effects.iter().enumerate() {
            Self::log_v3_system_prompt_effect_slot(turn_id, step, slot, effect);
            match effect {
                Effect::RefreshSystemPrompt => {
                    Self::run_refresh_system_prompt_effect(self, mode);
                }
                other => {
                    let mut interpreter = EffectInterpreter::new(self);
                    let _ = interpreter.interpret(other.clone()).await;
                }
            }
        }
    }
}
