use std::path::Path;

use anyhow::Result;
use colored::Colorize;
use serde_json::json;

use crate::cli::args::DoctorArgs;
use crate::cli::context::{CliContext, default_config_path, display_path};
use crate::cli::doctor::{
    McpServerDoctorStatus, doctor_api_target, doctor_check_mcp_server,
    doctor_timeout_recovery_lines,
};
use crate::cli::doctor_context::{build_context_report, print_context_human};
use crate::cli::doctor_tools::{
    build_tool_telemetry_report, default_sessions_db_path, print_tool_telemetry_human,
};
use crate::cli::mcp_config::load_mcp_config;
use crate::cli::setup::{
    ApiKeySource, count_dir_entries, default_plugins_dir, default_tools_dir,
    resolve_api_key_source, skills_count_for,
};
use crate::config::{Config, provider_capability};

pub async fn run(ctx: &CliContext, cli_config: Option<&Path>, args: DoctorArgs) -> Result<()> {
    if args.tools {
        return run_tools(&args);
    }
    if args.json {
        return run_json(&ctx.config, &ctx.workspace, cli_config);
    }
    run_human(&ctx.config, &ctx.workspace, cli_config).await;
    Ok(())
}

fn run_tools(args: &DoctorArgs) -> Result<()> {
    let db_path = default_sessions_db_path();
    let report = build_tool_telemetry_report(&db_path)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_tool_telemetry_human(&report);
    }
    Ok(())
}

async fn run_human(config: &Config, workspace: &Path, config_path_override: Option<&Path>) {
    println!("{}", "Zagens Doctor".bold());
    println!();
    println!("Version: zagens {}", env!("CARGO_PKG_VERSION"));
    println!("Rust: {}", rustc_version());
    println!();

    let config_path = config_path_override
        .map(Path::to_path_buf)
        .unwrap_or_else(default_config_path);
    if config_path.exists() {
        println!("✓ config: {}", display_path(&config_path));
    } else {
        println!(
            "! config missing at {} (using defaults/env)",
            display_path(&config_path)
        );
    }
    println!("  workspace: {}", display_path(workspace));

    println!();
    println!("{}", "API".bold());
    let target = doctor_api_target(config);
    println!("  provider: {}", target.provider);
    println!("  base_url: {}", target.base_url);
    println!("  model: {}", target.model);
    match resolve_api_key_source(config) {
        ApiKeySource::Missing => println!("  api_key: {}", "missing".red()),
        ApiKeySource::Env => println!("  api_key: set (env)"),
        ApiKeySource::Config => println!("  api_key: set (config)"),
        ApiKeySource::Keyring => println!("  api_key: set (keyring)"),
    }

    print_context_human(config, workspace);

    println!();
    println!("{}", "MCP".bold());
    let mcp_path = config.mcp_config_path();
    match load_mcp_config(&mcp_path) {
        Ok(cfg) if cfg.servers.is_empty() => {
            println!("  No servers in {}", mcp_path.display());
        }
        Ok(cfg) => {
            for (name, server) in cfg.servers {
                let status = doctor_check_mcp_server(&server);
                let label = match status {
                    McpServerDoctorStatus::Ok(d) => format!("ok — {d}"),
                    McpServerDoctorStatus::Warning(d) => format!("warn — {d}"),
                    McpServerDoctorStatus::Error(d) => format!("error — {d}"),
                };
                println!("  - {name}: {label}");
            }
        }
        Err(err) => println!("  Failed to read MCP config: {err}"),
    }

    println!();
    println!("{}", "Connectivity".bold());
    match test_api_connectivity(config).await {
        Ok(model) => println!("  ✓ API reachable (model probe: {model})"),
        Err(err) => {
            println!("  ✗ API check failed: {err}");
            for line in doctor_timeout_recovery_lines(config) {
                println!("    {line}");
            }
        }
    }
}

fn run_json(config: &Config, workspace: &Path, config_path_override: Option<&Path>) -> Result<()> {
    let config_path = config_path_override
        .map(Path::to_path_buf)
        .unwrap_or_else(default_config_path);

    let api_key_state = match resolve_api_key_source(config) {
        ApiKeySource::Env => "env",
        ApiKeySource::Config => "config",
        ApiKeySource::Keyring => "keyring",
        ApiKeySource::Missing => "missing",
    };

    let mcp_config_path = config.mcp_config_path();
    let mcp_summary = match load_mcp_config(&mcp_config_path) {
        Ok(cfg) => {
            let servers: Vec<serde_json::Value> = cfg
                .servers
                .iter()
                .map(|(name, server)| {
                    let status = doctor_check_mcp_server(server);
                    let (kind, detail) = match status {
                        McpServerDoctorStatus::Ok(d) => ("ok", d),
                        McpServerDoctorStatus::Warning(d) => ("warning", d),
                        McpServerDoctorStatus::Error(d) => ("error", d),
                    };
                    json!({
                        "name": name,
                        "enabled": server.enabled && !server.disabled,
                        "status": kind,
                        "detail": detail,
                    })
                })
                .collect();
            json!({
                "config_path": mcp_config_path.display().to_string(),
                "present": mcp_config_path.exists(),
                "servers": servers,
            })
        }
        Err(err) => json!({
            "config_path": mcp_config_path.display().to_string(),
            "present": mcp_config_path.exists(),
            "servers": [],
            "error": err.to_string(),
        }),
    };

    let target = doctor_api_target(config);
    let tools_dir = default_tools_dir();
    let plugins_dir = default_plugins_dir();
    let skills_dir = config.skills_dir();

    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "workspace": workspace.display().to_string(),
        "config_path": config_path.display().to_string(),
        "config_present": config_path.exists(),
        "api_key": api_key_state,
        "provider": target.provider,
        "base_url": target.base_url,
        "default_text_model": target.model,
        "mcp": mcp_summary,
        "skills": {
            "path": skills_dir.display().to_string(),
            "count": skills_count_for(&skills_dir),
        },
        "tools": {
            "path": tools_dir.display().to_string(),
            "count": if tools_dir.exists() { count_dir_entries(&tools_dir) } else { 0 },
        },
        "plugins": {
            "path": plugins_dir.display().to_string(),
            "count": if plugins_dir.exists() { count_dir_entries(&plugins_dir) } else { 0 },
        },
        "api_connectivity": {
            "checked": false,
            "note": "Skipped in --json mode; run `zagens doctor` for a live check.",
        },
        "capability": provider_capability_report(config),
        "context": build_context_report(config, workspace),
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn provider_capability_report(config: &Config) -> serde_json::Value {
    let provider = config.api_provider();
    let model = config.default_model();
    let cap = provider_capability(provider, &model);
    json!({
        "resolved_provider": provider.as_str(),
        "resolved_model": cap.resolved_model,
        "context_window": cap.context_window,
        "max_output": cap.max_output,
        "thinking_supported": cap.thinking_supported,
        "cache_telemetry_supported": cap.cache_telemetry_supported,
        "request_payload_mode": serde_json::to_value(cap.request_payload_mode).unwrap_or_default(),
    })
}

async fn test_api_connectivity(config: &Config) -> Result<String> {
    use crate::client::DeepSeekClient;
    use crate::models::{ContentBlock, Message, MessageRequest};
    use zagens_core::chat::LlmClient;

    let client = DeepSeekClient::new(config)?;
    let model = client.model().to_string();
    let request = MessageRequest {
        model: model.clone(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
                cache_control: None,
            }],
        }],
        max_tokens: 1,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort: None,
        stream: Some(false),
        temperature: None,
        top_p: None,
    };

    match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        client.create_message(request),
    )
    .await
    {
        Ok(Ok(_)) => Ok(model),
        Ok(Err(e)) => Err(e),
        Err(_) => anyhow::bail!("Request timeout after 15 seconds"),
    }
}

fn rustc_version() -> String {
    std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
}
