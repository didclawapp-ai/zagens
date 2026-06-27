//! Context profile refresh when the active model changes (P1).

use zagens_core::context_profile::{cycle_config_from_thresholds, is_large_context_profile};

use super::Engine;

impl Engine {
    /// Recompute cycle thresholds (and related gates) for the current model.
    pub(in crate::core::engine) fn refresh_context_profile_bindings(&mut self) {
        let model = self.session.model.clone();
        let thresholds = self
            .config_ext()
            .context_config
            .resolved_thresholds_for(&model);
        self.config.cycle = cycle_config_from_thresholds(&model, &thresholds);
        if is_large_context_profile(&model) && self.session.compaction_summary_prompt.is_some() {
            self.session.compaction_summary_prompt = None;
        }
    }

    pub(in crate::core::engine) fn scaled_context_thresholds(
        &self,
    ) -> zagens_core::context_profile::ScaledContextThresholds {
        self.config_ext()
            .context_config
            .resolved_thresholds_for(&self.session.model)
    }
}
