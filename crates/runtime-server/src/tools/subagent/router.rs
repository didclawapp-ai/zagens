use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::config::MAX_SUBAGENTS;
use deepseek_core::events::Event;
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt, Tool};
use crate::tools::plan::{PlanState, SharedPlanState};
use crate::tools::registry::{ToolRegistry, ToolRegistryBuilder};
use crate::tools::spec::{
    ApprovalRequirement, ToolCapability, ToolContext, ToolError, ToolResult, ToolSpec,
    optional_bool, optional_u64, required_str,
};
use crate::tools::todo::{SharedTodoList, TodoList};
use crate::utils::spawn_supervised;

use super::blackboard::{read_blackboard_section, write_blackboard_partition};
use deepseek_core::subagent::{
    MailboxMessage, StructuredVerdict, SubAgentAssignment, SubAgentResult, SubAgentStatus,
    SubAgentType, VerdictLevel,
};
use super::mailbox::{Mailbox, MailboxEnvelope, MailboxReceiver};

use super::runtime::SubAgentRuntime;


#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubAgentResolvedRoute {
    pub(crate) model: String,
    pub(crate) reasoning_effort: Option<String>,
}

pub async fn resolve_subagent_assignment_route(
    runtime: &SubAgentRuntime,
    configured_model: Option<String>,
    prompt: &str,
) -> SubAgentResolvedRoute {
    let explicit_model = configured_model.is_some();
    let mut route = fallback_subagent_assignment_route(runtime, configured_model, prompt);

    if (runtime.auto_model || runtime.reasoning_effort_auto)
        && let Ok(Some(recommendation)) = subagent_flash_router(runtime, prompt).await
    {
        if runtime.auto_model && !explicit_model {
            route.model = recommendation.model;
        }
        if runtime.reasoning_effort_auto {
            route.reasoning_effort = recommendation
                .reasoning_effort
                .map(|effort| effort.as_setting().to_string())
                .or(route.reasoning_effort);
        }
    }

    route
}

pub(crate) fn fallback_subagent_assignment_route(
    runtime: &SubAgentRuntime,
    configured_model: Option<String>,
    prompt: &str,
) -> SubAgentResolvedRoute {
    let model = if let Some(model) = configured_model {
        model
    } else if runtime.auto_model {
        crate::auto_route::auto_model_heuristic(prompt, &runtime.model)
    } else {
        runtime.model.clone()
    };

    let reasoning_effort = if runtime.reasoning_effort_auto {
        let effort = match crate::auto_reasoning::select(false, prompt) {
            crate::agent_surface::ReasoningEffort::Low
                | crate::agent_surface::ReasoningEffort::Medium => {
                crate::agent_surface::ReasoningEffort::High
            }
            other => other,
        };
        Some(effort.as_setting().to_string())
    } else {
        runtime.reasoning_effort.clone()
    };

    SubAgentResolvedRoute {
        model,
        reasoning_effort,
    }
}

pub(crate) async fn subagent_flash_router(
    runtime: &SubAgentRuntime,
    prompt: &str,
) -> Result<Option<crate::auto_route::AutoRouteRecommendation>> {
    if cfg!(test) {
        return Ok(None);
    }

    let request = MessageRequest {
        model: "deepseek-v4-flash".to_string(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: subagent_router_prompt(runtime, prompt),
                cache_control: None,
            }],
        }],
        max_tokens: 96,
        system: Some(SystemPrompt::Text(
            SUBAGENT_ROUTER_SYSTEM_PROMPT.to_string(),
        )),
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort: Some("off".to_string()),
        stream: Some(false),
        temperature: Some(0.0),
        top_p: None,
    };

    let response = tokio::time::timeout(
        Duration::from_secs(4),
        runtime.client.create_message(request),
    )
    .await??;
    Ok(crate::auto_route::parse_auto_route_recommendation(
        &message_response_text(&response.content),
    ))
}

const SUBAGENT_ROUTER_SYSTEM_PROMPT: &str = "\
You are the DeepSeek TUI sub-agent routing manager. Return only compact JSON: \
{\"model\":\"deepseek-v4-flash|deepseek-v4-pro\",\"thinking\":\"off|high|max\"}. \
Treat each child assignment like a customer request entering a team queue: decide the least \
sufficient worker and thinking budget for that assignment. Do not treat being a sub-agent as \
important by itself. Use Flash for trivial, read-only, status, lookup, or single-step work. \
Use Pro for coding, debugging, release work, multi-file changes, security, architecture, \
high-risk decisions, ambiguous requests, or work likely to need tool-call judgment. Use thinking \
off for trivial no-tool work, high for ordinary reasoning, and max only for hard, risky, \
multi-step, uncertain, or tool-heavy work.";

pub(crate) fn subagent_router_prompt(runtime: &SubAgentRuntime, prompt: &str) -> String {
    format!(
        "Parent selected model mode: {}\nParent selected thinking mode: {}\n\nSub-agent assignment:\n{}\n\nReturn JSON only.",
        if runtime.auto_model { "auto" } else { "fixed" },
        if runtime.reasoning_effort_auto {
            "auto"
        } else {
            runtime
                .reasoning_effort
                .as_deref()
                .unwrap_or("provider-default")
        },
        truncate_subagent_router_prompt(prompt, 4_000)
    )
}

pub(crate) fn truncate_subagent_router_prompt(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out = text.chars().take(max_chars).collect::<String>();
    out.push_str("\n[truncated]");
    out
}

pub(crate) fn message_response_text(blocks: &[ContentBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        match block {
            ContentBlock::Text { text, .. } => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(text);
            }
            ContentBlock::Thinking { thinking } => {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(thinking);
            }
            _ => {}
        }
    }
    out
}


