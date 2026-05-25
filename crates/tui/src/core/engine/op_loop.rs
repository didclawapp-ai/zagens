//! Engine background task: [`Op`] dispatch loop.

use super::*;
use super::turn_loop::host_impl::turn_loop_to_app_mode;

impl Engine {
    /// Run the engine event loop
    pub async fn run(mut self) {
        while let Some(op) = self.rx_op.recv().await {
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
                    self.handle_send_message(
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
                Op::CancelRequest => self.handle_cancel_request_op(),
                Op::ApproveToolCall { id } => self.handle_approve_tool_call_op(&id).await,
                Op::DenyToolCall { id } => self.handle_deny_tool_call_op(&id).await,
                Op::SpawnSubAgent { prompt } => self.handle_spawn_subagent_op(&prompt).await,
                Op::ListSubAgents => self.handle_list_subagents_op().await,
                Op::ChangeMode { mode } => {
                    self.handle_change_mode_op(turn_loop_to_app_mode(mode)).await
                }
                Op::SetModel { model } => self.apply_set_model_op(model).await,
                Op::SetCompaction { config } => self.apply_set_compaction_op(config).await,
                Op::SyncSession {
                    messages,
                    system_prompt,
                    model,
                    workspace,
                } => {
                    self.sync_session_from_op(messages, system_prompt, model, workspace)
                        .await;
                }
                Op::CompactContext => self.handle_compact_context_op().await,
                Op::Rlm {
                    content,
                    model,
                    child_model,
                    max_depth,
                } => {
                    self.handle_rlm_op(content, model, child_model, max_depth)
                        .await;
                }
                Op::EditLastTurn { new_message } => self.handle_edit_last_turn(new_message).await,
                Op::TruncateBeforeLastUserMessage { reply } => {
                    let truncated = deepseek_core::session::truncate_before_last_user_message(
                        &mut self.session.messages,
                    );
                    let _ = reply.send(truncated);
                }
                Op::QueryContext { reply } => self.handle_query_context_op(reply),
                Op::Shutdown => break,
            }
        }

        if let Some(pool) = self.mcp_pool.as_ref() {
            let mut guard = pool.lock().await;
            guard.shutdown_all().await;
        }
    }
}
