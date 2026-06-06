use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::network_policy::NetworkPolicyDecider;
use crate::util::write_atomic;

use super::auth::merge_preserved_secrets;
use super::config::{McpConfig, McpServerConfig, McpTransportKind};
use super::pool::McpPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpWriteStatus {
    Created,
    Overwritten,
    SkippedExists,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpDiscoveredItem {
    pub name: String,
    pub model_name: String,
    pub description: Option<String>,
    /// Whether the tool is enabled in server config (`enabled_tools` / `disabled_tools`).
    #[serde(default = "default_item_enabled")]
    pub enabled: bool,
}

fn default_item_enabled() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpServerSnapshot {
    pub name: String,
    pub enabled: bool,
    pub required: bool,
    pub transport: String,
    pub command_or_url: String,
    pub connect_timeout: u64,
    pub execute_timeout: u64,
    pub read_timeout: u64,
    pub connected: bool,
    pub error: Option<String>,
    pub tools: Vec<McpDiscoveredItem>,
    pub resources: Vec<McpDiscoveredItem>,
    pub prompts: Vec<McpDiscoveredItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct McpManagerSnapshot {
    pub config_path: std::path::PathBuf,
    pub config_exists: bool,
    pub restart_required: bool,
    pub servers: Vec<McpServerSnapshot>,
}

pub fn load_config(path: &Path) -> Result<McpConfig> {
    if !path.exists() {
        return Ok(McpConfig::default());
    }
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read MCP config {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse MCP config {}", path.display()))
}

pub fn save_config(path: &Path, cfg: &McpConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create MCP config directory {}", parent.display())
        })?;
    }
    let rendered = serde_json::to_string_pretty(cfg).context("Failed to serialize MCP config")?;
    write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("Failed to write MCP config {}", path.display()))?;
    Ok(())
}

fn mcp_template_json() -> Result<String> {
    let mut cfg = McpConfig::default();
    cfg.servers.insert(
        "example".to_string(),
        McpServerConfig {
            command: Some("node".to_string()),
            args: vec!["./path/to/your-mcp-server.js".to_string()],
            env: HashMap::new(),
            url: None,
            transport: None,
            headers: HashMap::new(),
            auth: None,
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: true,
            enabled: true,
            required: false,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
        },
    );
    serde_json::to_string_pretty(&cfg).context("Failed to render MCP template JSON")
}

pub fn init_config(path: &Path, force: bool) -> Result<McpWriteStatus> {
    if path.exists() && !force {
        return Ok(McpWriteStatus::SkippedExists);
    }
    let status = if path.exists() {
        McpWriteStatus::Overwritten
    } else {
        McpWriteStatus::Created
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create MCP config directory {}", parent.display())
        })?;
    }
    let template = mcp_template_json()?;
    write_atomic(path, template.as_bytes())
        .with_context(|| format!("Failed to write MCP config {}", path.display()))?;
    Ok(status)
}

pub fn add_server_config(
    path: &Path,
    name: String,
    command: Option<String>,
    url: Option<String>,
    args: Vec<String>,
) -> Result<()> {
    if command.is_none() && url.is_none() {
        anyhow::bail!("Provide either a command or URL for MCP server '{name}'.");
    }
    let mut cfg = load_config(path)?;
    cfg.servers.insert(
        name,
        McpServerConfig {
            command,
            args,
            env: HashMap::new(),
            url,
            transport: None,
            headers: HashMap::new(),
            auth: None,
            connect_timeout: None,
            execute_timeout: None,
            read_timeout: None,
            disabled: false,
            enabled: true,
            required: false,
            enabled_tools: Vec::new(),
            disabled_tools: Vec::new(),
        },
    );
    save_config(path, &cfg)
}

/// Load a single server block from `mcp.json` (for edit UIs).
pub fn get_server_entry(path: &Path, name: &str) -> Result<Option<McpServerConfig>> {
    let cfg = load_config(path)?;
    Ok(cfg.servers.get(name).cloned())
}

/// Remove a server entry and persist.
/// Returns `true` when removed, `false` when the name was already absent (idempotent).
pub fn remove_server_from_config(path: &Path, name: &str) -> Result<bool> {
    let mut cfg = load_config(path)?;
    if cfg.servers.remove(name).is_none() {
        return Ok(false);
    }
    save_config(path, &cfg)?;
    Ok(true)
}

/// Replace an existing server block (full document for that key). Errors if the name is missing.
pub fn replace_server_in_config(path: &Path, name: &str, server: McpServerConfig) -> Result<()> {
    if server.command.is_none() && server.url.is_none() {
        anyhow::bail!("MCP server '{name}': provide either `command` or `url`");
    }
    let mut cfg = load_config(path)?;
    let old = cfg
        .servers
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("MCP server '{name}' is not configured"))?;
    let mut server = server;
    merge_preserved_secrets(&mut server, &old);
    cfg.servers.insert(name.to_string(), server);
    save_config(path, &cfg)
}

/// Merge servers (and optionally `timeouts`) from a JSON fragment into `mcp.json`.
///
/// Supported shapes:
/// - Full config: `{ "mcpServers"|"servers": { "name": { … } }, "timeouts"?: … }`
/// - Server map: `{ "filesystem": { "command": "npx", "args": [] }, … }`
/// - Single server: `{ "name": "fs", "command": "…", "args": [] }` or `"url"`
///
/// Existing servers with the same name are replaced. Returns the number of server entries merged.
pub fn merge_mcp_json_fragment(path: &Path, fragment: &str) -> Result<usize> {
    let fragment = fragment.trim();
    if fragment.is_empty() {
        anyhow::bail!("JSON 不能为空");
    }
    let v: serde_json::Value =
        serde_json::from_str(fragment).context("无效的 JSON：请检查语法（逗号、引号、括号）")?;
    let obj = v.as_object().context("根节点必须是 JSON 对象 { … }")?;

    let mut cfg = load_config(path)?;
    let mut merged = 0usize;

    if obj.contains_key("mcpServers") || obj.contains_key("servers") {
        let partial: McpConfig = serde_json::from_value(v.clone())
            .context("无法解析为 MCP 配置（检查 mcpServers / servers 字段）")?;
        if obj.contains_key("timeouts") {
            cfg.timeouts = partial.timeouts;
        }
        if partial.servers.is_empty() && !obj.contains_key("timeouts") {
            anyhow::bail!("servers 为空：请至少包含一个服务器条目，或同时提供 timeouts");
        }
        for (name, sc) in partial.servers {
            cfg.servers.insert(name, sc);
            merged += 1;
        }
        save_config(path, &cfg)?;
        return Ok(merged);
    }

    if obj.contains_key("name") {
        let name = obj
            .get("name")
            .and_then(|x| x.as_str())
            .context("name 必须是字符串")?;
        if name.trim().is_empty() {
            anyhow::bail!("name 不能为空");
        }
        let mut inner = obj.clone();
        inner.remove("name");
        let server: McpServerConfig = serde_json::from_value(serde_json::Value::Object(inner))
            .context("服务器字段无效（可与 ~/.zagens/mcp.json 中条目对照）")?;
        if server.command.is_none() && server.url.is_none() {
            anyhow::bail!("必须提供 command 或 url");
        }
        cfg.servers.insert(name.trim().to_string(), server);
        merged = 1;
        save_config(path, &cfg)?;
        return Ok(merged);
    }

    let mut timeout_updated = false;
    let mut incoming: HashMap<String, McpServerConfig> = HashMap::new();
    for (key, val) in obj {
        if key == "timeouts" {
            cfg.timeouts = serde_json::from_value(val.clone()).context("timeouts 格式无效")?;
            timeout_updated = true;
            continue;
        }
        let server: McpServerConfig = serde_json::from_value(val.clone())
            .with_context(|| format!("服务器 \"{key}\" 的配置无效"))?;
        incoming.insert(key.clone(), server);
    }

    if incoming.is_empty() && !timeout_updated {
        anyhow::bail!(
            "未找到服务器条目。可粘贴完整 mcpServers 块，或形如 {{ \"myserver\": {{ \"command\": \"npx\", \"args\": [] }} }}"
        );
    }
    for (k, s) in incoming {
        cfg.servers.insert(k, s);
        merged += 1;
    }
    save_config(path, &cfg)?;
    Ok(merged)
}

pub fn remove_server_config(path: &Path, name: &str) -> Result<()> {
    let mut cfg = load_config(path)?;
    if cfg.servers.remove(name).is_none() {
        anyhow::bail!("MCP server '{name}' not found");
    }
    save_config(path, &cfg)
}

pub fn set_server_enabled(path: &Path, name: &str, enabled: bool) -> Result<()> {
    let mut cfg = load_config(path)?;
    let server = cfg
        .servers
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("MCP server '{name}' not found"))?;
    server.enabled = enabled;
    server.disabled = !enabled;
    save_config(path, &cfg)
}

pub fn manager_snapshot_from_config(
    path: &Path,
    restart_required: bool,
) -> Result<McpManagerSnapshot> {
    let cfg = load_config(path)?;
    Ok(snapshot_from_config(
        path,
        path.exists(),
        restart_required,
        &cfg,
        None,
    ))
}

pub async fn discover_manager_snapshot(
    path: &Path,
    network_policy: Option<NetworkPolicyDecider>,
    restart_required: bool,
) -> Result<McpManagerSnapshot> {
    let cfg = load_config(path)?;
    let mut pool = McpPool::new(cfg.clone());
    if let Some(policy) = network_policy {
        pool = pool.with_network_policy(policy);
    }
    let errors = pool
        .connect_all()
        .await
        .into_iter()
        .map(|(name, err)| (name, err.to_string()))
        .collect::<HashMap<_, _>>();
    Ok(snapshot_from_config(
        path,
        path.exists(),
        restart_required,
        &cfg,
        Some((&pool, &errors)),
    ))
}

/// Build a manager snapshot from a live pool (shared sidecar pool / hot-reload path).
pub async fn manager_snapshot_from_pool(path: &Path, pool: &mut McpPool) -> McpManagerSnapshot {
    let cfg = load_config(path).unwrap_or_default();
    let errors: HashMap<String, String> = pool
        .connect_all()
        .await
        .into_iter()
        .map(|(name, err)| (name, err.to_string()))
        .collect();
    snapshot_from_config(path, path.exists(), false, &cfg, Some((pool, &errors)))
}

fn snapshot_from_config(
    path: &Path,
    config_exists: bool,
    restart_required: bool,
    cfg: &McpConfig,
    discovery: Option<(&McpPool, &HashMap<String, String>)>,
) -> McpManagerSnapshot {
    let mut servers = cfg
        .servers
        .iter()
        .map(|(name, server)| {
            let transport = server
                .transport_kind()
                .map_or("unknown", McpTransportKind::as_str);
            let command_or_url = server.url.clone().unwrap_or_else(|| {
                let mut command = server
                    .command
                    .clone()
                    .unwrap_or_else(|| "(missing)".to_string());
                if !server.args.is_empty() {
                    command.push(' ');
                    command.push_str(&server.args.join(" "));
                }
                command
            });
            let mut snapshot = McpServerSnapshot {
                name: name.clone(),
                enabled: server.is_enabled(),
                required: server.required,
                transport: transport.to_string(),
                command_or_url,
                connect_timeout: server.effective_connect_timeout(&cfg.timeouts),
                execute_timeout: server.effective_execute_timeout(&cfg.timeouts),
                read_timeout: server.effective_read_timeout(&cfg.timeouts),
                connected: false,
                error: if server.is_enabled() {
                    None
                } else {
                    Some("disabled".to_string())
                },
                tools: Vec::new(),
                resources: Vec::new(),
                prompts: Vec::new(),
            };

            if let Some((pool, errors)) = discovery {
                if let Some(error) = errors.get(name) {
                    snapshot.error = Some(error.clone());
                }
                if let Some(conn) = pool.connections.get(name) {
                    snapshot.connected = conn.is_ready();
                    snapshot.tools = conn
                        .tools()
                        .iter()
                        .map(|tool| McpDiscoveredItem {
                            name: tool.name.clone(),
                            model_name: format!("mcp_{}_{}", name, tool.name),
                            description: tool.description.clone(),
                            enabled: conn.config().is_tool_enabled(&tool.name),
                        })
                        .collect();
                    snapshot.resources =
                        conn.resources()
                            .iter()
                            .map(|resource| McpDiscoveredItem {
                                name: resource.name.clone(),
                                model_name: format!(
                                    "mcp_{}_{}",
                                    name,
                                    resource.name.replace(' ', "_").to_lowercase()
                                ),
                                description: resource.description.clone(),
                                enabled: true,
                            })
                            .chain(conn.resource_templates().iter().map(|template| {
                                McpDiscoveredItem {
                                    name: template.name.clone(),
                                    model_name: format!(
                                        "mcp_{}_{}",
                                        name,
                                        template.name.replace(' ', "_").to_lowercase()
                                    ),
                                    description: template.description.clone(),
                                    enabled: true,
                                }
                            }))
                            .collect();
                    snapshot.prompts = conn
                        .prompts()
                        .iter()
                        .map(|prompt| McpDiscoveredItem {
                            name: prompt.name.clone(),
                            model_name: format!("mcp_{}_{}", name, prompt.name),
                            description: prompt.description.clone(),
                            enabled: true,
                        })
                        .collect();
                }
            }

            snapshot
        })
        .collect::<Vec<_>>();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    McpManagerSnapshot {
        config_path: path.to_path_buf(),
        config_exists,
        restart_required,
        servers,
    }
}
