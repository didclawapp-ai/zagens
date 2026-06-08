//! Workspace hook loading, event alias resolution, and config merge.

use std::collections::HashMap;
use std::path::Path;

use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use zagens_config::{WORKSPACE_META_DIR_NAME, workspace_meta_dir_read};

use crate::hooks::{Hook, HookCondition, HookEvent, HooksConfig};

const WORKSPACE_HOOKS_FILE: &str = "hooks.toml";
const WORKSPACE_HOOKS_JSON: &str = "hooks.json";
const CURSOR_HOOKS_JSON: &str = ".cursor/hooks.json";

/// Parse a hook event name, including Cursor-style aliases.
/// Returns `(canonical event, optional implicit condition)`.
pub fn parse_hook_event_name(raw: &str) -> Result<(HookEvent, Option<HookCondition>), String> {
    let key = raw.trim().replace(['-', ' '], "_").to_ascii_lowercase();
    match key.as_str() {
        "session_start" | "sessionstart" => Ok((HookEvent::SessionStart, None)),
        "session_end" | "sessionend" | "stop" => Ok((HookEvent::SessionEnd, None)),
        "message_submit" | "messagesubmit" | "before_submit_prompt" | "beforesubmitprompt" => {
            Ok((HookEvent::MessageSubmit, None))
        }
        "tool_call_before" | "toolcallbefore" | "pretooluse" | "pre_tool_use" => {
            Ok((HookEvent::ToolCallBefore, None))
        }
        "tool_call_after" | "toolcallafter" | "posttooluse" | "post_tool_use"
        | "posttoolusefailure" => Ok((HookEvent::ToolCallAfter, None)),
        "before_shell" | "beforeshell" | "beforeshellexecution" => Ok((
            HookEvent::ToolCallBefore,
            Some(HookCondition::ToolName {
                name: "exec_shell".to_string(),
            }),
        )),
        "after_shell" | "aftershell" | "aftershellexecution" => Ok((
            HookEvent::ToolCallAfter,
            Some(HookCondition::ToolName {
                name: "exec_shell".to_string(),
            }),
        )),
        "before_file_edit" | "beforefileedit" | "afterfileedit" | "after_file_edit" => {
            let event = if key.contains("before") {
                HookEvent::ToolCallBefore
            } else {
                HookEvent::ToolCallAfter
            };
            Ok((
                event,
                Some(HookCondition::ToolCategory {
                    category: "file_write".to_string(),
                }),
            ))
        }
        "before_read_file" | "beforereadfile" | "beforetabfileread" => Ok((
            HookEvent::ToolCallBefore,
            Some(HookCondition::ToolName {
                name: "read_file".to_string(),
            }),
        )),
        "beforemcpexecution" | "aftermcpexecution" => {
            let event = if key.contains("before") {
                HookEvent::ToolCallBefore
            } else {
                HookEvent::ToolCallAfter
            };
            Ok((
                event,
                Some(HookCondition::ToolNameRegex {
                    pattern: r"^mcp_".to_string(),
                }),
            ))
        }
        "mode_change" | "modechange" => Ok((HookEvent::ModeChange, None)),
        "on_error" | "onerror" => Ok((HookEvent::OnError, None)),
        "shell_env" | "shellenv" => Ok((HookEvent::ShellEnv, None)),
        "pre_compact" | "precompact" => Ok((HookEvent::PreCompact, None)),
        "post_compact" | "postcompact" => Ok((HookEvent::PostCompact, None)),
        "subagent_start" | "subagentstart" => Ok((HookEvent::SubagentStart, None)),
        "subagent_end" | "subagentstop" | "subagent_stop" => Ok((HookEvent::SubagentEnd, None)),
        other => Err(format!("unknown hook event: {other}")),
    }
}

fn merge_implicit_condition(existing: &mut Option<HookCondition>, implicit: Option<HookCondition>) {
    let Some(implicit) = implicit else {
        return;
    };
    match existing.take() {
        None => *existing = Some(implicit),
        Some(current) => {
            *existing = Some(HookCondition::All {
                conditions: vec![implicit, current],
            });
        }
    }
}

/// Apply alias expansion and implicit tool filters on a hook definition.
pub fn normalize_hook(hook: &mut Hook) {
    if let Ok((event, implicit)) = parse_hook_event_name(hook.event.as_str()) {
        hook.event = event;
        merge_implicit_condition(&mut hook.condition, implicit);
    }
}

/// Load optional project hooks from workspace metadata and Cursor-compatible paths.
pub fn load_workspace_hooks(workspace: &Path) -> HooksConfig {
    let mut merged = HooksConfig::default();
    let zagens_dir = workspace_meta_dir_read(workspace);

    if let Some(cfg) = load_hooks_toml_file(&zagens_dir.join(WORKSPACE_HOOKS_FILE)) {
        merged = merge_hooks_configs(merged, cfg);
    }
    if let Some(cfg) = load_hooks_json_file(&zagens_dir.join(WORKSPACE_HOOKS_JSON)) {
        merged = merge_hooks_configs(merged, cfg);
    }
    if let Some(cfg) = load_hooks_json_file(&workspace.join(CURSOR_HOOKS_JSON)) {
        merged = merge_hooks_configs(merged, cfg);
    }

    merged
}

fn load_hooks_toml_file(path: &Path) -> Option<HooksConfig> {
    if !path.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(path).unwrap_or_else(|err| {
        tracing::warn!(
            target: "hooks",
            path = %path.display(),
            error = %err,
            "failed to read workspace hooks file"
        );
        String::new()
    });
    if content.is_empty() {
        return None;
    }
    match toml::from_str::<HooksConfig>(&content) {
        Ok(cfg) => {
            let cfg = cfg.normalized();
            tracing::info!(
                target: "hooks",
                path = %path.display(),
                count = cfg.hooks.len(),
                "loaded workspace hooks (toml)"
            );
            Some(cfg)
        }
        Err(err) => {
            tracing::warn!(
                target: "hooks",
                path = %path.display(),
                error = %err,
                "failed to parse workspace hooks file"
            );
            None
        }
    }
}

#[derive(Debug, Deserialize)]
struct CursorHooksJson {
    #[serde(default)]
    #[allow(dead_code)]
    version: u64,
    #[serde(default)]
    hooks: HashMap<String, Vec<CursorHookEntry>>,
}

#[derive(Debug, Deserialize)]
struct CursorHookEntry {
    command: String,
    #[serde(default, rename = "type")]
    hook_type: Option<String>,
    #[serde(default)]
    matcher: Option<String>,
    #[serde(default)]
    timeout: Option<u64>,
    #[serde(default, rename = "failClosed")]
    fail_closed: Option<bool>,
    #[serde(default)]
    name: Option<String>,
}

fn map_cursor_subagent_matcher(matcher: &str) -> HookCondition {
    let parts = matcher
        .split('|')
        .map(map_cursor_subagent_token)
        .collect::<Vec<_>>();
    HookCondition::ToolNameRegex {
        pattern: format!("^({})$", parts.join("|")),
    }
}

fn map_cursor_subagent_token(raw: &str) -> String {
    match raw.trim() {
        "generalPurpose" => "general".to_string(),
        other => other.trim().to_ascii_lowercase(),
    }
}

fn map_cursor_tool_matcher(matcher: &str) -> HookCondition {
    match matcher.trim() {
        "Shell" => HookCondition::ToolName {
            name: "exec_shell".to_string(),
        },
        "Read" | "TabRead" => HookCondition::ToolName {
            name: "read_file".to_string(),
        },
        "Write" | "TabWrite" => HookCondition::ToolCategory {
            category: "file_write".to_string(),
        },
        "Task" => HookCondition::ToolName {
            name: "agent_spawn".to_string(),
        },
        other if other.starts_with("MCP:") => HookCondition::ToolNameRegex {
            pattern: format!("(?i)^{}", regex::escape(other)),
        },
        other => HookCondition::ToolNameRegex {
            pattern: other.to_string(),
        },
    }
}

fn cursor_matcher_for_event(event: HookEvent, matcher: &str) -> HookCondition {
    match event {
        HookEvent::SubagentStart | HookEvent::SubagentEnd => map_cursor_subagent_matcher(matcher),
        HookEvent::ToolCallBefore | HookEvent::ToolCallAfter => map_cursor_tool_matcher(matcher),
        _ => HookCondition::ToolNameRegex {
            pattern: matcher.to_string(),
        },
    }
}

fn cursor_entry_to_hook(event_raw: &str, entry: CursorHookEntry) -> Option<Hook> {
    if entry
        .hook_type
        .as_deref()
        .is_some_and(|t| t.eq_ignore_ascii_case("prompt"))
    {
        tracing::warn!(
            target: "hooks",
            event = event_raw,
            "skipping prompt hook (command hooks only)"
        );
        return None;
    }
    let (event, implicit) = parse_hook_event_name(event_raw).ok()?;
    let mut condition = entry
        .matcher
        .as_ref()
        .map(|m| cursor_matcher_for_event(event, m));
    merge_implicit_condition(&mut condition, implicit);
    Some(Hook {
        event,
        command: entry.command,
        condition,
        timeout_secs: entry.timeout.unwrap_or(30),
        background: false,
        continue_on_error: !entry.fail_closed.unwrap_or(false),
        name: entry.name,
    })
}

/// Parse Cursor-style `hooks.json` (`version` + event map).
pub fn load_hooks_json_file(path: &Path) -> Option<HooksConfig> {
    if !path.is_file() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: CursorHooksJson = serde_json::from_str(&content)
        .map_err(|err| {
            tracing::warn!(
                target: "hooks",
                path = %path.display(),
                error = %err,
                "failed to parse hooks.json"
            );
            err
        })
        .ok()?;
    let mut hooks = Vec::new();
    for (event_raw, entries) in parsed.hooks {
        for entry in entries {
            if let Some(hook) = cursor_entry_to_hook(&event_raw, entry) {
                hooks.push(hook);
            }
        }
    }
    if hooks.is_empty() {
        return None;
    }
    tracing::info!(
        target: "hooks",
        path = %path.display(),
        count = hooks.len(),
        "loaded workspace hooks (json)"
    );
    Some(
        HooksConfig {
            enabled: true,
            hooks,
            ..Default::default()
        }
        .normalized(),
    )
}

/// Merge global `~/.zagens/config.toml` hooks with project-local hooks.
/// Project hooks run after global hooks; project `enabled = false` disables only local entries.
pub fn merge_hooks_configs(global: HooksConfig, project: HooksConfig) -> HooksConfig {
    if project.hooks.is_empty() && project.enabled == global.enabled {
        return global.normalized();
    }
    let mut merged = global.normalized();
    let project_timeout = project.default_timeout_secs;
    let project_working_dir = project.working_dir.clone();
    let project_audit_jsonl = project.audit_jsonl.clone();
    let project_enabled = project.enabled;
    if project.enabled {
        let local = project.normalized();
        merged.hooks.extend(local.hooks);
    }
    merged.enabled = merged.enabled || project_enabled;
    merged.default_timeout_secs = merged.default_timeout_secs.or(project_timeout);
    if merged.working_dir.is_none() {
        merged.working_dir = project_working_dir;
    }
    if merged.audit_jsonl.is_none() {
        merged.audit_jsonl = project_audit_jsonl;
    }
    merged
}

impl HooksConfig {
    /// Normalize every hook (alias → canonical event + conditions).
    pub fn normalized(mut self) -> Self {
        for hook in &mut self.hooks {
            normalize_hook(hook);
        }
        self
    }
}

impl<'de> Deserialize<'de> for HookEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        parse_hook_event_name(&raw)
            .map(|(event, _)| event)
            .map_err(de::Error::custom)
    }
}

impl Serialize for HookEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

struct HookDeserializeVisitor;

impl<'de> Visitor<'de> for HookDeserializeVisitor {
    type Value = Hook;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a hook table")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut event_raw: Option<String> = None;
        let mut command: Option<String> = None;
        let mut condition: Option<HookCondition> = None;
        let mut timeout_secs: Option<u64> = None;
        let mut background: Option<bool> = None;
        let mut continue_on_error: Option<bool> = None;
        let mut name: Option<Option<String>> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "event" => event_raw = Some(map.next_value()?),
                "command" => command = Some(map.next_value()?),
                "condition" => condition = Some(map.next_value()?),
                "timeout_secs" => timeout_secs = Some(map.next_value()?),
                "background" => background = Some(map.next_value()?),
                "continue_on_error" => continue_on_error = Some(map.next_value()?),
                "name" => name = Some(map.next_value()?),
                other => {
                    let _ = map.next_value::<de::IgnoredAny>()?;
                    tracing::debug!(target: "hooks", field = other, "ignoring unknown hook field");
                }
            }
        }

        let event_raw = event_raw.ok_or_else(|| de::Error::missing_field("event"))?;
        let command = command.ok_or_else(|| de::Error::missing_field("command"))?;
        let (event, implicit) = parse_hook_event_name(&event_raw).map_err(de::Error::custom)?;
        let mut condition = condition;
        merge_implicit_condition(&mut condition, implicit);

        Ok(Hook {
            event,
            command,
            condition,
            timeout_secs: timeout_secs.unwrap_or(30),
            background: background.unwrap_or(false),
            continue_on_error: continue_on_error.unwrap_or(true),
            name: name.unwrap_or(None),
        })
    }
}

impl<'de> Deserialize<'de> for Hook {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(HookDeserializeVisitor)
    }
}

impl Serialize for Hook {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Hook", 7)?;
        state.serialize_field("event", &self.event)?;
        state.serialize_field("command", &self.command)?;
        if let Some(ref condition) = self.condition {
            state.serialize_field("condition", condition)?;
        }
        if self.timeout_secs != 30 {
            state.serialize_field("timeout_secs", &self.timeout_secs)?;
        }
        if self.background {
            state.serialize_field("background", &self.background)?;
        }
        if !self.continue_on_error {
            state.serialize_field("continue_on_error", &self.continue_on_error)?;
        }
        if let Some(ref name) = self.name {
            state.serialize_field("name", name)?;
        }
        state.end()
    }
}

/// Relative path shown in docs (`{WORKSPACE_META_DIR_NAME}/hooks.toml`).
pub fn workspace_hooks_relative_path() -> String {
    format!("{WORKSPACE_META_DIR_NAME}/{WORKSPACE_HOOKS_FILE}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_before_shell_alias_expands_condition() {
        let (event, cond) = parse_hook_event_name("beforeShell").unwrap();
        assert_eq!(event, HookEvent::ToolCallBefore);
        assert!(matches!(
            cond,
            Some(HookCondition::ToolName { name }) if name == "exec_shell"
        ));
    }

    #[test]
    fn hook_toml_deserialize_applies_alias_condition() {
        let raw = r#"
event = "before_shell"
command = "echo test"
"#;
        let hook: Hook = toml::from_str(raw).unwrap();
        assert_eq!(hook.event, HookEvent::ToolCallBefore);
        assert!(matches!(
            hook.condition,
            Some(HookCondition::ToolName { name }) if name == "exec_shell"
        ));
    }

    #[test]
    fn merge_hooks_appends_project_entries() {
        let global = HooksConfig {
            enabled: true,
            hooks: vec![Hook::new(HookEvent::SessionStart, "echo g")],
            ..Default::default()
        };
        let project = HooksConfig {
            enabled: true,
            hooks: vec![Hook::new(HookEvent::SessionEnd, "echo p")],
            ..Default::default()
        };
        let merged = merge_hooks_configs(global, project);
        assert_eq!(merged.hooks.len(), 2);
    }

    #[test]
    fn cursor_hooks_json_parses_session_start() {
        let raw = r#"{
            "version": 1,
            "hooks": {
                "sessionStart": [{ "command": "echo hi" }]
            }
        }"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.json");
        std::fs::write(&path, raw).unwrap();
        let cfg = load_hooks_json_file(&path).expect("json hooks");
        assert_eq!(cfg.hooks.len(), 1);
        assert_eq!(cfg.hooks[0].event, HookEvent::SessionStart);
    }

    #[test]
    fn cursor_before_shell_maps_to_exec_shell() {
        let raw = r#"{
            "version": 1,
            "hooks": {
                "beforeShellExecution": [{ "command": "echo gate", "matcher": "curl" }]
            }
        }"#;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hooks.json");
        std::fs::write(&path, raw).unwrap();
        let cfg = load_hooks_json_file(&path).unwrap();
        assert_eq!(cfg.hooks[0].event, HookEvent::ToolCallBefore);
        assert!(matches!(
            cfg.hooks[0].condition,
            Some(HookCondition::All { .. })
        ));
    }
}
