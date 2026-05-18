//! Per-turn tool registry setup.
//!
//! This keeps mode/feature-specific registry construction out of the send path.

use super::*;
use crate::task_type::TaskType;

impl Engine {
    pub(super) fn build_turn_tool_registry_builder(
        &self,
        mode: AppMode,
        todo_list: SharedTodoList,
        plan_state: SharedPlanState,
    ) -> ToolRegistryBuilder {
        if self.config.task_type == TaskType::Office {
            // Office always ships web_search / fetch_url / finance / web.run (install-time
            // networking only). Code mode still respects Feature::WebSearch.
            let mut builder = ToolRegistryBuilder::new().with_office_surface(true);
            if self.config.memory_enabled {
                builder = builder.with_remember_tool();
            }
            return builder;
        }

        let mut builder = if mode == AppMode::Plan {
            ToolRegistryBuilder::new()
                .with_read_only_file_tools()
                .with_search_tools()
                .with_git_tools()
                .with_git_history_tools()
                .with_diagnostics_tool()
                .with_skill_tools()
                .with_validation_tools()
                .with_runtime_task_tools()
                .with_todo_tool(todo_list)
                .with_plan_tool(plan_state)
        } else {
            ToolRegistryBuilder::new()
                .with_agent_tools(self.session.allow_shell)
                .with_todo_tool(todo_list)
                .with_plan_tool(plan_state)
        };

        builder = builder
            .with_review_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_rlm_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_fim_tool(self.deepseek_client.clone(), self.session.model.clone())
            .with_user_input_tool()
            .with_parallel_tool();

        if self.config.features.enabled(Feature::ApplyPatch) && mode != AppMode::Plan {
            builder = builder.with_patch_tools();
        }
        if self.config.features.enabled(Feature::WebSearch) {
            builder = builder.with_web_tools();
        }
        if self.config.features.enabled(Feature::ShellTool) && self.session.allow_shell {
            builder = builder.with_shell_tools();
        }

        if self.config.memory_enabled {
            builder = builder.with_remember_tool();
        }

        builder
    }
}
