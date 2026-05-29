//! Sidecar engine spawn for durable threads (Config, tools, task/automation slots).

use anyhow::Result;

use crate::config::MAX_SUBAGENTS;
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::todo::new_shared_todo_list;

use super::types::ThreadRecord;
use super::RuntimeThreadManager;

impl RuntimeThreadManager {
    pub(crate) async fn spawn_engine_for_thread_impl(
        &self,
        thread: &ThreadRecord,
    ) -> Result<EngineHandle> {
        let compaction = self.config.compaction_runtime_config(&thread.model);
        let network_policy = self.config.network.clone().map(|toml_cfg| {
            crate::network_policy::NetworkPolicyDecider::with_default_audit(toml_cfg.into_runtime())
        });
        let lsp_config = self
            .config
            .lsp
            .clone()
            .map(crate::config::LspConfigToml::into_runtime);
        let scratchpad_run_id_slot = std::sync::Arc::new(std::sync::Mutex::new(
            thread.scratchpad_run_id.clone(),
        ));
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
            trust_mode: thread.trust_mode,
            notes_path: self.config.notes_path(),
            mcp_config_path: self.config.mcp_config_path(),
            skills_dir: self.config.skills_dir(),
            instructions: crate::prompts::merge_instruction_paths_with_pick_rules(
                &thread.workspace,
                self.config.instructions_paths(&thread.workspace),
            ),
            max_steps: 100,
            max_subagents: self.config.max_subagents().clamp(1, MAX_SUBAGENTS),
            subagent_step_timeout: self.config.subagent_step_timeout(),
            features: self.config.features(),
            compaction,
            cycle: crate::cycle_manager::CycleConfig::default(),
            capacity: crate::core::capacity::capacity_config_from_app(&self.config),
            todos: new_shared_todo_list(),
            plan_state: new_shared_plan_state(),
            max_spawn_depth: crate::tools::subagent::DEFAULT_MAX_SPAWN_DEPTH,
            network_policy,
            snapshots_enabled: self.config.snapshots_config().enabled,
            lsp_config,
            runtime_services: self.background.build_runtime_tool_services(
                thread,
                self.manager_cfg.task_data_dir.clone(),
                scratchpad_run_id_slot,
                persist_scratchpad,
                self.config.scratchpad_config(),
            ),
            subagent_model_overrides: self.config.subagent_model_overrides(),
            memory_enabled: self.config.memory_enabled(),
            memory_path: self.config.memory_path(),
            topic_memory: crate::topic_memory::settings_from_config(&self.config),
            strict_tool_mode: self.config.strict_tool_mode.unwrap_or(false),
            goal_objective: None,
            locale_tag: crate::localization::resolve_locale(
                &crate::settings::Settings::load().unwrap_or_default().locale,
            )
            .tag()
            .to_string(),
            task_type: crate::task_type::TaskType::parse_str(&thread.task_type)
                .unwrap_or(crate::task_type::TaskType::Code),
            workshop: self.config.workshop.clone(),
            scratchpad: self.config.scratchpad_config(),
            long_horizon: self.config.long_horizon_config(),
            llm_client_override: None,
        };

        Ok(spawn_engine(engine_cfg, &self.config))
    }
}
