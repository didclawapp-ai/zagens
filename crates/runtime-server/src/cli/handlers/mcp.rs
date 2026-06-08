use anyhow::{Result, bail};

use crate::cli::args::McpCommand;
use crate::cli::context::CliContext;
use crate::cli::setup::{WriteStatus, init_mcp_config};
use crate::mcp::McpPool;

pub async fn run_mcp(ctx: &CliContext, command: McpCommand) -> Result<()> {
    let config_path = ctx.config.mcp_config_path();
    match command {
        McpCommand::Init { force } => {
            let status = init_mcp_config(&config_path, force)?;
            match status {
                WriteStatus::Created => println!("Created MCP config at {}", config_path.display()),
                WriteStatus::Overwritten => {
                    println!("Overwrote MCP config at {}", config_path.display())
                }
                WriteStatus::SkippedExists => println!(
                    "MCP config already exists at {} (use --force to overwrite)",
                    config_path.display()
                ),
            }
            println!("Edit the file, then run `zagens mcp list` or `zagens mcp tools`.");
            Ok(())
        }
        McpCommand::List => {
            let cfg = crate::cli::mcp_config::load_mcp_config(&config_path)?;
            if cfg.servers.is_empty() {
                println!("No MCP servers configured in {}", config_path.display());
                return Ok(());
            }
            println!("MCP servers ({}):", cfg.servers.len());
            for (name, server) in cfg.servers {
                let status = if server.enabled && !server.disabled {
                    "enabled"
                } else {
                    "disabled"
                };
                let args = if server.args.is_empty() {
                    String::new()
                } else {
                    format!(" {}", server.args.join(" "))
                };
                let cmd_str = if let Some(cmd) = server.command {
                    format!("{cmd}{args}")
                } else if let Some(url) = server.url {
                    url
                } else {
                    "unknown".to_string()
                };
                let required = if server.required { " required" } else { "" };
                println!("  - {name} [{status}{required}] {cmd_str}");
            }
            Ok(())
        }
        McpCommand::Tools { server } => {
            let mut pool = McpPool::from_config_path(&config_path)?;
            if let Some(name) = server {
                let conn = pool.get_or_connect(&name).await?;
                if conn.tools().is_empty() {
                    println!("No tools found for MCP server: {name}");
                } else {
                    println!("Tools for {name}:");
                    for tool in conn.tools() {
                        let desc = tool
                            .description
                            .as_ref()
                            .map_or(String::new(), |d| format!(": {d}"));
                        println!("  - {}{desc}", tool.name);
                    }
                }
            } else {
                let _ = pool.connect_all().await;
                let tools = pool.all_tools();
                if tools.is_empty() {
                    println!("No MCP tools discovered.");
                } else {
                    println!("MCP tools:");
                    for (name, tool) in tools {
                        let desc = tool
                            .description
                            .as_ref()
                            .map_or(String::new(), |d| format!(": {d}"));
                        println!("  - {name}{desc}");
                    }
                }
            }
            Ok(())
        }
        other => bail!("This `mcp` subcommand is not implemented yet (Headless CLI v2): {other:?}"),
    }
}
