//! Sidecar engine spawn for durable threads (Config, tools, task/automation slots).

use anyhow::Result;

use crate::config::{
    EffectiveLhtComposerMode, MAX_SUBAGENTS, resolve_effective_config, resolve_lht_composer_mode,
};
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::todo::new_shared_todo_list;

use super::RuntimeThreadManager;
use super::types::ThreadRecord;

impl RuntimeThreadManager {
    pub(crate) async fn spawn_engine_for_thread_impl(
        &self,
        thread: &ThreadRecord,
    ) -> Result<EngineHandle> {
        let effective = resolve_effective_config(&self.config, thread.config_overlay.as_ref());
        let compaction = effective.compaction_runtime_config(&thread.model);
        let network_policy = self.config.network.clone().map(|toml_cfg| {
            crate::network_policy::NetworkPolicyDecider::with_default_audit(toml_cfg.into_runtime())
        });
        let lsp_config = effective
            .lsp
            .clone()
            .map(crate::config::LspConfigToml::into_runtime);
        let scratchpad_run_id_slot =
            std::sync::Arc::new(std::sync::Mutex::new(thread.scratchpad_run_id.clone()));
        let ui_settings = crate::settings::Settings::load().unwrap_or_default();
        let settings_composer =
            zagens_config::read_lht_composer_mode_setting()
                .ok()
                .map(|m| match m {
                    zagens_config::LhtComposerMode::Strict => "strict",
                    zagens_config::LhtComposerMode::Off => "off",
                    zagens_config::LhtComposerMode::Auto => "auto",
                });
        let composer_mode =
            resolve_lht_composer_mode(thread.config_overlay.as_ref(), settings_composer);
        let mut long_horizon = effective.long_horizon_config();
        match composer_mode {
            EffectiveLhtComposerMode::Strict => {
                long_horizon.enabled = true;
                long_horizon.mode = zagens_core::long_horizon::LhtMode::Strict;
            }
            EffectiveLhtComposerMode::Off => {
                long_horizon.enabled = false;
            }
            EffectiveLhtComposerMode::Auto => {}
        }
        let store = self.store.clone();
        let thread_id_persist = thread.id.clone();
        let persist_scratchpad: std::sync::Arc<dyn Fn(String) + Send + Sync> =
            std::sync::Arc::new(move |run_id: String| {
                if let Ok(mut t) = store.load_thread(&thread_id_persist) {
                    t.record_scratchpad_run(&run_id);
                    t.updated_at = chrono::Utc::now();
                    let _ = store.save_thread(&t);
                }
            });
        let engine_cfg = EngineConfig {
            model: thread.model.clone(),
            workspace: thread.workspace.clone(),
            allow_shell: thread.allow_shell,
            sandbox_mode: self.config.sandbox_mode.clone(),
            trust_mode: thread.trust_mode,
            notes_path: self.config.notes_path(),
            mcp_config_path: self.config.mcp_config_path(),
            skills_dir: self.config.skills_dir(),
            instructions: crate::prompts::merge_instruction_paths_with_pick_rules(
                &thread.workspace,
                self.config.instructions_paths(&thread.workspace),
            ),
            max_steps: 100,
            max_subagents: effective.max_subagents().clamp(1, MAX_SUBAGENTS),
            subagent_step_timeout: effective.subagent_step_timeout(),
            features: effective.features(),
            compaction,
            cycle: effective.cycle_runtime_config(&thread.model),
            capacity: crate::core::capacity::capacity_config_from_app(&effective),
            todos: new_shared_todo_list(),
            plan_state: new_shared_plan_state(),
            max_spawn_depth: crate::tools::subagent::DEFAULT_MAX_SPAWN_DEPTH,
            network_policy,
            snapshots_enabled: effective.snapshots_config().enabled,
            snapshots_max_workspace_gb: effective.snapshots_config().max_workspace_gb,
            lsp_config,
            runtime_services: self.background.build_runtime_tool_services(
                thread,
                self.manager_cfg.task_data_dir.clone(),
                scratchpad_run_id_slot,
                persist_scratchpad,
                self.config.scratchpad_config(),
            ),
            subagent_model_overrides: effective.subagent_model_overrides(),
            memory_enabled: effective.memory_enabled(),
            memory_path: self.config.memory_path(),
            topic_memory: crate::topic_memory::settings_from_config(&effective),
            strict_tool_mode: self.config.strict_tool_mode.unwrap_or(false),
            goal_objective: None,
            locale_tag: crate::localization::resolve_locale(&ui_settings.locale)
                .tag()
                .to_string(),
            task_type: crate::task_type::TaskType::parse_str(&thread.task_type)
                .unwrap_or(crate::task_type::TaskType::Code),
            workshop: self.config.workshop.clone(),
            scratchpad: self.config.scratchpad_config(),
            long_horizon,
            llm_client_override: None,
            search_provider: {
                let search = self.config.search_config();
                search.provider.unwrap_or_default()
            },
            search_api_key: self.config.search_config().api_key,
            session_manager: self.session_manager.clone(),
        };

        Ok(spawn_engine(engine_cfg, &self.config))
    }
}
