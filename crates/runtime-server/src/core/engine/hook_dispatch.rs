//! Config-driven lifecycle hook dispatch into the agent turn loop.

use std::sync::Arc;

use serde_json::Value;
use zagens_tools::{ToolError, ToolResult};

use crate::agent_surface::AppMode;
use crate::hooks::{HookContext, HookEvent, HookExecutor};

use super::Engine;

impl Engine {
    pub(in crate::core::engine) fn hook_context(&self, mode: AppMode) -> HookContext {
        let total_tokens = self
            .session
            .total_usage
            .input_tokens
            .saturating_add(self.session.total_usage.output_tokens);
        self.runtime_ext()
            .hook_executor
            .base_context()
            .with_mode(mode.as_setting())
            .with_model(&self.session.model)
            .with_tokens(total_tokens.min(u32::MAX as u64) as u32)
    }

    pub(in crate::core::engine) fn maybe_fire_session_start(&mut self, mode: AppMode) {
        if self.runtime_ext().session_hooks_started {
            return;
        }
        self.runtime_ext_mut().session_hooks_started = true;
        let executor = Arc::clone(&self.runtime_ext().hook_executor);
        if !executor.has_hooks_for_event(HookEvent::SessionStart) {
            return;
        }
        let ctx = self.hook_context(mode);
        executor.execute(HookEvent::SessionStart, &ctx);
    }

    pub(in crate::core::engine) fn fire_session_end(&self) {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::SessionEnd) {
            return;
        }
        let ctx = self.hook_context(self.runtime_ext().turn_app_mode);
        executor.execute(HookEvent::SessionEnd, &ctx);
    }

    pub(in crate::core::engine) fn fire_message_submit(
        &self,
        mode: AppMode,
        content: &str,
    ) -> Result<(), String> {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::MessageSubmit) {
            return Ok(());
        }
        let ctx = self.hook_context(mode).with_message(content);
        executor
            .execute_blocking(HookEvent::MessageSubmit, &ctx)
            .map(|_| ())
    }

    pub(in crate::core::engine) fn fire_mode_change(&self, previous: AppMode, next: AppMode) {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::ModeChange) {
            return;
        }
        let ctx = self
            .hook_context(next)
            .with_previous_mode(previous.as_setting());
        executor.execute(HookEvent::ModeChange, &ctx);
    }

    pub(in crate::core::engine) fn fire_on_error(&self, mode: AppMode, error: &str) {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::OnError) {
            return;
        }
        let ctx = self.hook_context(mode).with_error(error);
        executor.execute(HookEvent::OnError, &ctx);
    }

    pub(in crate::core::engine) fn fire_tool_call_before(
        &self,
        mode: AppMode,
        tool_name: &str,
        tool_input: &Value,
    ) -> Result<Option<Value>, String> {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::ToolCallBefore) {
            return Ok(None);
        }
        let ctx = self
            .hook_context(mode)
            .with_tool_name(tool_name)
            .with_tool_args(tool_input);
        let (_, outcome) =
            executor.execute_blocking_with_outcome(HookEvent::ToolCallBefore, &ctx)?;
        Ok(outcome.updated_tool_input)
    }

    pub(in crate::core::engine) fn fire_pre_compact(&self, mode: AppMode, manual: bool) {
        let executor = &self.runtime_ext().hook_executor;
        if !executor.has_hooks_for_event(HookEvent::PreCompact) {
            return;
        }
        let ctx = self
            .hook_context(mode)
            .with_message(if manual { "manual" } else { "auto" });
        executor.execute(HookEvent::PreCompact, &ctx);
    }

    pub(in crate::core::engine) fn fire_post_compact(
        &self,
        mode: AppMode,
        manual: bool,
        messages_before: usize,
        messages_after: usize,
    ) {
        self.runtime_ext().hook_executor.fire_post_compact(
            &self.hook_context(mode),
            manual,
            messages_before,
            messages_after,
        );
    }

    pub(in crate::core::engine) fn fire_tool_call_after(
        &self,
        mode: AppMode,
        tool_name: &str,
        tool_input: &Value,
        result: &Result<ToolResult, ToolError>,
    ) {
        let executor = &self.runtime_ext().hook_executor;
        fire_tool_call_after_with_executor(
            executor,
            self.hook_context(mode),
            tool_name,
            tool_input,
            result,
        );
    }
}

pub(in crate::core::engine) fn fire_tool_call_after_with_executor(
    executor: &HookExecutor,
    ctx: HookContext,
    tool_name: &str,
    tool_input: &Value,
    result: &Result<ToolResult, ToolError>,
) {
    if !executor.has_hooks_for_event(HookEvent::ToolCallAfter) {
        return;
    }
    let (output, success, exit_code) = match result {
        Ok(tool_result) => {
            // Extract exit_code from structured metadata when available (e.g. exec_shell).
            let exit_code = tool_result
                .metadata
                .as_ref()
                .and_then(|m| m.get("exit_code"))
                .and_then(|v| v.as_i64())
                .map(|c| c as i32);
            (tool_result.content.clone(), tool_result.success, exit_code)
        }
        Err(err) => (err.to_string(), false, None),
    };
    let ctx = ctx
        .with_tool_name(tool_name)
        .with_tool_args(tool_input)
        .with_tool_result(&output, success, exit_code);
    executor.execute(HookEvent::ToolCallAfter, &ctx);
}
