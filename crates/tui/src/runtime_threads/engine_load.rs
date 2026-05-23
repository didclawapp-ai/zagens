//! Engine spawn + session sync for durable threads (R-003 A4.6).

use anyhow::{Result, anyhow};

use crate::config::MAX_SUBAGENTS;
use crate::core::engine::{EngineConfig, EngineHandle, spawn_engine};
use crate::core::ops::Op;
use crate::models::{Message, SystemPrompt};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::todo::new_shared_todo_list;

use super::active::{enforce_lru_capacity, touch_lru, ActiveThreadState};
use super::persist::reconstruct_messages_for_store;
use super::types::ThreadRecord;
use super::RuntimeThreadManager;

impl RuntimeThreadManager {
    pub(crate) async fn ensure_engine_loaded(&self, thread: &ThreadRecord) -> Result<EngineHandle> {
        {
            let mut active = self.active.lock().await;
            if let Some(engine) = active
                .engines
                .get(thread.id.as_str())
                .map(|state| state.engine.clone())
            {
                touch_lru(&mut active.lru, &thread.id);
                return Ok(engine);
            }
        }

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
                    if t.scratchpad_run_id.as_deref() != Some(run_id.as_str()) {
                        t.scratchpad_run_id = Some(run_id);
                        t.updated_at = chrono::Utc::now();
                        let _ = store.save_thread(&t);
                    }
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
                self.config.instructions_paths(),
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
            runtime_services: crate::tools::spec::RuntimeToolServices {
                task_manager: self.task_manager.lock().ok().and_then(|slot| slot.clone()),
                automations: self.automations.lock().ok().and_then(|slot| slot.clone()),
                task_data_dir: Some(self.manager_cfg.task_data_dir.clone()),
                active_task_id: thread.task_id.clone(),
                active_thread_id: Some(thread.id.clone()),
                shell_manager: None,
                hook_executor: None,
                scratchpad_run_id: scratchpad_run_id_slot,
                persist_scratchpad_run_id: Some(persist_scratchpad),
                scratchpad_config: Some(self.config.scratchpad_config()),
            },
            subagent_model_overrides: self.config.subagent_model_overrides(),
            memory_enabled: self.config.memory_enabled(),
            memory_path: self.config.memory_path(),
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
            llm_client_override: None,
        };

        let engine = spawn_engine(engine_cfg, &self.config);

        let store = self.store.clone();
        let thread_id = thread.id.clone();
        let session_messages = tokio::task::spawn_blocking(move || -> Result<Vec<Message>> {
            let turns = store.list_turns_for_thread(&thread_id)?;
            reconstruct_messages_for_store(&store, &turns)
        })
        .await
        .map_err(|e| anyhow!("ensure_engine_loaded panicked: {e}"))??;

        let sys_prompt = thread
            .system_prompt
            .as_ref()
            .map(|s| SystemPrompt::Text(s.clone()));
        if !session_messages.is_empty() || sys_prompt.is_some() {
            engine
                .send(Op::SyncSession {
                    messages: session_messages,
                    system_prompt: sys_prompt,
                    model: thread.model.clone(),
                    workspace: thread.workspace.clone(),
                })
                .await
                .map_err(|e| anyhow!("Failed to sync thread session: {e}"))?;
        }

        let mut active = self.active.lock().await;
        let evicted = enforce_lru_capacity(&mut active, self.manager_cfg.max_active_threads);
        active.engines.insert(
            thread.id.clone(),
            ActiveThreadState {
                engine: engine.clone(),
                active_turn: None,
            },
        );
        touch_lru(&mut active.lru, &thread.id);
        drop(active);
        for handle in evicted {
            let _ = handle.send(Op::Shutdown).await;
        }
        Ok(engine)
    }
}
