//! Sidecar `RuntimeThreadHost` — start-turn params (engine load/monitor in sibling modules).

use anyhow::Result;
use async_trait::async_trait;
use deepseek_core::engine::StartTurnParams;

use super::manager::parse_mode;
use super::{RuntimeThreadManager, StartTurnRequest};
use deepseek_runtime_orchestrator::runtime_threads::types::ThreadRecord;
use deepseek_runtime_orchestrator::runtime_threads::RuntimeThreadHost;

#[async_trait]
impl RuntimeThreadHost<super::RuntimeEnginePolicy, super::RuntimeUserInputResponse>
    for RuntimeThreadManager
{
    async fn spawn_engine_for_thread(
        &self,
        thread: &ThreadRecord,
    ) -> Result<
        deepseek_core::engine::handle::EngineHandle<
            super::RuntimeEnginePolicy,
            super::RuntimeUserInputResponse,
        >,
    > {
        self.spawn_engine_for_thread_impl(thread).await
    }

    async fn prepare_start_turn_params(
        &self,
        thread: &ThreadRecord,
        req: &StartTurnRequest,
        prompt: &str,
    ) -> Result<StartTurnParams> {
        let mode = parse_mode(req.mode.as_deref().unwrap_or(&thread.mode));
        let requested_model = req
            .model
            .clone()
            .unwrap_or_else(|| thread.model.clone());
        let auto_model = requested_model.trim().eq_ignore_ascii_case("auto");
        let (model, reasoning_effort) = if auto_model {
            let selection = crate::auto_route::resolve_auto_route_with_flash(
                &self.config,
                prompt,
                "",
                "auto",
                "auto",
            )
            .await;
            (
                selection.model,
                selection
                    .reasoning_effort
                    .map(|effort| effort.as_setting().to_string()),
            )
        } else {
            (requested_model, None)
        };
        let allow_shell = req.allow_shell.unwrap_or(thread.allow_shell);
        let trust_mode = req.trust_mode.unwrap_or(thread.trust_mode);
        let auto_approve = req.auto_approve.unwrap_or(thread.auto_approve);
        let approval_mode = if auto_approve {
            deepseek_core::approval::ApprovalMode::Auto
        } else {
            self.config
                .approval_policy
                .as_deref()
                .and_then(deepseek_core::approval::ApprovalMode::from_config_value)
                .unwrap_or(deepseek_core::approval::ApprovalMode::Suggest)
        };

        Ok(StartTurnParams {
            prompt: prompt.to_string(),
            mode: mode.as_setting().to_string(),
            model,
            reasoning_effort,
            reasoning_effort_auto: auto_model,
            auto_model,
            allow_shell,
            trust_mode,
            auto_approve,
            approval_mode,
            temperature: req.temperature,
            top_p: req.top_p,
            max_output_tokens: req.max_tokens,
        })
    }

    async fn monitor_turn(
        &self,
        thread_id: String,
        turn_id: String,
        engine: deepseek_core::engine::handle::EngineHandle<
            super::RuntimeEnginePolicy,
            super::RuntimeUserInputResponse,
        >,
    ) -> Result<()> {
        deepseek_runtime_orchestrator::runtime_threads::monitor_turn(
            self,
            self,
            thread_id,
            turn_id,
            engine,
        )
        .await
    }
}

impl RuntimeThreadManager {
    pub(crate) async fn ensure_engine_loaded(
        &self,
        thread: &ThreadRecord,
    ) -> Result<crate::core::engine::EngineHandle> {
        deepseek_runtime_orchestrator::runtime_threads::ensure_engine_loaded(
            self,
            self,
            thread,
        )
        .await
    }
}
