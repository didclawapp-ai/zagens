//! Runtime-side engine builder — wires concrete subsystems into core `Engine::with_hosts`.

use std::sync::Arc;

use tokio::sync::{Mutex as AsyncMutex, mpsc};
use zagens_core::engine::EngineHostBundle;

use crate::agent_surface::AppMode;
use crate::client::DeepSeekClient;
use crate::config::{ApiProvider, Config};
use crate::config::{resolve_windows_sandbox_mode, resolve_windows_sandbox_private_desktop};
use crate::prompts;
use crate::sandbox::TuiSandboxHost;
use crate::seam_manager::{SeamConfig, SeamManager};
use crate::tools::approval_cache::ApprovalCache;
use crate::tools::host_impl::HookShellEnvHost;
use crate::tools::large_output_router::TuiWorkshopHost;
use crate::tools::shell::{TuiShellHost, new_shared_shell_manager};
use crate::tools::subagent::{new_shared_subagent_manager, spawn_subagent_maintenance_task};

use super::Engine;
use super::cycle_hooks;
use super::handle::EngineHandle;
use super::kernel_effect_shadow::KernelEffectShadow;
use super::kernel_guard_shadow::KernelGuardShadow;
use super::kernel_memory_shadow::KernelMemoryShadow;
use super::kernel_projection_shadow::KernelProjectionShadow;
use super::kernel_replay_shadow::KernelReplayShadow;
use super::runtime_ext::EngineRuntimeExt;
use super::types::EngineConfig;
use crate::core::capacity::CapacityController;
use crate::core::engine::kernel_message_coverage_shadow::{
    KernelMessageCoverageShadowStats, register_global_message_coverage_shadow_stats,
};
use crate::core::engine::kernel_message_timeline_shadow::{
    KernelMessageTimelineShadowStats, register_global_message_timeline_shadow_stats,
};
use crate::core::session::Session;
use crate::hooks::HookExecutor;
use crate::long_horizon::LongHorizonSessionState;
use zagens_runtime_adapters::tools::ToolShellEnvHost;

fn env_only_api_key_recovery_hint(api_config: &Config) -> Option<String> {
    if !crate::config::active_provider_uses_env_only_api_key(api_config) {
        return None;
    }

    let provider = api_config.api_provider();
    let env_var = match provider {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => "DEEPSEEK_API_KEY",
        ApiProvider::NvidiaNim => "NVIDIA_API_KEY/NVIDIA_NIM_API_KEY",
        ApiProvider::Openai => "OPENAI_API_KEY",
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

/// Build a core [`Engine`] plus handle from tui configuration.
pub fn build_engine(config: EngineConfig, api_config: &Config) -> (Engine, EngineHandle) {
    let mut config_ext = config.ext();
    let lean = config.lean();

    let (deepseek_client, deepseek_client_error) =
        if let Some(client) = config_ext.llm_client_override.clone() {
            (Some(client), None)
        } else {
            match DeepSeekClient::new(api_config) {
                Ok(client) => (
                    Some(Arc::new(client) as Arc<dyn crate::llm_client::LlmClient>),
                    None,
                ),
                Err(err) => (None, Some(err.to_string())),
            }
        };
    let api_key_env_only_recovery = env_only_api_key_recovery_hint(api_config);

    let mut session = Session::new(
        lean.model.clone(),
        lean.workspace.clone(),
        lean.allow_shell,
        lean.trust_mode,
        lean.notes_path.clone(),
        lean.mcp_config_path.clone(),
    );
    let user_memory_block = crate::memory::compose_block(lean.memory_enabled, &lean.memory_path);
    let system_prompt = prompts::system_prompt_for_mode_with_context_skills_session_and_approval(
        AppMode::Agent,
        &lean.workspace,
        None,
        Some(&lean.skills_dir),
        Some(&lean.instructions),
        prompts::PromptSessionContext {
            user_memory_block: user_memory_block.as_deref(),
            topic_memory_block: None,
            goal_objective: lean.goal_objective.as_deref(),
            locale_tag: &lean.locale_tag,
            task_type: lean.task_type,
        },
        session.approval_mode,
    );
    let stable_prompt = Some(system_prompt);
    session.last_system_prompt_hash = Some(cycle_hooks::system_prompt_hash(stable_prompt.as_ref()));
    session.system_prompt = stable_prompt;

    let subagent_manager = new_shared_subagent_manager(
        lean.workspace.clone(),
        lean.max_subagents,
        api_config.subagent_heartbeat_timeout(),
    );
    spawn_subagent_maintenance_task(subagent_manager.clone());
    let shell_manager = config_ext
        .runtime_services
        .shell_manager
        .clone()
        .unwrap_or_else(|| new_shared_shell_manager(lean.workspace.clone()));
    if let Ok(mut guard) = shell_manager.lock() {
        guard.set_windows_sandbox_mode(resolve_windows_sandbox_mode(api_config));
        guard.set_windows_private_desktop(resolve_windows_sandbox_private_desktop(api_config));
        guard.set_prefer_bwrap(api_config.prefer_bwrap.unwrap_or(false));
    }
    let capacity_controller = CapacityController::new(lean.capacity.clone());

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

    let lsp_manager = Arc::new(match config_ext.lsp_config.clone() {
        Some(cfg) => crate::lsp::LspManager::new(cfg, lean.workspace.clone()),
        None => crate::lsp::LspManager::disabled(),
    });

    let workshop_vars = if config_ext.workshop.is_some() {
        Some(Arc::new(AsyncMutex::new(
            crate::tools::large_output_router::WorkshopVariables::default(),
        )))
    } else {
        None
    };

    let sandbox_init = crate::sandbox::backend::init_backend(api_config);
    let sandbox_backend = sandbox_init.backend.map(Arc::from);
    let sandbox_init_warning = sandbox_init.user_warning;

    let scratchpad_run_id = config_ext
        .runtime_services
        .wire
        .scratchpad_run_id
        .lock()
        .ok()
        .and_then(|g| g.clone());

    let topic_memory_settings = config_ext.topic_memory.clone();
    let topic_memory_runtime = crate::topic_memory::TopicMemoryRuntime::new(topic_memory_settings);

    let (tx_subagent_completion, rx_subagent_completion) = mpsc::unbounded_channel();
    let rx_subagent_completion = Arc::new(AsyncMutex::new(rx_subagent_completion));

    let hook_executor = Arc::new(HookExecutor::with_session(
        crate::hooks_load::merge_hooks_configs(
            api_config.hooks_config(),
            crate::hooks_load::load_workspace_hooks(&lean.workspace),
        ),
        lean.workspace.clone(),
        session.id.clone(),
    ));
    config_ext.runtime_services.shell_env =
        Some(Arc::new(HookShellEnvHost(Arc::clone(&hook_executor))) as Arc<dyn ToolShellEnvHost>);

    let kernel_event_writer =
        zagens_runtime_adapters::persist::KernelEventWriter::try_open_default()
            .map(std::sync::Arc::new);
    let kernel_machine_mode = api_config.kernel_machine_mode();
    let kernel_projection_shadow = KernelProjectionShadow::new(kernel_event_writer.is_some());
    let kernel_effect_shadow = KernelEffectShadow::new(
        kernel_event_writer.is_some() && kernel_machine_mode.uses_effect_replay_shadow(),
    );
    let kernel_guard_shadow = KernelGuardShadow::new(
        kernel_event_writer.is_some() && kernel_machine_mode.uses_effect_replay_shadow(),
    );
    let kernel_memory_shadow = KernelMemoryShadow::new(
        kernel_event_writer.is_some() && kernel_machine_mode.uses_effect_replay_shadow(),
    );
    let kernel_replay_shadow = KernelReplayShadow::new(
        kernel_event_writer.is_some() && kernel_machine_mode.uses_replay_verification(),
    );
    let kernel_v3_effect_shadow = super::kernel_v3_effect_shadow::KernelV3EffectShadow::new(
        kernel_event_writer.is_some() && kernel_machine_mode.uses_v3_turn_loop(),
    );
    if kernel_event_writer.is_some() {
        super::kernel_projection_shadow::register_global_projection_shadow_stats(
            kernel_projection_shadow.stats.clone(),
        );
    }
    if kernel_event_writer.is_some() && kernel_machine_mode.uses_effect_replay_shadow() {
        super::kernel_effect_shadow::register_global_effect_shadow_stats(
            kernel_effect_shadow.stats.clone(),
        );
        super::kernel_guard_shadow::register_global_guard_shadow_stats(
            kernel_guard_shadow.stats.clone(),
        );
        super::kernel_memory_shadow::register_global_memory_shadow_stats(
            kernel_memory_shadow.stats.clone(),
        );
    }
    if kernel_event_writer.is_some() && kernel_machine_mode.uses_replay_verification() {
        super::kernel_replay_shadow::register_global_replay_shadow_stats(
            kernel_replay_shadow.stats.clone(),
        );
        register_global_message_coverage_shadow_stats(std::sync::Arc::new(
            KernelMessageCoverageShadowStats::default(),
        ));
        register_global_message_timeline_shadow_stats(std::sync::Arc::new(
            KernelMessageTimelineShadowStats::default(),
        ));
    }
    if kernel_event_writer.is_some() && kernel_machine_mode.uses_v3_turn_loop() {
        super::kernel_v3_effect_shadow::register_global_v3_effect_shadow_stats(
            kernel_v3_effect_shadow.stats.clone(),
        );
    }

    let runtime_ext = EngineRuntimeExt {
        config_ext,
        long_horizon_state: LongHorizonSessionState::default(),
        turn_app_mode: AppMode::Agent,
        turn_lht_mode: None,
        lsp_manager: Arc::clone(&lsp_manager),
        shell_manager: shell_manager.clone(),
        workshop_vars: workshop_vars.clone(),
        subagent_manager: subagent_manager.clone(),
        mcp_pool: None,
        approval_cache: ApprovalCache::default(),
        tx_subagent_completion,
        rx_subagent_completion: rx_subagent_completion.clone(),
        sandbox_init_warning,
        hook_executor,
        session_hooks_started: false,
        tools_policy: api_config.tools_policy_mode(),
        tools_scheduler: api_config.tools_scheduler_mode(),
        resource_lock_registry: Arc::new(crate::tools::resource_locks::ResourceLockRegistry::new()),
        kernel_event_writer,
        kernel_projection_shadow,
        kernel_effect_shadow,
        kernel_guard_shadow,
        kernel_memory_shadow,
        kernel_replay_shadow,
        kernel_v3_effect_shadow,
        kernel_machine_mode,
        kernel_active_turn_id: None,
        kernel_active_step: 0,
    };

    let hosts = EngineHostBundle {
        lsp: lsp_manager.clone() as Arc<dyn zagens_core::engine::LspHost>,
        shell: Box::new(TuiShellHost::new(shell_manager)),
        sandbox: Box::new(TuiSandboxHost::new(sandbox_backend)),
        seam: seam_manager.map(|mgr| Box::new(mgr) as Box<dyn zagens_core::engine::SeamHost>),
        workshop: workshop_vars.map(|vars| {
            Box::new(TuiWorkshopHost(Some(vars))) as Box<dyn zagens_core::engine::WorkshopHost>
        }),
        topic_memory: Box::new(topic_memory_runtime),
        capacity_controller,
        deepseek_client,
        deepseek_client_error,
        api_key_env_only_recovery,
        ext: Box::new(runtime_ext),
        scratchpad_run_id,
    };

    let (mut engine, handle) = Engine::with_hosts(lean, session, hosts);
    engine.rehydrate_latest_canonical_state();

    (engine, handle)
}
