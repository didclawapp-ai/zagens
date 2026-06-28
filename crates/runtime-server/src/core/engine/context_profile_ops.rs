//! Context profile refresh when the active model changes (P1).

use zagens_core::context_profile::{cycle_config_from_thresholds, is_large_context_profile};
use zagens_core::engine::hosts::SeamHost;

use super::Engine;

impl Engine {
    /// Recompute cycle thresholds (and related gates) for the current model.
    pub(in crate::core::engine) fn refresh_context_profile_bindings(&mut self) {
        let model = self.session.model.clone();
        let (thresholds, seam_enabled, verbatim_window_turns) = {
            let ctx = &self.config_ext().context_config;
            (
                ctx.resolved_thresholds_for(&model),
                ctx.seam_enabled_for_model(&model),
                ctx.verbatim_window_turns
                    .unwrap_or(crate::seam_manager::VERBATIM_WINDOW_TURNS),
            )
        };
        self.config.cycle = cycle_config_from_thresholds(&model, &thresholds);
        if is_large_context_profile(&model) && self.session.compaction_summary_prompt.is_some() {
            self.session.compaction_summary_prompt = None;
        }
        // P1: the seam host is built once with the initial model's profile;
        // hot-reload its enablement + thresholds so a mid-session model switch
        // (e.g. Medium → Large) actually engages/disengages seams correctly.
        if let Some(seam) = self.seam.as_mut() {
            seam.reconfigure_for_model(
                seam_enabled,
                verbatim_window_turns,
                thresholds.l1,
                thresholds.l2,
                thresholds.l3,
                thresholds.cycle,
            );
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
