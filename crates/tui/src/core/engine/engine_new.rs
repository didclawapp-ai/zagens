//! Engine construction (`Engine::new`).

use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::{Mutex as AsyncMutex, RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use crate::client::DeepSeekClient;
use crate::config::{ApiProvider, Config};
use crate::mcp::McpPool;
use crate::prompts;
use crate::seam_manager::{SeamConfig, SeamManager};
use crate::tools::shell::{new_shared_shell_manager, SharedShellManager};
use crate::tools::subagent::{new_shared_subagent_manager, SharedSubAgentManager, SubAgentCompletion};
use crate::tui::app::AppMode;

use super::CapacityController;
use super::CoherenceState;
use super::Session;
use super::cycle_hooks;
use super::handle::EngineHandle;
use super::scratchpad_flow;
use super::types::EngineConfig;
use super::Engine;

fn env_only_api_key_recovery_hint(api_config: &Config) -> Option<String> {
    if !crate::config::active_provider_uses_env_only_api_key(api_config) {
        return None;
    }

    let provider = api_config.api_provider();
    let env_var = match provider {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => "DEEPSEEK_API_KEY",
        ApiProvider::NvidiaNim => "NVIDIA_API_KEY/NVIDIA_NIM_API_KEY",
        ApiProvider::Openrouter => "OPENROUTER_API_KEY",
        ApiProvider::Novita => "NOVITA_API_KEY",
        ApiProvider::Fireworks => "FIREWORKS_API_KEY",
        ApiProvider::Sglang => "SGLANG_API_KEY",
        ApiProvider::Vllm => "VLLM_API_KEY",
        ApiProvider::Ollama => "OLLAMA_API_KEY",
    };

    Some(format!(
        "The rejected key came from {env_var}; no saved config key is present.\n\
         Run `deepseek auth set --provider {provider}` to save a valid key in ~/.deepseek/config.toml, \
         or remove the stale export and open a fresh shell.",
        provider = provider.as_str()
    ))
}

impl Engine {
    /// Create a new engine with the given configuration
    pub fn new(config: EngineConfig, api_config: &Config) -> (Self, EngineHandle) {
        let (tx_op, rx_op) = mpsc::channel(32);
        let (tx_event, rx_event) = mpsc::channel(256);
        let (tx_approval, rx_approval) = mpsc::channel(64);
        let (tx_user_input, rx_user_input) = mpsc::channel(32);
        let (tx_steer, rx_steer) = mpsc::channel(64);
        let (tx_subagent_completion, rx_subagent_completion) = mpsc::unbounded_channel();
        let cancel_token = CancellationToken::new();
        let shared_cancel_token = Arc::new(StdMutex::new(cancel_token.clone()));
        let tool_exec_lock = Arc::new(RwLock::new(()));

        let (deepseek_client, deepseek_client_error) =
            if let Some(client) = config.llm_client_override.clone() {
                (Some(client), None)
            } else {
                match DeepSeekClient::new(api_config) {
                    Ok(client) => (Some(Arc::new(client) as Arc<dyn crate::llm_client::LlmClient>), None),
                    Err(err) => (None, Some(err.to_string())),
                }
            };
        let api_key_env_only_recovery = env_only_api_key_recovery_hint(api_config);

        let mut session = Session::new(
            config.model.clone(),
            config.workspace.clone(),
            config.allow_shell,
            config.trust_mode,
            config.notes_path.clone(),
            config.mcp_config_path.clone(),
        );
        let user_memory_block =
            crate::memory::compose_block(config.memory_enabled, &config.memory_path);
        let system_prompt =
            prompts::system_prompt_for_mode_with_context_skills_session_and_approval(
                AppMode::Agent,
                &config.workspace,
                None,
                Some(&config.skills_dir),
                Some(&config.instructions),
                prompts::PromptSessionContext {
                    user_memory_block: user_memory_block.as_deref(),
                    goal_objective: config.goal_objective.as_deref(),
                    locale_tag: &config.locale_tag,
                    task_type: config.task_type,
                },
                session.approval_mode,
            );
        let stable_prompt = Some(system_prompt);
        session.last_system_prompt_hash =
            Some(cycle_hooks::system_prompt_hash(stable_prompt.as_ref()));
        session.system_prompt = stable_prompt;

        let subagent_manager =
            new_shared_subagent_manager(config.workspace.clone(), config.max_subagents);
        let shell_manager = config
            .runtime_services
            .shell_manager
            .clone()
            .unwrap_or_else(|| new_shared_shell_manager(config.workspace.clone()));
        let capacity_controller = CapacityController::new(config.capacity.clone());

        let seam_manager = deepseek_client.as_ref().map(|main_client| {
            let seam_config = SeamConfig {
                enabled: api_config.context.enabled.unwrap_or(false),
                verbatim_window_turns: api_config
                    .context
                    .verbatim_window_turns
                    .unwrap_or(crate::seam_manager::VERBATIM_WINDOW_TURNS),
                l1_threshold: api_config
                    .context
                    .l1_threshold
                    .unwrap_or(crate::seam_manager::DEFAULT_L1_THRESHOLD),
                l2_threshold: api_config
                    .context
                    .l2_threshold
                    .unwrap_or(crate::seam_manager::DEFAULT_L2_THRESHOLD),
                l3_threshold: api_config
                    .context
                    .l3_threshold
                    .unwrap_or(crate::seam_manager::DEFAULT_L3_THRESHOLD),
                cycle_threshold: api_config
                    .context
                    .cycle_threshold
                    .unwrap_or(crate::seam_manager::DEFAULT_CYCLE_THRESHOLD),
                seam_model: api_config
                    .context
                    .seam_model
                    .clone()
                    .unwrap_or_else(|| crate::seam_manager::DEFAULT_SEAM_MODEL.to_string()),
            };
            SeamManager::new(main_client.clone(), seam_config)
        });

        let lsp_manager = Arc::new(match config.lsp_config.clone() {
            Some(cfg) => crate::lsp::LspManager::new(cfg, config.workspace.clone()),
            None => crate::lsp::LspManager::disabled(),
        });

        let workshop_vars: Option<
            std::sync::Arc<tokio::sync::Mutex<crate::tools::large_output_router::WorkshopVariables>>,
        > = if config.workshop.is_some() {
            Some(std::sync::Arc::new(tokio::sync::Mutex::new(
                crate::tools::large_output_router::WorkshopVariables::default(),
            )))
        } else {
            None
        };

        let sandbox_backend = crate::sandbox::backend::create_backend(api_config)
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to create sandbox backend: {e}");
                None
            })
            .map(std::sync::Arc::from);

        let scratchpad_run_id = config
            .runtime_services
            .scratchpad_run_id
            .lock()
            .ok()
            .and_then(|g| g.clone());

        let mut engine = Engine {
            config,
            deepseek_client,
            deepseek_client_error,
            api_key_env_only_recovery,
            session,
            subagent_manager,
            shell_manager,
            mcp_pool: None,
            rx_op,
            tx_approval: tx_approval.clone(),
            rx_approval,
            rx_user_input,
            rx_steer,
            tx_event,
            tx_subagent_completion,
            rx_subagent_completion,
            cancel_token: cancel_token.clone(),
            shared_cancel_token: shared_cancel_token.clone(),
            tool_exec_lock,
            capacity_controller,
            seam_manager,
            coherence_state: CoherenceState::default(),
            turn_counter: 0,
            lsp_manager,
            pending_lsp_blocks: Vec::new(),
            workshop_vars,
            sandbox_backend,
            scratchpad_step: scratchpad_flow::ScratchpadStepState::default(),
            scratchpad_run_id,
            scratchpad_summary_injected_this_turn: false,
        };
        engine.rehydrate_latest_canonical_state();

        let handle = EngineHandle {
            tx_op,
            rx_event: Arc::new(RwLock::new(rx_event)),
            cancel_token: shared_cancel_token,
            tx_approval,
            tx_user_input,
            tx_steer,
        };

        (engine, handle)
    }
}
