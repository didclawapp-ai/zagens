use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::features::FeaturesToml;

use super::super::types::{Config, ConfigFile, RequirementsFile, *};
use super::paths::{default_managed_config_path, default_requirements_path, expand_path};
#[cfg(windows)]
use crate::config::resolve_windows_sandbox_mode;
#[cfg(windows)]
use zagens_config::WindowsSandboxModeToml;

pub(crate) fn apply_profile(config: ConfigFile, profile: Option<&str>) -> Result<Config> {
    if let Some(profile_name) = profile {
        let profiles = config.profiles.as_ref();
        match profiles.and_then(|profiles| profiles.get(profile_name)) {
            Some(override_cfg) => Ok(merge_config(config.base, override_cfg.clone())),
            None => {
                let available = profiles
                    .map(|profiles| {
                        let mut keys = profiles.keys().cloned().collect::<Vec<_>>();
                        keys.sort();
                        if keys.is_empty() {
                            "none".to_string()
                        } else {
                            keys.join(", ")
                        }
                    })
                    .unwrap_or_else(|| "none".to_string());
                anyhow::bail!(
                    "Profile '{}' not found. Available profiles: {}",
                    profile_name,
                    available
                )
            }
        }
    } else {
        Ok(config.base)
    }
}

pub(crate) fn merge_config(base: Config, override_cfg: Config) -> Config {
    Config {
        provider: override_cfg.provider.or(base.provider),
        api_key: override_cfg.api_key.or(base.api_key),
        base_url: override_cfg.base_url.or(base.base_url),
        http_headers: override_cfg.http_headers.or(base.http_headers),
        default_text_model: override_cfg.default_text_model.or(base.default_text_model),
        reasoning_effort: override_cfg.reasoning_effort.or(base.reasoning_effort),
        cost_currency: override_cfg.cost_currency.or(base.cost_currency),
        tools_file: override_cfg.tools_file.or(base.tools_file),
        skills_dir: override_cfg.skills_dir.or(base.skills_dir),
        mcp_config_path: override_cfg.mcp_config_path.or(base.mcp_config_path),
        notes_path: override_cfg.notes_path.or(base.notes_path),
        memory_path: override_cfg.memory_path.or(base.memory_path),
        // #454: project's instructions array replaces user's array
        // wholesale. The typical "merge" pattern is for users who want
        // both — they list `~/global.md` inside the project array.
        instructions: override_cfg.instructions.or(base.instructions),
        allow_shell: override_cfg.allow_shell.or(base.allow_shell),
        approval_policy: override_cfg.approval_policy.or(base.approval_policy),
        sandbox_mode: override_cfg.sandbox_mode.or(base.sandbox_mode),
        prefer_bwrap: override_cfg.prefer_bwrap.or(base.prefer_bwrap),
        sandbox_backend: override_cfg.sandbox_backend.or(base.sandbox_backend),
        sandbox_url: override_cfg.sandbox_url.or(base.sandbox_url),
        sandbox_api_key: override_cfg.sandbox_api_key.or(base.sandbox_api_key),
        managed_config_path: override_cfg
            .managed_config_path
            .or(base.managed_config_path),
        requirements_path: override_cfg.requirements_path.or(base.requirements_path),
        max_subagents: override_cfg.max_subagents.or(base.max_subagents),
        retry: override_cfg.retry.or(base.retry),
        capacity: override_cfg.capacity.or(base.capacity),
        tui: override_cfg.tui.or(base.tui),
        hooks: override_cfg.hooks.or(base.hooks),
        providers: merge_providers(base.providers, override_cfg.providers),
        vision: override_cfg.vision.or(base.vision),
        features: merge_features(base.features, override_cfg.features),
        notifications: override_cfg.notifications.or(base.notifications),
        network: override_cfg.network.or(base.network),
        skills: override_cfg.skills.or(base.skills),
        snapshots: override_cfg.snapshots.or(base.snapshots),
        search: override_cfg.search.or(base.search),
        memory: override_cfg.memory.or(base.memory),
        topic_memory: override_cfg.topic_memory.or(base.topic_memory),
        session: override_cfg.session.or(base.session),
        lsp: override_cfg.lsp.or(base.lsp),
        context: ContextConfig {
            enabled: override_cfg.context.enabled.or(base.context.enabled),
            verbatim_window_turns: override_cfg
                .context
                .verbatim_window_turns
                .or(base.context.verbatim_window_turns),
            l1_threshold: override_cfg
                .context
                .l1_threshold
                .or(base.context.l1_threshold),
            l2_threshold: override_cfg
                .context
                .l2_threshold
                .or(base.context.l2_threshold),
            l3_threshold: override_cfg
                .context
                .l3_threshold
                .or(base.context.l3_threshold),
            cycle_threshold: override_cfg
                .context
                .cycle_threshold
                .or(base.context.cycle_threshold),
            seam_model: override_cfg.context.seam_model.or(base.context.seam_model),
            per_model: override_cfg.context.per_model.or(base.context.per_model),
        },
        subagents: override_cfg.subagents.or(base.subagents),
        strict_tool_mode: override_cfg.strict_tool_mode.or(base.strict_tool_mode),
        runtime_api: override_cfg.runtime_api.or(base.runtime_api),
        workshop: override_cfg.workshop.or(base.workshop),
        scratchpad: override_cfg.scratchpad.or(base.scratchpad),
        long_horizon: override_cfg.long_horizon.or(base.long_horizon),
        compaction: override_cfg.compaction.or(base.compaction),
        windows: override_cfg.windows.or(base.windows),
        tools: merge_tools_config(base.tools, override_cfg.tools),
        kernel: override_cfg.kernel.or(base.kernel),
    }
}

fn merge_tools_config(
    base: Option<ToolsConfigToml>,
    override_cfg: Option<ToolsConfigToml>,
) -> Option<ToolsConfigToml> {
    match (base, override_cfg) {
        (None, None) => None,
        (Some(b), None) => Some(b),
        (None, Some(o)) => Some(o),
        (Some(b), Some(o)) => Some(ToolsConfigToml {
            policy: o.policy.or(b.policy),
            scheduler: o.scheduler.or(b.scheduler),
            compiler: o.compiler.or(b.compiler),
        }),
    }
}

pub(crate) fn merge_provider_config(
    base: ProviderConfig,
    override_cfg: ProviderConfig,
) -> ProviderConfig {
    ProviderConfig {
        api_key: override_cfg.api_key.or(base.api_key),
        base_url: override_cfg.base_url.or(base.base_url),
        model: override_cfg.model.or(base.model),
        http_headers: override_cfg.http_headers.or(base.http_headers),
    }
}

pub(crate) fn merge_providers(
    base: Option<ProvidersConfig>,
    override_cfg: Option<ProvidersConfig>,
) -> Option<ProvidersConfig> {
    match (base, override_cfg) {
        (None, None) => None,
        (Some(base), None) => Some(base),
        (None, Some(override_cfg)) => Some(override_cfg),
        (Some(base), Some(override_cfg)) => Some(ProvidersConfig {
            deepseek: merge_provider_config(base.deepseek, override_cfg.deepseek),
            deepseek_cn: merge_provider_config(base.deepseek_cn, override_cfg.deepseek_cn),
            nvidia_nim: merge_provider_config(base.nvidia_nim, override_cfg.nvidia_nim),
            openai: merge_provider_config(base.openai, override_cfg.openai),
            openrouter: merge_provider_config(base.openrouter, override_cfg.openrouter),
            novita: merge_provider_config(base.novita, override_cfg.novita),
            fireworks: merge_provider_config(base.fireworks, override_cfg.fireworks),
            sglang: merge_provider_config(base.sglang, override_cfg.sglang),
            vllm: merge_provider_config(base.vllm, override_cfg.vllm),
            ollama: merge_provider_config(base.ollama, override_cfg.ollama),
        }),
    }
}

pub(crate) fn load_single_config_file(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let parsed: ConfigFile = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(parsed.base)
}

pub(crate) fn apply_managed_overrides(config: &mut Config) -> Result<()> {
    let path = config
        .managed_config_path
        .as_deref()
        .map(expand_path)
        .or_else(default_managed_config_path);
    let Some(path) = path else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let managed = load_single_config_file(&path)?;
    *config = merge_config(config.clone(), managed);
    Ok(())
}

pub(crate) fn apply_requirements(config: &mut Config) -> Result<()> {
    let path = config
        .requirements_path
        .as_deref()
        .map(expand_path)
        .or_else(default_requirements_path);
    let Some(path) = path else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read requirements file: {}", path.display()))?;
    let requirements: RequirementsFile = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse requirements file: {}", path.display()))?;

    if !requirements.allowed_approval_policies.is_empty()
        && let Some(policy) = config.approval_policy.as_ref()
    {
        let policy = policy.to_ascii_lowercase();
        if !requirements
            .allowed_approval_policies
            .iter()
            .any(|p| p.eq_ignore_ascii_case(&policy))
        {
            anyhow::bail!(
                "approval_policy '{policy}' is not allowed by requirements ({})",
                requirements.allowed_approval_policies.join(", ")
            );
        }
    }
    if !requirements.allowed_sandbox_modes.is_empty()
        && let Some(mode) = config.sandbox_mode.as_ref()
    {
        let mode = mode.to_ascii_lowercase();
        if !requirements
            .allowed_sandbox_modes
            .iter()
            .any(|m| m.eq_ignore_ascii_case(&mode))
        {
            anyhow::bail!(
                "sandbox_mode '{mode}' is not allowed by requirements ({})",
                requirements.allowed_sandbox_modes.join(", ")
            );
        }
    }

    if !requirements.allowed_windows_sandbox_modes.is_empty() {
        #[cfg(windows)]
        {
            let mode = resolve_windows_sandbox_mode(config);
            let mode_str = match mode {
                WindowsSandboxModeToml::Elevated => "elevated",
                WindowsSandboxModeToml::Unelevated => "unelevated",
            };
            if !requirements
                .allowed_windows_sandbox_modes
                .iter()
                .any(|m| m.eq_ignore_ascii_case(mode_str))
            {
                anyhow::bail!(
                    "windows sandbox mode '{mode_str}' is not allowed by requirements ({})",
                    requirements.allowed_windows_sandbox_modes.join(", ")
                );
            }
        }
    }

    if requirements.require_windows_sandbox_setup {
        #[cfg(windows)]
        {
            let mode = resolve_windows_sandbox_mode(config);
            if mode == WindowsSandboxModeToml::Elevated {
                let home = zagens_windows_sandbox::zagens_home();
                if !zagens_windows_sandbox::sandbox_setup_is_complete(&home) {
                    anyhow::bail!(
                        "requirements require completed Windows elevated sandbox setup; \
                         run `zagens sandbox setup` (home: {})",
                        home.display()
                    );
                }
            }
        }
    }

    Ok(())
}

pub(crate) fn merge_features(
    base: Option<FeaturesToml>,
    override_cfg: Option<FeaturesToml>,
) -> Option<FeaturesToml> {
    match (base, override_cfg) {
        (None, None) => None,
        (Some(mut base), Some(override_cfg)) => {
            for (key, value) in override_cfg.entries {
                base.entries.insert(key, value);
            }
            Some(base)
        }
        (Some(base), None) => Some(base),
        (None, Some(override_cfg)) => Some(override_cfg),
    }
}

#[cfg(test)]
mod requirements_tests {
    use super::*;
    use std::fs;

    fn write_requirements(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
        let path = dir.join("requirements.toml");
        fs::write(&path, body).expect("write requirements");
        path
    }

    #[test]
    fn apply_requirements_rejects_disallowed_sandbox_mode() {
        let dir = std::env::temp_dir().join(format!("zagens-req-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let path = write_requirements(
            &dir,
            r#"
allowed_sandbox_modes = ["read-only"]
"#,
        );
        let mut config = Config::default();
        config.requirements_path = Some(path.to_string_lossy().into());
        config.sandbox_mode = Some("workspace-write".into());
        let err = apply_requirements(&mut config).unwrap_err();
        assert!(err.to_string().contains("not allowed by requirements"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(windows)]
    #[test]
    fn apply_requirements_windows_mode_allowlist() {
        let dir = std::env::temp_dir().join(format!("zagens-req-win-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        let path = write_requirements(
            &dir,
            r#"
allowed_windows_sandbox_modes = ["elevated"]
"#,
        );
        let mut config = Config::default();
        config.requirements_path = Some(path.to_string_lossy().into());
        config.windows = Some(zagens_config::WindowsConfigToml {
            sandbox: Some(zagens_config::WindowsSandboxModeToml::Unelevated),
            sandbox_private_desktop: None,
            sandbox_initialized: None,
        });
        let err = apply_requirements(&mut config).unwrap_err();
        assert!(err.to_string().contains("windows sandbox mode"));
        let _ = fs::remove_dir_all(&dir);
    }
}
