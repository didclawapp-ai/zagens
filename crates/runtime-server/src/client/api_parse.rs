use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use zagens_core::chat::is_deepseek_v4_model;

use crate::config::ApiProvider;
use crate::models::{ServerToolUsage, SystemPrompt, Usage};

use super::types::AvailableModel;

#[derive(Debug, Deserialize)]
struct ModelsListResponse {
    data: Vec<ModelListItem>,
}

#[derive(Debug, Deserialize)]
struct ModelListItem {
    id: String,
    #[serde(default)]
    owned_by: Option<String>,
    #[serde(default)]
    created: Option<u64>,
}

pub(super) fn parse_models_response(payload: &str) -> Result<Vec<AvailableModel>> {
    let parsed: ModelsListResponse =
        serde_json::from_str(payload).context("Failed to parse model list JSON")?;

    let mut models = parsed
        .data
        .into_iter()
        .map(|item| AvailableModel {
            id: item.id,
            owned_by: item.owned_by,
            created: item.created,
        })
        .collect::<Vec<_>>();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    Ok(models)
}

pub(crate) fn system_to_instructions(system: Option<SystemPrompt>) -> Option<String> {
    match system {
        Some(SystemPrompt::Text(text)) => Some(text),
        Some(SystemPrompt::Blocks(blocks)) => {
            let joined = blocks
                .into_iter()
                .map(|b| b.text)
                .collect::<Vec<_>>()
                .join("\n\n---\n\n");
            if joined.trim().is_empty() {
                None
            } else {
                Some(joined)
            }
        }
        None => None,
    }
}

pub(super) fn apply_reasoning_effort(
    body: &mut Value,
    effort: Option<&str>,
    provider: ApiProvider,
) {
    let Some(effort) = effort else {
        return;
    };
    let model = body.get("model").and_then(Value::as_str).unwrap_or("");
    let v4_hosted = is_deepseek_v4_model(model);
    let normalized = effort.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "off" | "disabled" | "none" | "false" => match provider {
            ApiProvider::Deepseek
            | ApiProvider::DeepseekCN
            | ApiProvider::Openrouter
            | ApiProvider::Novita
            | ApiProvider::Fireworks
            | ApiProvider::Sglang
            | ApiProvider::Vllm => {
                body["thinking"] = json!({ "type": "disabled" });
            }
            ApiProvider::Openai
            | ApiProvider::Ollama
            | ApiProvider::Agnes
            | ApiProvider::SenseNova => {
                if v4_hosted {
                    body["thinking"] = json!({ "type": "disabled" });
                }
            }
            ApiProvider::NvidiaNim => {
                body["chat_template_kwargs"] = json!({
                    "thinking": false,
                });
            }
        },
        "low" | "minimal" | "medium" | "mid" | "high" | "" => match provider {
            ApiProvider::Deepseek
            | ApiProvider::DeepseekCN
            | ApiProvider::Openrouter
            | ApiProvider::Novita
            | ApiProvider::Fireworks
            | ApiProvider::Sglang
            | ApiProvider::Vllm => {
                body["reasoning_effort"] = json!("high");
                body["thinking"] = json!({ "type": "enabled" });
            }
            ApiProvider::Openai
            | ApiProvider::Ollama
            | ApiProvider::Agnes
            | ApiProvider::SenseNova => {
                if v4_hosted {
                    body["reasoning_effort"] = json!("high");
                    body["thinking"] = json!({ "type": "enabled" });
                }
            }
            ApiProvider::NvidiaNim => {
                body["chat_template_kwargs"] = json!({
                    "thinking": true,
                    "reasoning_effort": "high",
                });
            }
        },
        "xhigh" | "max" | "highest" => match provider {
            ApiProvider::Deepseek
            | ApiProvider::DeepseekCN
            | ApiProvider::Openrouter
            | ApiProvider::Novita
            | ApiProvider::Fireworks
            | ApiProvider::Sglang
            | ApiProvider::Vllm => {
                body["reasoning_effort"] = json!("max");
                body["thinking"] = json!({ "type": "enabled" });
            }
            ApiProvider::Openai
            | ApiProvider::Ollama
            | ApiProvider::Agnes
            | ApiProvider::SenseNova => {
                if v4_hosted {
                    body["reasoning_effort"] = json!("max");
                    body["thinking"] = json!({ "type": "enabled" });
                }
            }
            ApiProvider::NvidiaNim => {
                body["chat_template_kwargs"] = json!({
                    "thinking": true,
                    "reasoning_effort": "max",
                });
            }
        },
        _ => {}
    }
}

pub(super) fn parse_usage(usage: Option<&Value>) -> Usage {
    let input_tokens = usage
        .and_then(|u| u.get("input_tokens").or_else(|| u.get("prompt_tokens")))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let mut output_tokens = usage
        .and_then(|u| {
            u.get("output_tokens")
                .or_else(|| u.get("completion_tokens"))
        })
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let reasoning_tokens_raw = usage
        .and_then(|u| u.get("completion_tokens_details"))
        .and_then(|details| details.get("reasoning_tokens"))
        .and_then(Value::as_u64);
    if output_tokens == 0
        && let Some(reasoning_tokens) = reasoning_tokens_raw
    {
        output_tokens = reasoning_tokens;
    }
    let cached_tokens = usage
        .and_then(|u| u.get("prompt_tokens_details"))
        .and_then(|details| details.get("cached_tokens"))
        .and_then(Value::as_u64);
    let prompt_cache_hit_tokens = usage
        .and_then(|u| u.get("prompt_cache_hit_tokens"))
        .and_then(Value::as_u64)
        .or(cached_tokens)
        .map(|v| v as u32);
    let prompt_cache_miss_tokens = usage
        .and_then(|u| u.get("prompt_cache_miss_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| cached_tokens.map(|cached| input_tokens.saturating_sub(cached)))
        .map(|v| v as u32);
    let reasoning_tokens = reasoning_tokens_raw.map(|v| v as u32);

    let server_tool_use = usage.and_then(|u| u.get("server_tool_use")).map(|server| {
        let code_execution_requests = server
            .get("code_execution_requests")
            .and_then(Value::as_u64)
            .map(|v| v as u32);
        let tool_search_requests = server
            .get("tool_search_requests")
            .and_then(Value::as_u64)
            .map(|v| v as u32);
        ServerToolUsage {
            code_execution_requests,
            tool_search_requests,
        }
    });

    Usage {
        input_tokens: input_tokens as u32,
        output_tokens: output_tokens as u32,
        prompt_cache_hit_tokens,
        prompt_cache_miss_tokens,
        reasoning_tokens,
        reasoning_replay_tokens: None,
        server_tool_use,
    }
}
