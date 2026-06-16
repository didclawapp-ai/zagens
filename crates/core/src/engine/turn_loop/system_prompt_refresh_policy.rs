//! System prompt refresh → memory-plane query + assembly effect plan (Phase 3b batch 5b cont.).
//!
//! v3 live path runs the full chain through runtime [`EffectInterpreter`] +
//! [`RefreshSystemPrompt`](crate::engine::turn_machine::Effect::RefreshSystemPrompt).

use crate::engine::turn_machine::Effect;

use super::memory_plane_episodic_policy::QUERY_TOPIC_EPISODIC;
use super::memory_plane_projection_policy::MemoryPlaneLayer;
use super::memory_plane_query_policy::QUERY_USER_MEMORY;

/// Target v3 refresh chain: `QueryMemory` reads + `RefreshSystemPrompt` assembly tail.
#[derive(Debug, Clone)]
pub struct SystemPromptRefreshPlan {
    /// `false` when assembly is the terminal `RefreshSystemPrompt` effect (v3 default).
    pub host_io_required: bool,
    pub effects: Vec<Effect>,
}

/// Canonical v3 query chain for refresh inputs (user memory + topic episodic).
#[must_use]
pub fn plan_system_prompt_refresh_query_effects() -> Vec<Effect> {
    vec![
        Effect::QueryMemory {
            layer: MemoryPlaneLayer::Episodic,
            query_key: QUERY_USER_MEMORY.into(),
        },
        Effect::QueryMemory {
            layer: MemoryPlaneLayer::Episodic,
            query_key: QUERY_TOPIC_EPISODIC.into(),
        },
    ]
}

/// Full v3 refresh effect chain (`QueryMemory` ×2 + `RefreshSystemPrompt`).
#[must_use]
pub fn plan_system_prompt_refresh_effects() -> Vec<Effect> {
    let mut effects = plan_system_prompt_refresh_query_effects();
    effects.push(Effect::RefreshSystemPrompt);
    effects
}

#[must_use]
pub fn plan_system_prompt_refresh() -> SystemPromptRefreshPlan {
    SystemPromptRefreshPlan {
        host_io_required: false,
        effects: plan_system_prompt_refresh_effects(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_plan_is_effect_driven_with_assembly_tail() {
        let plan = plan_system_prompt_refresh();
        assert!(!plan.host_io_required);
        assert_eq!(plan.effects.len(), 3);
        assert!(matches!(plan.effects[0], Effect::QueryMemory { .. }));
        assert!(matches!(
            plan.effects.last(),
            Some(Effect::RefreshSystemPrompt)
        ));
    }
}
