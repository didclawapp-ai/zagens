//! Process-in Engine one-shot runner for `zagens exec` (shared with future TUI).

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Result, bail};
use zagens_core::approval::ApprovalMode;
use zagens_core::chat::LlmClient;

use crate::agent_surface::AppMode;
use crate::cli::auto_route_cli::resolve_cli_auto_route;
use crate::cli::context::CliContext;
use crate::compaction::CompactionConfig;
use crate::config::{Config, MAX_SUBAGENTS};
use crate::core::engine::turn_loop::host_impl::app_mode_to_turn_loop;
use crate::core::engine::{EngineConfig, spawn_engine};
use crate::core::events::Event;
use crate::core::events::TurnOutcomeStatus;
use crate::core::ops::Op;
use crate::models::compaction_threshold_for_model;
use crate::models::{ContentBlock, Message, MessageRequest, SystemPrompt};
use crate::tools::plan::new_shared_plan_state;
use crate::tools::todo::new_shared_todo_list;

pub struct ExecOptions {
    pub prompt: String,
    pub model: Option<String>,
    pub auto_mode: bool,
    pub json_output: bool,
    pub max_subagents: Option<usize>,
}

pub async fn run_exec(ctx: &CliContext, opts: ExecOptions) -> Result<()> {
    let model = opts
        .model
        .or_else(|| ctx.config.default_text_model.clone())
        .unwrap_or_else(|| ctx.config.default_model());

    if opts.auto_mode {
        let max_subagents = opts.max_subagents.map_or_else(
            || ctx.config.max_subagents(),
            |value| value.clamp(1, MAX_SUBAGENTS),
        );
        run_exec_agent(
            &ctx.config,
            &model,
            &opts.prompt,
            ctx.workspace.clone(),
            max_subagents,
            ExecAgentRunOptions {
                auto_approve: true,
                trust_mode: true,
                json_output: opts.json_output,
                llm_client_override: None,
            },
        )
        .await
    } else if opts.json_output {
        run_one_shot_json(&ctx.config, &model, &opts.prompt).await
    } else {
        run_one_shot(&ctx.config, &model, &opts.prompt).await
    }
}

async fn run_one_shot(config: &Config, model: &str, prompt: &str) -> Result<()> {
    let client = crate::client::DeepSeekClient::new(config)?;
    let route = resolve_cli_auto_route(config, model, prompt).await;
    let reasoning_effort = route
        .reasoning_effort
        .map(|effort| effort.as_setting().to_string());

    let request = MessageRequest {
        model: route.model,
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
                cache_control: None,
            }],
        }],
        max_tokens: 4096,
        system: None,
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort,
        stream: Some(false),
        temperature: None,
        top_p: None,
    };

    let response = client.create_message(request).await?;
    for block in response.content {
        if let ContentBlock::Text { text, .. } = block {
            println!("{text}");
        }
    }
    Ok(())
}

async fn run_one_shot_json(config: &Config, model: &str, prompt: &str) -> Result<()> {
    let client = crate::client::DeepSeekClient::new(config)?;
    let route = resolve_cli_auto_route(config, model, prompt).await;
    let model = route.model;
    let reasoning_effort = route
        .reasoning_effort
        .map(|effort| effort.as_setting().to_string());
    let request = MessageRequest {
        model: model.clone(),
        messages: vec![Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: prompt.to_string(),
                cache_control: None,
            }],
        }],
        max_tokens: 4096,
        system: Some(SystemPrompt::Text(
            "You are a coding assistant. Give concise, actionable responses.".to_string(),
        )),
        tools: None,
        tool_choice: None,
        metadata: None,
        thinking: None,
        reasoning_effort,
        stream: Some(false),
        temperature: Some(0.2),
        top_p: Some(0.9),
    };

    let response = client.create_message(request).await?;
    let mut output = String::new();
    for block in response.content {
        if let ContentBlock::Text { text, .. } = block {
            output.push_str(&text);
        }
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "mode": "one-shot",
            "model": model,
            "success": true,
            "content": output
        }))?
    );
    Ok(())
}

struct ExecAgentRunOptions {
    auto_approve: bool,
    trust_mode: bool,
    json_output: bool,
    llm_client_override: Option<std::sync::Arc<dyn LlmClient>>,
}

async fn run_exec_agent(
    config: &Config,
    model: &str,
    prompt: &str,
    workspace: PathBuf,
    max_subagents: usize,
    run: ExecAgentRunOptions,
) -> Result<()> {
    let ExecAgentRunOptions {
        auto_approve,
        trust_mode,
        json_output,
        llm_client_override,
    } = run;
    let route = resolve_cli_auto_route(config, model, prompt).await;
    let auto_model = route.auto_model;
    let effective_model = route.model;
    let effective_reasoning_effort = route
        .reasoning_effort
        .map(|effort| effort.as_setting().to_string());

    let compaction = CompactionConfig {
        enabled: false,
        model: effective_model.clone(),
        token_threshold: compaction_threshold_for_model(&effective_model),
        ..Default::default()
    };

    let network_policy = config.network.clone().map(|toml_cfg| {
        crate::network_policy::NetworkPolicyDecider::with_default_audit(toml_cfg.into_runtime())
    });
    let lsp_config = config
        .lsp
        .clone()
        .map(crate::config::LspConfigToml::into_runtime);
    let search = config.search_config();

    let engine_config = EngineConfig {
        model: effective_model.clone(),
        workspace: workspace.clone(),
        allow_shell: auto_approve || config.allow_shell(),
        sandbox_mode: config.sandbox_mode.clone(),
        trust_mode,
        notes_path: config.notes_path(),
        mcp_config_path: config.mcp_config_path(),
        skills_dir: config.skills_dir(),
        instructions: crate::prompts::merge_instruction_paths_with_pick_rules(
            &workspace,
            config.instructions_paths(&workspace),
        ),
        max_steps: 100,
        max_subagents,
        subagent_step_timeout: config.subagent_step_timeout(),
        features: config.features(),
        compaction,
        cycle: config.cycle_runtime_config(&effective_model),
        capacity: crate::core::capacity::capacity_config_from_app(config),
        todos: new_shared_todo_list(),
        plan_state: new_shared_plan_state(),
        max_spawn_depth: crate::tools::subagent::DEFAULT_MAX_SPAWN_DEPTH,
        network_policy,
        snapshots_enabled: config.snapshots_config().enabled,
        snapshots_max_workspace_gb: config.snapshots_config().max_workspace_gb,
        lsp_config,
        runtime_services: crate::tools::spec::RuntimeToolServices::default(),
        subagent_model_overrides: config.subagent_model_overrides(),
        memory_enabled: config.memory_enabled(),
        memory_path: config.memory_path(),
        topic_memory: crate::topic_memory::settings_from_config(config),
        strict_tool_mode: config.strict_tool_mode.unwrap_or(false),
        goal_objective: None,
        locale_tag: crate::localization::resolve_locale(
            &crate::settings::Settings::load().unwrap_or_default().locale,
        )
        .tag()
        .to_string(),
        task_type: crate::task_type::TaskType::Code,
        workshop: config.workshop.clone(),
        scratchpad: config.scratchpad_config(),
        long_horizon: config.long_horizon_config(),
        llm_client_override,
        search_provider: search.provider.unwrap_or_default(),
        search_api_key: search.api_key,
        worktrees: config.worktrees_config().runtime_config(),
        session_manager: None,
    };

    let engine_handle = spawn_engine(engine_config, config);
    let mode = if auto_approve {
        AppMode::Yolo
    } else {
        AppMode::Agent
    };

    engine_handle
        .send(Op::SendMessage {
            turn_id: None,
            content: prompt.to_string(),
            mode: app_mode_to_turn_loop(mode),
            model: effective_model.clone(),
            goal_objective: None,
            reasoning_effort: effective_reasoning_effort,
            reasoning_effort_auto: auto_model,
            auto_model,
            allow_shell: auto_approve || config.allow_shell(),
            trust_mode,
            auto_approve,
            approval_mode: if auto_approve {
                ApprovalMode::Auto
            } else {
                config
                    .approval_policy
                    .as_deref()
                    .and_then(ApprovalMode::from_config_value)
                    .unwrap_or_default()
            },
            temperature: None,
            top_p: None,
            max_output_tokens: None,
        })
        .await?;

    #[derive(serde::Serialize)]
    struct ExecToolEntry {
        name: String,
        success: bool,
        output: String,
    }
    #[derive(serde::Serialize, Default)]
    struct ExecSummary {
        mode: String,
        model: String,
        prompt: String,
        output: String,
        tools: Vec<ExecToolEntry>,
        status: Option<String>,
        error: Option<String>,
    }

    let mut summary = ExecSummary {
        mode: "agent".to_string(),
        model: effective_model,
        prompt: prompt.to_string(),
        ..ExecSummary::default()
    };

    let mut stdout = io::stdout();
    let mut ends_with_newline = false;
    let mut failed = false;

    loop {
        let event = {
            let mut rx = engine_handle.rx_event.write().await;
            rx.recv().await
        };

        let Some(event) = event else {
            break;
        };

        match event {
            Event::MessageDelta { content, .. } => {
                summary.output.push_str(&content);
                if !json_output {
                    print!("{content}");
                    stdout.flush()?;
                }
                ends_with_newline = content.ends_with('\n');
            }
            Event::MessageComplete { .. } if !json_output && !ends_with_newline => {
                println!();
            }
            Event::ToolCallStarted { name, .. } if !json_output => {
                eprintln!("tool: {name}");
            }
            Event::ToolCallComplete { name, result, .. } => match result {
                Ok(output) => {
                    summary.tools.push(ExecToolEntry {
                        name: name.clone(),
                        success: output.success,
                        output: truncate_for_log(&output.content, 500),
                    });
                    if !json_output {
                        eprintln!("tool {name} completed");
                    }
                }
                Err(err) => {
                    summary.tools.push(ExecToolEntry {
                        name: name.clone(),
                        success: false,
                        output: err.to_string(),
                    });
                    if !json_output {
                        eprintln!("tool {name} failed: {err}");
                    }
                }
            },
            Event::ApprovalRequired { id, tool_name, .. } => {
                if auto_approve {
                    let _ = engine_handle.approve_tool_call(id).await;
                } else {
                    failed = true;
                    if !json_output {
                        eprintln!(
                            "approval required for `{tool_name}` — re-run with `--auto` to allow tools"
                        );
                    }
                    let _ = engine_handle.deny_tool_call(id).await;
                }
            }
            Event::UserInputRequired { id, .. } => {
                failed = true;
                if !json_output {
                    eprintln!("interactive user input requested — not supported in headless mode");
                }
                let _ = engine_handle.cancel_user_input(id).await;
            }
            Event::ElevationRequired {
                tool_id,
                tool_name,
                denial_reason,
                ..
            } => {
                if auto_approve {
                    eprintln!("sandbox denied {tool_name}: {denial_reason} (auto-elevating)");
                    let policy = crate::sandbox::SandboxPolicy::DangerFullAccess;
                    let _ = engine_handle.retry_tool_with_policy(tool_id, policy).await;
                } else {
                    failed = true;
                    eprintln!("sandbox denied {tool_name}: {denial_reason}");
                    let _ = engine_handle.deny_tool_call(tool_id).await;
                }
            }
            Event::Error { envelope, .. } => {
                failed = true;
                summary.error = Some(envelope.message.clone());
                if !json_output {
                    eprintln!("error: {}", envelope.message);
                }
            }
            Event::TurnComplete { status, error, .. } => {
                summary.status = Some(format!("{status:?}").to_lowercase());
                summary.error = error.clone();
                if matches!(status, TurnOutcomeStatus::Failed) || error.is_some() {
                    failed = true;
                }
                let _ = engine_handle.send(Op::Shutdown).await;
                break;
            }
            _ => {}
        }
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    }

    if failed {
        bail!("exec finished with errors");
    }
    Ok(())
}

fn truncate_for_log(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::llm_client::mock::{MockLlmClient, canned};
    use crate::models::Usage;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn exec_agent_json_summary_has_stable_top_level_keys() {
        let summary = serde_json::json!({
            "mode": "agent",
            "model": "deepseek-v4-pro",
            "prompt": "hello",
            "output": "world",
            "tools": [],
            "status": "completed",
            "error": null
        });
        for key in [
            "mode", "model", "prompt", "output", "tools", "status", "error",
        ] {
            assert!(
                summary.get(key).is_some(),
                "exec --json summary missing key: {key}"
            );
        }
    }

    #[tokio::test]
    async fn exec_agent_json_e2e_with_mock_llm() {
        let tmp = tempdir().expect("tempdir");
        let workspace = tmp.path().to_path_buf();
        let config = Config::default();
        let turn = vec![
            canned::message_start("msg_1"),
            canned::text_block_start(0),
            canned::text_delta(0, "mock-cli-agent-reply"),
            canned::block_stop(0),
            canned::message_delta("end_turn", Some(Usage::default())),
            canned::message_stop(),
        ];
        let mock = Arc::new(MockLlmClient::new(vec![turn]).with_model("deepseek-v4-pro"));

        run_exec_agent(
            &config,
            "deepseek-v4-pro",
            "hello mock",
            workspace,
            1,
            ExecAgentRunOptions {
                auto_approve: true,
                trust_mode: true,
                json_output: true,
                llm_client_override: Some(mock.clone()),
            },
        )
        .await
        .expect("exec agent with mock LLM");

        assert_eq!(mock.call_count(), 1, "mock should receive one stream call");
    }
}
