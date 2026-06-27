//! Engine session/config mutations from the op loop (P2 — thin shell over core helpers).

use std::path::PathBuf;

use crate::compaction::CompactionConfig;
use crate::context_snapshot::{ThreadContextSnapshot, build_thread_context_snapshot};
use crate::core::events::Event;
use crate::models::{Message, SystemPrompt};
use zagens_core::engine::ContextUsageBreakdown;

use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn set_runtime_model(&mut self, model: String) {
        zagens_core::session::apply_model_selection(
            &mut self.0.session,
            &mut self.0.config.model,
            model,
        );
    }

    pub(in crate::core::engine) async fn apply_set_model_op(&mut self, model: String) {
        self.set_runtime_model(model);
        self.refresh_context_profile_bindings();
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "Model set to: {}",
                self.session.model
            )))
            .await;
    }

    pub(in crate::core::engine) fn set_compaction_config(
        &mut self,
        config: CompactionConfig,
    ) -> bool {
        let enabled = config.enabled;
        self.config.compaction = config;
        enabled
    }

    pub(in crate::core::engine) async fn apply_set_compaction_op(
        &mut self,
        config: CompactionConfig,
    ) {
        let enabled = self.set_compaction_config(config);
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "Auto-compaction {}",
                if enabled { "enabled" } else { "disabled" }
            )))
            .await;
    }

    pub(in crate::core::engine) async fn sync_session_from_op(
        &mut self,
        messages: Vec<Message>,
        system_prompt: Option<SystemPrompt>,
        model: String,
        workspace: PathBuf,
    ) {
        zagens_core::session::apply_sync_session_payload(
            &mut self.0.session,
            &mut self.0.config.workspace,
            &mut self.0.config.model,
            messages,
            system_prompt,
            model,
            workspace,
        );
        self.rehydrate_latest_canonical_state();
        self.emit_pending_startup_warnings().await;
        self.emit_session_updated().await;
        let _ = self
            .tx_event
            .send(Event::status("Session context synced".to_string()))
            .await;
    }

    #[must_use]
    pub(in crate::core::engine) fn engine_context_snapshot(&self) -> ThreadContextSnapshot {
        build_thread_context_snapshot(
            &self.session.model,
            &self.session.messages,
            self.session.system_prompt.as_ref(),
            &self.config.compaction,
            Some(&self.session.workspace),
            self.session.last_api_input_tokens,
            None,
            "engine",
        )
    }

    #[must_use]
    pub(in crate::core::engine) fn engine_context_breakdown(&self) -> ContextUsageBreakdown {
        let thresholds = self
            .config_ext()
            .context_config
            .resolved_thresholds_for(&self.session.model);
        let seam_enabled = self
            .config_ext()
            .context_config
            .seam_enabled_for_model(&self.session.model);
        let report = self.runtime_ext().last_context_assembly_report.as_ref();
        crate::context_snapshot::build_thread_context_breakdown(
            &self.session.model,
            &self.session.messages,
            self.session.system_prompt.as_ref(),
            &self.config.compaction,
            Some(&self.session.workspace),
            report,
            thresholds,
            seam_enabled,
        )
    }

    pub(in crate::core::engine) async fn push_live_context_panel_events(&self) {
        if let Ok(json) = serde_json::to_string(&self.engine_context_snapshot()) {
            let _ = self
                .tx_event
                .send(Event::status(format!(
                    "long_horizon.context_snapshot:{json}"
                )))
                .await;
        }
        let breakdown = self.engine_context_breakdown();
        if let Ok(json) = serde_json::to_string(&breakdown) {
            let _ = self
                .tx_event
                .send(Event::status(format!("long_horizon.context_usage:{json}")))
                .await;
        }
    }
}
