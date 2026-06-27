//! Doctor helpers retained for unit tests after CLI/TUI sunset (D6 Phase B).

use std::path::Path;

use crate::config::Config;
use crate::mcp::McpServerConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DoctorApiTarget {
    pub(crate) provider: &'static str,
    pub(crate) base_url: String,
    pub(crate) model: String,
}

pub(crate) fn doctor_api_target(config: &Config) -> DoctorApiTarget {
    let provider = config.api_provider();
    DoctorApiTarget {
        provider: provider.as_str(),
        base_url: config.deepseek_base_url(),
        model: config.default_model(),
    }
}

pub(crate) fn doctor_timeout_recovery_lines(config: &Config) -> Vec<String> {
    let target = doctor_api_target(config);
    let mut lines = vec![format!(
        "Connection timed out while reaching {}.",
        target.base_url
    )];

    match config.api_provider() {
        crate::config::ApiProvider::Deepseek
            if target.base_url.contains("api.deepseek.com")
                && !target.base_url.contains("api.deepseeki.com") =>
        {
            lines.push(
                "If you are in mainland China, set `provider = \"deepseek-cn\"` or `base_url = \"https://api.deepseeki.com\"` in ~/.zagens/config.toml, then rerun `zagens doctor`."
                    .to_string(),
            );
        }
        crate::config::ApiProvider::Deepseek | crate::config::ApiProvider::DeepseekCN => {
            lines.push(
                "If this is a custom DeepSeek-compatible endpoint, confirm it serves `/v1/models` and `/v1/chat/completions` over HTTPS."
                    .to_string(),
            );
        }
        _ => {
            lines.push(
                "Confirm the configured provider endpoint is reachable and OpenAI-compatible for `/v1/models` and `/v1/chat/completions`."
                    .to_string(),
            );
        }
    }

    lines.push(
        "Run `deepseek doctor --json` and include `base_url`, `default_text_model`, and `api_connectivity` when filing an issue."
            .to_string(),
    );
    lines
}

#[derive(Debug)]
pub(crate) enum McpServerDoctorStatus {
    Ok(String),
    Warning(String),
    Error(String),
}

pub(crate) fn doctor_check_mcp_server(server: &McpServerConfig) -> McpServerDoctorStatus {
    if server.command.is_none() && server.url.is_none() {
        return McpServerDoctorStatus::Error("no command or url configured".to_string());
    }

    if let Some(ref url) = server.url {
        return McpServerDoctorStatus::Ok(format!("HTTP/SSE server at {url}"));
    }

    let cmd = server.command.as_deref().unwrap_or("");
    if cmd.is_empty() {
        return McpServerDoctorStatus::Error("empty command".to_string());
    }

    let cmd_path = Path::new(cmd);
    let is_absolute = cmd_path.is_absolute() || cmd.starts_with('/');

    if is_absolute && !cmd_path.exists() {
        return McpServerDoctorStatus::Error(format!("command not found: {cmd}"));
    }

    let is_self_hosted = server
        .args
        .windows(2)
        .any(|w| w[0] == "serve" && w[1] == "--mcp");

    let args_str = server.args.join(" ");
    if is_self_hosted {
        if is_absolute {
            McpServerDoctorStatus::Ok(format!("self-hosted MCP server ({cmd} {args_str})"))
        } else {
            McpServerDoctorStatus::Warning(format!(
                "self-hosted MCP server uses relative command \"{cmd}\" — consider using an absolute path"
            ))
        }
    } else {
        McpServerDoctorStatus::Ok(format!(
            "stdio server ({cmd}{})",
            if args_str.is_empty() {
                String::new()
            } else {
                format!(" {args_str}")
            }
        ))
    }
}
