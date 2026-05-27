use std::path::PathBuf;

use anyhow::Result;
use serde_json::Value;

use crate::tools::spec::{
    ToolError,
    optional_bool,
};

use deepseek_core::subagent::{
    SubAgentAssignment,
    SubAgentType,
};

use super::constants::VALID_SUBAGENT_TYPES;
use super::runtime::SubAgentRuntime;
use super::types::{AssignRequest, SpawnRequest, WaitMode};


pub(crate) fn parse_wait_mode(input: &Value) -> Result<WaitMode, ToolError> {
    let raw_mode = input
        .get("wait_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("any");
    WaitMode::from_str(raw_mode).ok_or_else(|| {
        ToolError::invalid_input(format!("Invalid wait_mode '{raw_mode}'. Use: any or all"))
    })
}

pub(crate) fn parse_wait_ids(input: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for key in ["ids", "agent_ids"] {
        if let Some(list) = input.get(key).and_then(|v| v.as_array()) {
            for value in list {
                if let Some(id) = value.as_str() {
                    let id = id.trim();
                    if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                        ids.push(id.to_string());
                    }
                }
            }
        }
    }

    for key in ["agent_id", "id"] {
        if let Some(id) = input.get(key).and_then(|v| v.as_str()) {
            let id = id.trim();
            if !id.is_empty() && !ids.iter().any(|existing| existing == id) {
                ids.push(id.to_string());
            }
        }
    }

    ids
}

pub(crate) fn optional_input_str<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .filter_map(|key| input.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
}

pub(crate) fn parse_text_or_items(
    input: &Value,
    text_keys: &[&str],
    items_key: &str,
    required_field: &str,
) -> Result<String, ToolError> {
    let text = optional_input_str(input, text_keys).map(str::to_string);
    let items = parse_items_text(input, items_key)?;
    match (text, items) {
        (Some(_), Some(_)) => Err(ToolError::invalid_input(format!(
            "Provide either {required_field} text or {items_key}, but not both"
        ))),
        (Some(text), None) => Ok(text),
        (None, Some(items)) => Ok(items),
        (None, None) => Err(ToolError::missing_field(required_field)),
    }
}

pub(crate) fn parse_optional_text_or_items(
    input: &Value,
    text_keys: &[&str],
    items_key: &str,
) -> Result<Option<String>, ToolError> {
    let text = optional_input_str(input, text_keys).map(str::to_string);
    let items = parse_items_text(input, items_key)?;
    match (text, items) {
        (Some(_), Some(_)) => Err(ToolError::invalid_input(format!(
            "Provide either {} text or {}, but not both",
            text_keys[0], items_key
        ))),
        (Some(text), None) => Ok(Some(text)),
        (None, Some(items)) => Ok(Some(items)),
        (None, None) => Ok(None),
    }
}

pub(crate) fn parse_items_text(input: &Value, key: &str) -> Result<Option<String>, ToolError> {
    let Some(items) = input.get(key) else {
        return Ok(None);
    };
    let array = items
        .as_array()
        .ok_or_else(|| ToolError::invalid_input(format!("'{key}' must be an array")))?;
    if array.is_empty() {
        return Err(ToolError::invalid_input(format!("'{key}' cannot be empty")));
    }

    let mut lines = Vec::new();
    for item in array {
        let object = item
            .as_object()
            .ok_or_else(|| ToolError::invalid_input("each item must be an object"))?;
        let item_type = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("text")
            .trim();
        let rendered = match item_type {
            "text" => object
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .ok_or_else(|| ToolError::invalid_input("text item requires non-empty text"))?,
            "mention" => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("mention item requires name"))?;
                let path = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("mention item requires path"))?;
                format!("[mention:${name}]({path})")
            }
            "skill" => {
                let name = object
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("skill item requires name"))?;
                let path = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("skill item requires path"))?;
                format!("[skill:${name}]({path})")
            }
            "local_image" => {
                let path = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("local_image item requires path"))?;
                format!("[local_image:{path}]")
            }
            "image" => {
                let url = object
                    .get("image_url")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| ToolError::invalid_input("image item requires image_url"))?;
                format!("[image:{url}]")
            }
            _ => object
                .get("text")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "[input]".to_string()),
        };
        lines.push(rendered);
    }

    Ok(Some(lines.join("\n")))
}

pub(crate) fn parse_spawn_request(input: &Value) -> Result<SpawnRequest, ToolError> {
    let prompt = parse_text_or_items(
        input,
        &["prompt", "message", "objective"],
        "items",
        "prompt",
    )?;

    let type_input = optional_input_str(input, &["type", "agent_type", "agent_name"]);
    let role_input = optional_input_str(input, &["role", "agent_role"]);

    let parsed_type = type_input
        .map(|kind| {
            SubAgentType::from_str(kind).ok_or_else(|| {
                ToolError::invalid_input(format!(
                    "Invalid sub-agent type '{kind}'. Use: {VALID_SUBAGENT_TYPES}"
                ))
            })
        })
        .transpose()?;

    let parsed_role_type = role_input
        .map(|role| {
            SubAgentType::from_str(role).ok_or_else(|| {
                ToolError::invalid_input(format!(
                    "Invalid role alias '{role}'. Use: worker, explorer, awaiter, default"
                ))
            })
        })
        .transpose()?;

    if let (Some(type_kind), Some(role_kind)) = (&parsed_type, &parsed_role_type)
        && type_kind != role_kind
    {
        return Err(ToolError::invalid_input(
            "Conflicting type/agent_type and role/agent_role values".to_string(),
        ));
    }

    let agent_type = parsed_type
        .or(parsed_role_type)
        .unwrap_or(SubAgentType::General);

    if let Some(role) = role_input
        && normalize_role_alias(role).is_none()
    {
        return Err(ToolError::invalid_input(format!(
            "Invalid role alias '{role}'. Use: worker, explorer, awaiter, default"
        )));
    }

    let role = role_input
        .and_then(normalize_role_alias)
        .or_else(|| type_input.and_then(normalize_role_alias))
        .map(str::to_string);

    let allowed_tools = input
        .get("allowed_tools")
        .and_then(|v| v.as_array())
        .map(|items| {
            let mut tools = Vec::new();
            for item in items {
                if let Some(tool) = item.as_str() {
                    let trimmed = tool.trim();
                    if !trimmed.is_empty() && !tools.iter().any(|existing| existing == trimmed) {
                        tools.push(trimmed.to_string());
                    }
                }
            }
            tools
        });

    let cwd = parse_optional_cwd(input)?;
    let model = parse_optional_subagent_model(input, "model")?;
    let resident_file = input
        .get("resident_file")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty());

    let task_id = input
        .get("task_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|s| !s.trim().is_empty());

    Ok(SpawnRequest {
        prompt: prompt.clone(),
        agent_type,
        assignment: SubAgentAssignment::new(prompt, role),
        allowed_tools,
        model,
        cwd,
        resident_file,
        task_id,
    })
}

pub(crate) fn normalize_requested_subagent_model(
    value: &str,
    field: &str,
) -> Result<String, ToolError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ToolError::invalid_input(format!("{field} cannot be blank")));
    }
    crate::config::normalize_model_name(trimmed).ok_or_else(|| {
        ToolError::invalid_input(format!(
            "Invalid {field} '{trimmed}'. Expected a DeepSeek model id such as deepseek-v4-pro or deepseek-v4-flash"
        ))
    })
}

pub(crate) fn configured_model_for_role_or_type(
    runtime: &SubAgentRuntime,
    role: Option<&str>,
    agent_type: &SubAgentType,
) -> Result<Option<String>, ToolError> {
    let mut keys = Vec::new();
    if let Some(role) = role.map(str::trim).filter(|role| !role.is_empty()) {
        keys.push(role.to_ascii_lowercase());
    }
    keys.push(agent_type.as_str().to_string());
    keys.push("default".to_string());

    for key in keys {
        if let Some(model) = runtime.role_models.get(&key) {
            return normalize_requested_subagent_model(model, &format!("subagents.{key}.model"))
                .map(Some);
        }
    }
    Ok(None)
}

pub(crate) fn parse_optional_subagent_model(input: &Value, key: &str) -> Result<Option<String>, ToolError> {
    match input.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => normalize_requested_subagent_model(value, key).map(Some),
        Some(_) => Err(ToolError::invalid_input(format!("{key} must be a string"))),
    }
}

/// Extract an optional `cwd: String` from spawn input and convert to a
/// `PathBuf`. Empty / absent → `None`. Workspace-boundary check happens
/// at spawn time (the parent's workspace is known there, not here).
pub(crate) fn parse_optional_cwd(input: &Value) -> Result<Option<PathBuf>, ToolError> {
    let raw = input.get("cwd").and_then(|v| v.as_str()).map(str::trim);
    match raw {
        None | Some("") => Ok(None),
        Some(s) => Ok(Some(PathBuf::from(s))),
    }
}

pub(crate) fn parse_assign_request(input: &Value) -> Result<AssignRequest, ToolError> {
    let agent_id = input
        .get("agent_id")
        .or_else(|| input.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| ToolError::missing_field("agent_id"))?
        .to_string();
    let objective = optional_input_str(input, &["objective"]).map(str::to_string);
    let role = optional_input_str(input, &["role", "agent_role"])
        .map(|role| {
            normalize_role_alias(role).ok_or_else(|| {
                ToolError::invalid_input(format!(
                    "Invalid role alias '{role}'. Use: worker, explorer, awaiter, default"
                ))
            })
        })
        .transpose()?
        .map(str::to_string);
    let message = parse_optional_text_or_items(input, &["message", "input"], "items")?;
    let interrupt = optional_bool(input, "interrupt", true);

    if objective.is_none() && role.is_none() && message.is_none() {
        return Err(ToolError::invalid_input(
            "Provide at least one of objective, role/agent_role, message/input, or items"
                .to_string(),
        ));
    }

    Ok(AssignRequest {
        agent_id,
        objective,
        role,
        message,
        interrupt,
    })
}

pub(crate) fn normalize_role_alias(input: &str) -> Option<&'static str> {
    match input.to_ascii_lowercase().as_str() {
        "default" => Some("default"),
        "worker" | "general" => Some("worker"),
        "explorer" | "explore" => Some("explorer"),
        "awaiter" | "plan" | "planner" => Some("awaiter"),
        _ => None,
    }
}

pub(crate) fn build_assignment_prompt(
    prompt: &str,
    assignment: &SubAgentAssignment,
    agent_type: &SubAgentType,
    blackboard_section: Option<&str>,
) -> String {
    let role = assignment.role.as_deref().unwrap_or("default");
    let header = format!(
        "Assignment metadata:\n- objective: {}\n- role: {}\n- resolved_type: {}",
        assignment.objective,
        role,
        agent_type.as_str(),
    );
    if let Some(bb) = blackboard_section {
        format!("{header}\n\n## Blackboard\n\n{bb}\n\nTask:\n{prompt}")
    } else {
        format!("{header}\n\nTask:\n{prompt}")
    }
}
