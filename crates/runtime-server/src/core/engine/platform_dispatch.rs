//! Tui [`EnginePlatformExt`] — op-loop dispatch (M8).

use async_trait::async_trait;
use deepseek_core::engine::op::Op;
use deepseek_core::engine::platform_ext::EnginePlatformExt;
use deepseek_core::engine::Engine as CoreEngine;

use crate::context_snapshot::ThreadContextSnapshot;
use crate::core::events::Event;
use crate::agent_surface::AppMode;
use tokio::sync::oneshot;

use super::Engine;
use super::runtime_ext::EngineRuntimeExt;
use super::turn_loop::host_impl::turn_loop_to_app_mode;

#[async_trait]
impl EnginePlatformExt<crate::sandbox::SandboxPolicy, crate::tools::user_input::UserInputResponse>
    for EngineRuntimeExt
{
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    async fn dispatch_op(
        &mut self,
        core: &mut CoreEngine<
            crate::sandbox::SandboxPolicy,
            crate::tools::user_input::UserInputResponse,
        >,
        op: Op,
    ) {
        let engine = super::engine_from_core(core);
        match op {
            Op::SendMessage {
                content,
                mode,
                model,
                goal_objective,
                reasoning_effort,
                reasoning_effort_auto,
                auto_model,
                allow_shell,
                trust_mode,
                auto_approve,
                approval_mode,
                temperature,
                top_p,
                max_output_tokens,
            } => {
                engine
                    .handle_send_message(
                        content,
                        turn_loop_to_app_mode(mode),
                        model,
                        goal_objective,
                        reasoning_effort,
                        reasoning_effort_auto,
                        auto_model,
                        allow_shell,
                        trust_mode,
                        auto_approve,
                        approval_mode,
                        temperature,
                        top_p,
                        max_output_tokens,
                    )
                    .await;
            }
            Op::SpawnSubAgent { prompt } => {
                engine.handle_spawn_subagent_op(&prompt).await;
            }
            Op::ListSubAgents => {
                engine.handle_list_subagents_op().await;
            }
            Op::ChangeMode { mode } => {
                engine
                    .handle_change_mode_op(turn_loop_to_app_mode(mode))
                    .await;
            }
            Op::SetModel { model } => {
                engine.apply_set_model_op(model).await;
            }
            Op::SetCompaction { config } => {
                engine.apply_set_compaction_op(config).await;
            }
            Op::SyncSession {
                messages,
                system_prompt,
                model,
                workspace,
            } => {
                engine
                    .sync_session_from_op(messages, system_prompt, model, workspace)
                    .await;
            }
            Op::CompactContext => {
                engine.handle_compact_context_op().await;
            }
            Op::Rlm {
                content,
                model,
                child_model,
                max_depth,
            } => {
                engine
                    .handle_rlm_op(content, model, child_model, max_depth)
                    .await;
            }
            Op::EditLastTurn { new_message } => {
                engine.handle_edit_last_turn(new_message).await;
            }
            Op::QueryContext { reply } => {
                engine.handle_query_context_op(reply);
            }
            Op::QueryHarnessTaskGraph { reply } => {
                engine.handle_query_harness_task_graph_op(reply).await;
            }
            Op::QueryHarnessCycles { reply } => {
                engine.handle_query_harness_cycles_op(reply).await;
            }
            Op::CancelRequest
            | Op::ApproveToolCall { .. }
            | Op::DenyToolCall { .. }
            | Op::TruncateBeforeLastUserMessage { .. }
            | Op::Shutdown => {}
        }
    }

    async fn on_shutdown(
        &mut self,
        _core: &mut CoreEngine<
            crate::sandbox::SandboxPolicy,
            crate::tools::user_input::UserInputResponse,
        >,
    ) {
        if let Some(pool) = self.mcp_pool.as_ref() {
            let mut guard = pool.lock().await;
            guard.shutdown_all().await;
        }
    }
}

impl Engine {
    pub(in crate::core::engine) async fn handle_list_subagents_op(&self) {
        let agents = self.list_subagents().await;
        let _ = self.tx_event.send(Event::AgentList { agents }).await;
    }

    pub(in crate::core::engine) async fn handle_change_mode_op(&self, mode: AppMode) {
        let _ = self
            .tx_event
            .send(Event::status(format!("Mode changed to: {mode:?}")))
            .await;
    }

    pub(in crate::core::engine) fn handle_query_context_op(
        &self,
        reply: oneshot::Sender<ThreadContextSnapshot>,
    ) {
        let _ = reply.send(self.engine_context_snapshot());
    }

    pub(in crate::core::engine) async fn handle_query_harness_task_graph_op(
        &self,
        reply: oneshot::Sender<serde_json::Value>,
    ) {
        let plan = self.config_ext().plan_state.lock().await.snapshot();
        let checklist = self.config_ext().todos.lock().await.snapshot();
        let value = crate::long_horizon::build_task_graph_value(
            &plan,
            &checklist,
            &self.session.messages,
            &self.config.locale_tag,
            &self.config.long_horizon,
            Some(&self.runtime_ext().long_horizon_state),
        );
        let _ = reply.send(value);
    }

    pub(in crate::core::engine) async fn handle_query_harness_cycles_op(
        &self,
        reply: oneshot::Sender<serde_json::Value>,
    ) {
        let active = self.estimated_input_tokens() as u64;
        let headroom = super::context::turn_response_headroom_tokens();
        let pressure = crate::long_horizon::context_pressure_ratio(
            active,
            headroom,
            &self.session.model,
        )
        .map(|r| (r * 100.0).round() as u8);
        let archives =
            crate::cycle_manager::list_cycle_archive_summaries(&self.session.id);
        let value = crate::long_horizon::build_cycles_value(
            self.session.cycle_count,
            &self.session.cycle_briefings,
            &archives,
            pressure,
            Some(self.session.model.as_str()),
        );
        let _ = reply.send(value);
    }

    pub(in crate::core::engine) async fn handle_compact_context_op(&mut self) {
        self.handle_manual_compaction().await;
    }

    pub(in crate::core::engine) async fn handle_rlm_op(
        &mut self,
        content: String,
        model: String,
        child_model: String,
        max_depth: u32,
    ) {
        self.handle_rlm(content, model, child_model, max_depth).await;
    }
}
