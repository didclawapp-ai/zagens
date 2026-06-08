use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::features::{Features, FeaturesToml, is_known_feature_key};
use crate::hooks::HooksConfig;

use super::super::providers::{ApiProvider, normalize_model_name};
use super::super::types::*;
use super::super::{
    API_KEYRING_SENTINEL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEKCN_BASE_URL,
    DEFAULT_FIREWORKS_BASE_URL, DEFAULT_FIREWORKS_MODEL, DEFAULT_MAX_SUBAGENTS,
    DEFAULT_NOVITA_BASE_URL, DEFAULT_NOVITA_MODEL, DEFAULT_NVIDIA_NIM_BASE_URL,
    DEFAULT_NVIDIA_NIM_MODEL, DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL,
    DEFAULT_OPENROUTER_BASE_URL, DEFAULT_OPENROUTER_MODEL, DEFAULT_SGLANG_BASE_URL,
    DEFAULT_SGLANG_MODEL, DEFAULT_TEXT_MODEL, DEFAULT_VLLM_BASE_URL, DEFAULT_VLLM_MODEL,
    MAX_SUBAGENTS,
};
use super::env_overrides::apply_env_overrides;
use super::merge::{apply_managed_overrides, apply_profile, apply_requirements};
use super::model::{
    model_for_provider, normalize_base_url, normalize_model_config, normalize_model_for_provider,
};
use super::paths::{
    default_mcp_config_path, default_memory_path, default_notes_path, default_skills_dir,
    expand_path, resolve_load_config_path,
};

// === Config Loading ===

impl Config {
    /// Load configuration from disk and merge with environment overrides.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// # use crate::config::Config;
    /// let config = Config::load(None, None)?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn load(path: Option<PathBuf>, profile: Option<&str>) -> Result<Self> {
        let path = resolve_load_config_path(path);
        let mut config = if let Some(path) = path.as_ref() {
            if path.exists() {
                let contents = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config file: {}", path.display()))?;
                let parsed: ConfigFile = toml::from_str(&contents)
                    .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
                apply_profile(parsed, profile)?
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };

        apply_env_overrides(&mut config);
        apply_managed_overrides(&mut config)?;
        apply_requirements(&mut config)?;
        normalize_model_config(&mut config);
        config.validate()?;
        Ok(config)
    }

    /// Validate that critical config fields are present.
    pub fn validate(&self) -> Result<()> {
        if let Some(provider) = self.provider.as_deref()
            && ApiProvider::parse(provider).is_none()
        {
            anyhow::bail!(
                "Invalid provider '{provider}': expected deepseek, deepseek-cn, nvidia-nim, openrouter, novita, fireworks, sglang, vllm, or ollama."
            );
        }
        if let Some(ref key) = self.api_key
            && key.trim().is_empty()
        {
            anyhow::bail!("api_key cannot be empty string");
        }
        if let Some(features) = &self.features {
            for key in features.entries.keys() {
                if !is_known_feature_key(key) {
                    anyhow::bail!("Unknown feature flag: {key}");
                }
            }
        }
        if let Some(model) = self.default_text_model.as_deref()
            && !model.trim().eq_ignore_ascii_case("auto")
            && !matches!(self.api_provider(), ApiProvider::Ollama)
            && normalize_model_name(model).is_none()
        {
            anyhow::bail!(
                "Invalid default_text_model '{model}': expected auto or a DeepSeek model ID (for example: deepseek-v4-pro, deepseek-v4-flash, deepseek-ai/deepseek-v4-pro)."
            );
        }
        if let Some(policy) = self.approval_policy.as_deref() {
            let normalized = policy.trim().to_ascii_lowercase();
            if !matches!(
                normalized.as_str(),
                "on-request" | "untrusted" | "never" | "auto" | "suggest"
            ) {
                anyhow::bail!(
                    "Invalid approval_policy '{policy}': expected on-request, untrusted, never, auto, or suggest."
                );
            }
        }
        if let Some(mode) = self.sandbox_mode.as_deref() {
            let normalized = mode.trim().to_ascii_lowercase();
            if !matches!(
                normalized.as_str(),
                "read-only" | "workspace-write" | "danger-full-access" | "external-sandbox"
            ) {
                anyhow::bail!(
                    "Invalid sandbox_mode '{mode}': expected read-only, workspace-write, danger-full-access, or external-sandbox."
                );
            }
        }
        if let Some(tui) = &self.tui
            && let Some(mode) = tui.alternate_screen.as_deref()
        {
            let mode = mode.to_ascii_lowercase();
            if !matches!(mode.as_str(), "auto" | "always" | "never") {
                anyhow::bail!(
                    "Invalid tui.alternate_screen '{mode}': expected auto, always, or never."
                );
            }
        }
        if let Some(capacity) = &self.capacity {
            if let Some(v) = capacity.low_risk_max
                && !(0.0..=1.0).contains(&v)
            {
                anyhow::bail!(
                    "Invalid capacity.low_risk_max '{v}': expected a value in [0.0, 1.0]."
                );
            }
            if let Some(v) = capacity.medium_risk_max
                && !(0.0..=1.0).contains(&v)
            {
                anyhow::bail!(
                    "Invalid capacity.medium_risk_max '{v}': expected a value in [0.0, 1.0]."
                );
            }
            if let (Some(low), Some(medium)) = (capacity.low_risk_max, capacity.medium_risk_max)
                && low > medium
            {
                anyhow::bail!(
                    "Invalid capacity thresholds: low_risk_max ({low}) must be <= medium_risk_max ({medium})."
                );
            }
            if let Some(v) = capacity.severe_violation_ratio
                && !(0.0..=1.0).contains(&v)
            {
                anyhow::bail!(
                    "Invalid capacity.severe_violation_ratio '{v}': expected a value in [0.0, 1.0]."
                );
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn api_provider(&self) -> ApiProvider {
        self.provider
            .as_deref()
            .and_then(ApiProvider::parse)
            .unwrap_or_else(|| {
                self.base_url
                    .as_deref()
                    .filter(|base| base.contains("integrate.api.nvidia.com"))
                    .map(|_| ApiProvider::NvidiaNim)
                    .or_else(|| {
                        self.base_url
                            .as_deref()
                            .filter(|base| base.contains("api.deepseeki.com"))
                            .map(|_| ApiProvider::DeepseekCN)
                    })
                    .unwrap_or(ApiProvider::Deepseek)
            })
    }

    pub(crate) fn provider_config_for(&self, provider: ApiProvider) -> Option<&ProviderConfig> {
        let providers = self.providers.as_ref()?;
        Some(match provider {
            ApiProvider::Deepseek => &providers.deepseek,
            ApiProvider::DeepseekCN => &providers.deepseek_cn,
            ApiProvider::NvidiaNim => &providers.nvidia_nim,
            ApiProvider::Openrouter => &providers.openrouter,
            ApiProvider::Novita => &providers.novita,
            ApiProvider::Fireworks => &providers.fireworks,
            ApiProvider::Sglang => &providers.sglang,
            ApiProvider::Vllm => &providers.vllm,
            ApiProvider::Ollama => &providers.ollama,
        })
    }

    pub(crate) fn provider_config(&self) -> Option<&ProviderConfig> {
        self.provider_config_for(self.api_provider())
    }

    #[must_use]
    pub fn http_headers(&self) -> HashMap<String, String> {
        let mut headers = self.http_headers.clone().unwrap_or_default();
        if let Some(provider_headers) = self
            .provider_config()
            .and_then(|provider| provider.http_headers.as_ref())
        {
            headers.extend(provider_headers.clone());
        }
        headers.retain(|name, value| !name.trim().is_empty() && !value.trim().is_empty());
        headers
    }

    #[must_use]
    pub fn default_model(&self) -> String {
        let provider = self.api_provider();
        if let Some(model) = self
            .provider_config()
            .and_then(|provider| provider.model.as_deref())
        {
            if matches!(provider, ApiProvider::Ollama) {
                return model.trim().to_string();
            }
            if let Some(normalized) = normalize_model_for_provider(provider, model) {
                return normalized;
            }
        }
        if let Some(model) = self.default_text_model.as_deref()
            && matches!(provider, ApiProvider::Ollama)
        {
            return model.trim().to_string();
        }
        if let Some(model) = self.default_text_model.as_deref()
            && model.trim().eq_ignore_ascii_case("auto")
        {
            return "auto".to_string();
        }
        if let Some(model) = self.default_text_model.as_deref()
            && let Some(normalized) = normalize_model_name(model)
        {
            return model_for_provider(provider, normalized);
        }

        match provider {
            ApiProvider::Deepseek | ApiProvider::DeepseekCN => DEFAULT_TEXT_MODEL,
            ApiProvider::NvidiaNim => DEFAULT_NVIDIA_NIM_MODEL,
            ApiProvider::Openrouter => DEFAULT_OPENROUTER_MODEL,
            ApiProvider::Novita => DEFAULT_NOVITA_MODEL,
            ApiProvider::Fireworks => DEFAULT_FIREWORKS_MODEL,
            ApiProvider::Sglang => DEFAULT_SGLANG_MODEL,
            ApiProvider::Vllm => DEFAULT_VLLM_MODEL,
            ApiProvider::Ollama => DEFAULT_OLLAMA_MODEL,
        }
        .to_string()
    }

    /// Return the configured API base URL (normalized).
    #[must_use]
    pub fn deepseek_base_url(&self) -> String {
        let provider = self.api_provider();
        let provider_base = self
            .provider_config_for(provider)
            .and_then(|provider| provider.base_url.clone());
        // Root `base_url` is the legacy DeepSeek field; only NvidiaNim has a
        // back-compat sniff (integrate.api.nvidia.com). OpenRouter / Novita
        // were added in v0.6.7 and require explicit `[providers.<name>]`
        // entries or the corresponding `*_BASE_URL` env var.
        let root_base = match provider {
            ApiProvider::Deepseek | ApiProvider::DeepseekCN => self.base_url.clone(),
            ApiProvider::NvidiaNim => self
                .base_url
                .as_ref()
                .filter(|base| base.contains("integrate.api.nvidia.com"))
                .cloned(),
            ApiProvider::Openrouter
            | ApiProvider::Novita
            | ApiProvider::Fireworks
            | ApiProvider::Sglang
            | ApiProvider::Vllm
            | ApiProvider::Ollama => None,
        };
        let base = provider_base.or(root_base).unwrap_or_else(|| {
            match provider {
                ApiProvider::Deepseek => DEFAULT_DEEPSEEK_BASE_URL,
                ApiProvider::DeepseekCN => DEFAULT_DEEPSEEKCN_BASE_URL,
                ApiProvider::NvidiaNim => DEFAULT_NVIDIA_NIM_BASE_URL,
                ApiProvider::Openrouter => DEFAULT_OPENROUTER_BASE_URL,
                ApiProvider::Novita => DEFAULT_NOVITA_BASE_URL,
                ApiProvider::Fireworks => DEFAULT_FIREWORKS_BASE_URL,
                ApiProvider::Sglang => DEFAULT_SGLANG_BASE_URL,
                ApiProvider::Vllm => DEFAULT_VLLM_BASE_URL,
                ApiProvider::Ollama => DEFAULT_OLLAMA_BASE_URL,
            }
            .to_string()
        });
        normalize_base_url(&base)
    }

    /// Read the API key.
    ///
    /// Precedence: **explicit in-memory override → provider/root config
    /// → environment**.
    ///
    /// The in-memory `self.api_key` override is only honored when the user
    /// explicitly set the field (not the legacy `API_KEYRING_SENTINEL`
    /// placeholder, not empty whitespace).
    pub fn deepseek_api_key(&self) -> Result<String> {
        let provider = self.api_provider();
        let slot = match provider {
            ApiProvider::Deepseek | ApiProvider::DeepseekCN => "deepseek",
            ApiProvider::NvidiaNim => "nvidia-nim",
            ApiProvider::Openrouter => "openrouter",
            ApiProvider::Novita => "novita",
            ApiProvider::Fireworks => "fireworks",
            ApiProvider::Sglang => "sglang",
            ApiProvider::Vllm => "vllm",
            ApiProvider::Ollama => "ollama",
        };

        // 0. Explicit in-memory override (set by onboarding / provider
        //    picker). Wins so a freshly-entered key takes effect immediately.
        if let Some(configured) = self.api_key.as_ref()
            && !configured.trim().is_empty()
            && configured != API_KEYRING_SENTINEL
        {
            return Ok(configured.clone());
        }

        // 1. Config file (provider-scoped slot). This intentionally wins
        // over ambient env so `deepseek auth set` fixes stale shell exports.
        if let Some(configured) = self
            .provider_config_for(provider)
            .and_then(|provider| provider.api_key.clone())
            && !configured.trim().is_empty()
        {
            return Ok(configured);
        }

        // 2. Environment variables. Do not query platform credential stores
        // here; routine startup and doctor checks must stay prompt-free.
        if let Some(value) = zagens_secrets::env_for(slot)
            && !value.trim().is_empty()
        {
            return Ok(value);
        }

        match provider {
            ApiProvider::Deepseek | ApiProvider::DeepseekCN => anyhow::bail!(
                "DeepSeek API key not found.\n\
                 \n\
                 1. Get a key:  https://platform.deepseek.com/api_keys\n\
                 2. Save it (works in every folder, no OS prompts):\n\
                        deepseek auth set --provider deepseek\n\
                 \n\
                 Alternatives:\n\
                   • export DEEPSEEK_API_KEY=<your-key>      (current shell only;\n\
                     also note: zsh users — exports in ~/.zshrc only reach interactive\n\
                     shells, prefer ~/.zshenv for everything)\n\
                   • api_key = \"<your-key>\"  in ~/.deepseek/config.toml"
            ),
            ApiProvider::NvidiaNim => anyhow::bail!(
                "NVIDIA NIM API key not found. Run 'deepseek auth set --provider nvidia-nim', \
                 set NVIDIA_API_KEY/NVIDIA_NIM_API_KEY, or save api_key in ~/.deepseek/config.toml \
                 with provider = \"nvidia-nim\"."
            ),
            ApiProvider::Openrouter => anyhow::bail!(
                "OpenRouter API key not found. Run 'deepseek auth set --provider openrouter', \
                 set OPENROUTER_API_KEY, or add [providers.openrouter] api_key in ~/.deepseek/config.toml."
            ),
            ApiProvider::Novita => anyhow::bail!(
                "Novita API key not found. Run 'deepseek auth set --provider novita', \
                 set NOVITA_API_KEY, or add [providers.novita] api_key in ~/.deepseek/config.toml."
            ),
            ApiProvider::Fireworks => anyhow::bail!(
                "Fireworks AI API key not found. Run 'deepseek auth set --provider fireworks', \
                 set FIREWORKS_API_KEY, or add [providers.fireworks] api_key in ~/.deepseek/config.toml."
            ),
            // Self-hosted deployments commonly run without auth on localhost.
            // Return an empty key and let the client omit the Authorization header.
            ApiProvider::Sglang | ApiProvider::Vllm | ApiProvider::Ollama => Ok(String::new()),
        }
    }

    /// Resolve the skills directory path.
    #[must_use]
    pub fn skills_dir(&self) -> PathBuf {
        self.skills_dir
            .as_deref()
            .map(expand_path)
            .or_else(default_skills_dir)
            .unwrap_or_else(|| PathBuf::from("./skills"))
    }

    /// Resolve the MCP config path.
    #[must_use]
    pub fn mcp_config_path(&self) -> PathBuf {
        self.mcp_config_path
            .as_deref()
            .map(expand_path)
            .or_else(default_mcp_config_path)
            .unwrap_or_else(|| PathBuf::from("./mcp.json"))
    }

    /// Resolve the notes file path.
    #[must_use]
    pub fn notes_path(&self) -> PathBuf {
        self.notes_path
            .as_deref()
            .map(expand_path)
            .or_else(default_notes_path)
            .unwrap_or_else(|| PathBuf::from("./notes.txt"))
    }

    /// Resolve the memory file path.
    #[must_use]
    pub fn memory_path(&self) -> PathBuf {
        self.memory_path
            .as_deref()
            .map(expand_path)
            .or_else(default_memory_path)
            .unwrap_or_else(|| PathBuf::from("./memory.md"))
    }

    /// Resolve instruction file paths for a workspace.
    ///
    /// Priority:
    /// 1. Explicit non-empty `instructions = [...]` → use as-is (expanded).
    /// 2. Otherwise auto-discover `PROJECT_RULES.md` and `.cursor/rules/*.mdc`.
    ///
    /// `.zagens/pick-rules.md` is handled separately by
    /// [`crate::prompts::merge_instruction_paths_with_pick_rules`].
    #[must_use]
    pub fn instructions_paths(&self, workspace: &std::path::Path) -> Vec<PathBuf> {
        if let Some(explicit) = self.instructions.as_deref() {
            let non_empty: Vec<&str> = explicit
                .iter()
                .map(String::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if !non_empty.is_empty() {
                return non_empty.into_iter().map(expand_path).collect();
            }
        }

        let mut paths: Vec<PathBuf> = Vec::new();

        let candidate = workspace.join("PROJECT_RULES.md");
        if candidate.is_file() {
            tracing::info!("auto-discovered instruction: {}", candidate.display());
            paths.push(candidate);
        }

        let cursor_rules = workspace.join(".cursor").join("rules");
        if let Ok(entries) = std::fs::read_dir(&cursor_rules) {
            let mut mdc_files: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "mdc"))
                .collect();
            mdc_files.sort();
            for p in mdc_files {
                tracing::info!("auto-discovered instruction: {}", p.display());
                paths.push(p);
            }
        }

        paths
    }

    /// Whether the user-memory feature is enabled. The default is **off**
    /// to preserve zero-overhead behavior for users who haven't opted in.
    /// Flips to `true` when `[memory] enabled = true` in `config.toml` or
    /// `DEEPSEEK_MEMORY=on` is set in the environment.
    #[must_use]
    pub fn memory_enabled(&self) -> bool {
        self.memory
            .as_ref()
            .and_then(|m| m.enabled)
            .unwrap_or(false)
    }

    /// Return whether shell execution is allowed.
    #[must_use]
    pub fn allow_shell(&self) -> bool {
        self.allow_shell.unwrap_or(true)
    }

    /// Return the maximum number of concurrent sub-agents.
    /// Checks `[subagents] max_concurrent` first, then top-level `max_subagents`,
    /// then falls back to `DEFAULT_MAX_SUBAGENTS`.
    #[must_use]
    pub fn max_subagents(&self) -> usize {
        // Check [subagents] max_concurrent first
        if let Some(subagents_cfg) = self.subagents.as_ref()
            && let Some(max) = subagents_cfg.max_concurrent
        {
            return max.clamp(1, MAX_SUBAGENTS);
        }
        // Fall back to top-level max_subagents
        self.max_subagents
            .unwrap_or(DEFAULT_MAX_SUBAGENTS)
            .clamp(1, MAX_SUBAGENTS)
    }

    /// Resolved per-step sub-agent LLM API timeout (`[subagents] step_timeout_secs`).
    #[must_use]
    pub fn subagent_step_timeout(&self) -> std::time::Duration {
        let secs = self
            .subagents
            .as_ref()
            .and_then(|s| s.step_timeout_secs)
            .unwrap_or(DEFAULT_SUBAGENT_STEP_TIMEOUT_SECS)
            .clamp(
                MIN_SUBAGENT_STEP_TIMEOUT_SECS,
                MAX_SUBAGENT_STEP_TIMEOUT_SECS,
            );
        std::time::Duration::from_secs(secs)
    }

    /// Default `step_timeout_ms` for `agent_spawn` when the model omits the parameter.
    #[must_use]
    pub fn subagent_step_timeout_ms(&self) -> u64 {
        self.subagent_step_timeout()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    /// Resolved sub-agent heartbeat idle timeout (`[subagents] heartbeat_timeout_secs`).
    #[must_use]
    pub fn subagent_heartbeat_timeout(&self) -> std::time::Duration {
        let secs = self
            .subagents
            .as_ref()
            .and_then(|s| s.heartbeat_timeout_secs)
            .unwrap_or(DEFAULT_SUBAGENT_HEARTBEAT_TIMEOUT_SECS)
            .clamp(
                MIN_SUBAGENT_HEARTBEAT_TIMEOUT_SECS,
                MAX_SUBAGENT_HEARTBEAT_TIMEOUT_SECS,
            );
        std::time::Duration::from_secs(secs)
    }

    /// Resolved compaction policy for the engine / runtime API (config.toml
    /// `[compaction]` overrides `settings.toml` `auto_compact` when set).
    #[must_use]
    pub fn compaction_runtime_config(&self, model: &str) -> crate::compaction::CompactionConfig {
        use crate::models::compaction_threshold_for_model_and_effort;
        use crate::settings::Settings;

        let settings = Settings::load().unwrap_or_default();
        let auto_compact = self
            .compaction
            .as_ref()
            .and_then(|c| c.auto_compact)
            .unwrap_or(settings.auto_compact);
        let token_threshold = self
            .compaction
            .as_ref()
            .and_then(|c| c.token_threshold)
            .unwrap_or_else(|| {
                compaction_threshold_for_model_and_effort(model, self.reasoning_effort())
            });
        crate::compaction::CompactionConfig {
            enabled: auto_compact,
            token_threshold,
            model: model.to_string(),
            ..Default::default()
        }
    }

    /// Resolved checkpoint-restart cycle config for the engine.
    ///
    /// Starts from the per-model defaults (`CycleConfig::default()`, 768K) and
    /// applies any `[context] cycle_threshold` (global) / `[context.per_model.
    /// <model>] cycle_threshold` (per-model) overrides, so the cycle boundary
    /// is operator-tunable. With no override the default (768K) is unchanged.
    ///
    /// Note: `CycleConfig::threshold_for` checks `per_model` *before* the
    /// top-level `threshold_tokens`, and the default config seeds per-model
    /// entries for the V4 models — so a global override must also rewrite those
    /// seeded entries, otherwise it would be shadowed for exactly those models.
    #[must_use]
    pub fn cycle_runtime_config(&self, model: &str) -> crate::cycle_manager::CycleConfig {
        use zagens_core::cycle::ModelCycleConfig;

        let mut cfg = crate::cycle_manager::CycleConfig::default();
        if let Some(t) = self.context.cycle_threshold {
            cfg.threshold_tokens = t;
            for m in cfg.per_model.values_mut() {
                m.threshold_tokens = t;
            }
        }
        if let Some(t) = self
            .context
            .per_model
            .as_ref()
            .and_then(|pm| pm.get(model))
            .and_then(|pm| pm.cycle_threshold)
        {
            cfg.per_model
                .entry(model.to_string())
                .or_insert_with(ModelCycleConfig::default)
                .threshold_tokens = t;
        }
        cfg
    }

    /// 读取 session 文件上限（MB）：`[session] max_file_mb` > 环境变量 > 默认 5。
    /// **0 表示不限制**（返回 `u64::MAX`），与现有 `max_session_file_size()` 语义一致。
    #[must_use]
    pub fn session_max_file_mb(&self) -> u64 {
        // 1. TOML [session] max_file_mb
        if let Some(cfg) = self.session.as_ref() {
            return if cfg.max_file_mb > 0 {
                cfg.max_file_mb
            } else {
                u64::MAX // 0 = 不限制
            };
        }
        // 2. 环境变量（向后兼容）
        if let Ok(val) = std::env::var("DEEPSEEK_MAX_SESSION_FILE_MB")
            && let Ok(mb) = val.trim().parse::<u64>()
        {
            return if mb > 0 { mb } else { u64::MAX };
        }
        // 3. 默认 5 MB
        5
    }

    /// Raw sub-agent model override map. Values are validated at spawn time
    /// so an invalid role/type model fails before any partial agent spawn.
    #[must_use]
    pub fn subagent_model_overrides(&self) -> HashMap<String, String> {
        let mut overrides = HashMap::new();
        let Some(cfg) = self.subagents.as_ref() else {
            return overrides;
        };

        let mut insert = |key: &str, value: &Option<String>| {
            if let Some(model) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
                overrides.insert(key.to_string(), model.to_string());
            }
        };
        insert("default", &cfg.default_model);
        insert("worker", &cfg.worker_model);
        insert("general", &cfg.worker_model);
        insert("explorer", &cfg.explorer_model);
        insert("explore", &cfg.explorer_model);
        insert("awaiter", &cfg.awaiter_model);
        insert("plan", &cfg.awaiter_model);
        insert("review", &cfg.review_model);
        insert("custom", &cfg.custom_model);

        if let Some(models) = cfg.models.as_ref() {
            for (key, model) in models {
                let key = key.trim();
                let model = model.trim();
                if !key.is_empty() && !model.is_empty() {
                    overrides.insert(key.to_ascii_lowercase(), model.to_string());
                }
            }
        }

        overrides
    }

    /// Return the configured DeepSeek reasoning-effort tier, if any.
    #[must_use]
    pub fn reasoning_effort(&self) -> Option<&str> {
        self.reasoning_effort.as_deref()
    }

    /// Get hooks configuration, returning default if not configured.
    pub fn hooks_config(&self) -> HooksConfig {
        self.hooks.clone().unwrap_or_default().normalized()
    }

    /// Resolve the notifications configuration with defaults applied.
    #[must_use]
    pub fn notifications_config(&self) -> NotificationsConfig {
        self.notifications.clone().unwrap_or_default()
    }

    /// Resolve workspace side-git snapshot settings with defaults applied.
    #[must_use]
    pub fn snapshots_config(&self) -> SnapshotsConfig {
        self.snapshots.clone().unwrap_or_default()
    }

    /// Resolved `[search]` settings with defaults applied.
    #[must_use]
    pub fn search_config(&self) -> SearchConfig {
        self.search.clone().unwrap_or_default()
    }

    /// Audit scratchpad runtime settings (Phase B).
    #[must_use]
    pub fn scratchpad_config(&self) -> crate::scratchpad::ScratchpadConfig {
        self.scratchpad
            .clone()
            .map(crate::scratchpad::ScratchpadConfigToml::into_runtime)
            .unwrap_or_default()
    }

    /// Long-horizon code task harness settings (LHT Phase 1).
    #[must_use]
    pub fn long_horizon_config(&self) -> zagens_core::long_horizon::LongHorizonConfig {
        let mut cfg = self
            .long_horizon
            .clone()
            .map(zagens_core::long_horizon::LongHorizonConfigToml::into_runtime)
            .unwrap_or_default();
        // §6.1/§6.4 (v0.4): the executable completion-gate manifest is only
        // trusted because this `Config` is loaded from a single global/explicit
        // path (`resolve_load_config_path`) and never merges workspace project
        // config. The trust hook is applied explicitly so that if a future
        // workspace/overlay merge feeds `[long_horizon.completion_gate]`, it must
        // pass `trusted = false` here (or sanitize the merged subtree) to keep an
        // untrusted enforce manifest from auto-executing commands.
        cfg.completion_gate = cfg.completion_gate.sanitized_for_source(true);
        cfg
    }

    /// Resolve enabled features from defaults and config entries.
    #[must_use]
    pub fn features(&self) -> Features {
        let mut features = Features::with_defaults();
        if let Some(table) = &self.features {
            features.apply_map(&table.entries);
        }
        features
    }

    /// Override a feature flag in memory (used by CLI overrides).
    pub fn set_feature(&mut self, key: &str, enabled: bool) -> Result<()> {
        if !is_known_feature_key(key) {
            anyhow::bail!("Unknown feature flag: {key}");
        }
        let table = self.features.get_or_insert_with(FeaturesToml::default);
        table.entries.insert(key.to_string(), enabled);
        Ok(())
    }

    /// Resolve the effective retry policy with defaults applied.
    #[must_use]
    pub fn retry_policy(&self) -> RetryPolicy {
        let defaults = RetryPolicy {
            enabled: true,
            max_retries: 3,
            initial_delay: 1.0,
            max_delay: 60.0,
            exponential_base: 2.0,
        };

        let Some(cfg) = &self.retry else {
            return defaults;
        };

        RetryPolicy {
            enabled: cfg.enabled.unwrap_or(defaults.enabled),
            max_retries: cfg.max_retries.unwrap_or(defaults.max_retries),
            initial_delay: cfg.initial_delay.unwrap_or(defaults.initial_delay),
            max_delay: cfg.max_delay.unwrap_or(defaults.max_delay),
            exponential_base: cfg.exponential_base.unwrap_or(defaults.exponential_base),
        }
    }
}
