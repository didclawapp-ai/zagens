use std::collections::HashMap;
use std::fmt::Write;
use std::fs;
#[cfg(unix)]
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use crate::audit::log_sensitive_event;
use crate::features::is_known_feature_key;

use crate::features::{Features, FeaturesToml};
use crate::hooks::HooksConfig;

use super::providers::{canonical_model_name, normalize_model_name, ApiProvider};
use super::types::{ConfigFile, RequirementsFile, *};
use super::{
    API_KEYRING_SENTINEL, DEFAULT_DEEPSEEK_BASE_URL, DEFAULT_DEEPSEEKCN_BASE_URL,
    DEFAULT_FIREWORKS_BASE_URL, DEFAULT_FIREWORKS_MODEL, DEFAULT_MAX_SUBAGENTS,
    DEFAULT_NVIDIA_NIM_BASE_URL, DEFAULT_NVIDIA_NIM_FLASH_MODEL, DEFAULT_NVIDIA_NIM_MODEL,
    DEFAULT_NOVITA_BASE_URL, DEFAULT_NOVITA_FLASH_MODEL, DEFAULT_NOVITA_MODEL,
    DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL, DEFAULT_OPENROUTER_BASE_URL,
    DEFAULT_OPENROUTER_FLASH_MODEL, DEFAULT_OPENROUTER_MODEL, DEFAULT_SGLANG_BASE_URL,
    DEFAULT_SGLANG_FLASH_MODEL, DEFAULT_SGLANG_MODEL, DEFAULT_TEXT_MODEL, DEFAULT_VLLM_BASE_URL,
    DEFAULT_VLLM_FLASH_MODEL, DEFAULT_VLLM_MODEL,
    MAX_SUBAGENTS,
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
        if let Some(value) = deepseek_secrets::env_for(slot)
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
    /// `.deepseek/pick-rules.md` is handled separately by
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
        if let Ok(val) = std::env::var("DEEPSEEK_MAX_SESSION_FILE_MB") {
            if let Ok(mb) = val.trim().parse::<u64>() {
                return if mb > 0 { mb } else { u64::MAX };
            }
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
        self.hooks.clone().unwrap_or_default()
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

    /// Audit scratchpad runtime settings (Phase B).
    #[must_use]
    pub fn scratchpad_config(&self) -> crate::scratchpad::ScratchpadConfig {
        self.scratchpad
            .clone()
            .map(crate::scratchpad::ScratchpadConfigToml::into_runtime)
            .unwrap_or_default()
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

// === Defaults ===

fn default_config_path() -> Option<PathBuf> {
    env_config_path().or_else(home_config_path)
}

fn effective_home_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HOME") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }

    if let Some(path) = std::env::var_os("USERPROFILE") {
        let path = PathBuf::from(path);
        if !path.as_os_str().is_empty() {
            return Some(path);
        }
    }

    #[cfg(windows)]
    {
        if let (Some(drive), Some(homepath)) =
            (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH"))
        {
            let mut path = PathBuf::from(drive);
            path.push(homepath);
            if !path.as_os_str().is_empty() {
                return Some(path);
            }
        }
    }

    dirs::home_dir()
}

fn home_config_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("config.toml"))
}

#[must_use]
pub(crate) fn is_workspace_trusted(workspace: &Path) -> bool {
    let Some(config_path) = default_config_path() else {
        return false;
    };
    let Ok(raw) = fs::read_to_string(config_path) else {
        return false;
    };
    let Ok(doc) = toml::from_str::<toml::Value>(&raw) else {
        return false;
    };
    workspace_trust_level_from_doc(&doc, workspace).is_some_and(is_trusted_level)
}

pub(crate) fn save_workspace_trust(workspace: &Path) -> Result<PathBuf> {
    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;
    ensure_parent_dir(&config_path)?;

    let mut doc = if config_path.exists() {
        let raw = fs::read_to_string(&config_path)?;
        toml::from_str::<toml::Value>(&raw)
            .with_context(|| format!("Failed to parse config at {}", config_path.display()))?
    } else {
        toml::Value::Table(toml::value::Table::new())
    };

    let root = doc
        .as_table_mut()
        .context("Config root must be a TOML table.")?;
    let projects = root
        .entry("projects".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .context("`projects` must be a table.")?;
    let project = projects
        .entry(workspace_config_key(workspace))
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .context("Project entry must be a table.")?;
    project.insert(
        "trust_level".to_string(),
        toml::Value::String("trusted".to_string()),
    );

    let serialized = toml::to_string_pretty(&doc).context("failed to serialize updated config")?;
    write_config_file_secure(&config_path, &serialized)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    Ok(config_path)
}

fn workspace_trust_level_from_doc<'a>(doc: &'a toml::Value, workspace: &Path) -> Option<&'a str> {
    let workspace = canonicalize_or_keep(workspace);
    let projects = doc.get("projects")?.as_table()?;
    for (raw_path, project) in projects {
        let project_path = canonicalize_or_keep(&expand_path(raw_path));
        if project_path == workspace {
            return project.get("trust_level").and_then(toml::Value::as_str);
        }
    }
    None
}

fn is_trusted_level(level: &str) -> bool {
    level.trim().eq_ignore_ascii_case("trusted")
}

fn workspace_config_key(workspace: &Path) -> String {
    canonicalize_or_keep(workspace)
        .to_string_lossy()
        .into_owned()
}

fn canonicalize_or_keep(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn env_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DEEPSEEK_CONFIG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(expand_path(trimmed));
        }
    }
    None
}

fn expand_pathbuf(path: PathBuf) -> PathBuf {
    if let Some(raw) = path.to_str() {
        return expand_path(raw);
    }
    path
}

fn resolve_load_config_path(path: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = path {
        return Some(expand_pathbuf(path));
    }

    if let Some(path) = env_config_path() {
        if path.exists() {
            return Some(path);
        }

        if let Some(home_path) = home_config_path()
            && home_path.exists()
        {
            return Some(home_path);
        }

        return Some(path);
    }

    home_config_path()
}

/// Create an inspectable config file on first interactive launch.
///
/// The file intentionally omits `api_key`; onboarding or `deepseek auth set`
/// writes that field after the user supplies a key.
pub fn ensure_config_file_exists(path: Option<PathBuf>) -> Result<Option<PathBuf>> {
    let config_path = path
        .map(expand_pathbuf)
        .or_else(default_config_path)
        .context("Failed to resolve config path: home directory not found.")?;
    if config_path.exists() {
        return Ok(None);
    }

    ensure_parent_dir(&config_path)?;
    let content = format!(
        r#"# DeepSeek TUI Configuration
# Get your API key from https://platform.deepseek.com
# Save it with: deepseek auth set --provider deepseek

# Base URL (default: https://api.deepseek.com/beta)
# Set https://api.deepseek.com to opt out of beta features.
# base_url = "https://api.deepseek.com/beta"

# Default model
default_text_model = "{default_model}"

# Thinking mode (DeepSeek V4 reasoning effort):
# "auto" | "off" | "low" | "medium" | "high" | "max"
# Shift+Tab in the TUI cycles between off / high / max.
reasoning_effort = "auto"
"#,
        default_model = DEFAULT_TEXT_MODEL
    );
    write_config_file_secure(&config_path, &content)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    Ok(Some(config_path))
}

fn default_managed_config_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from("/etc/deepseek/managed_config.toml"))
    }
    #[cfg(not(unix))]
    {
        effective_home_dir().map(|home| home.join(".deepseek").join("managed_config.toml"))
    }
}

fn default_requirements_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from("/etc/deepseek/requirements.toml"))
    }
    #[cfg(not(unix))]
    {
        effective_home_dir().map(|home| home.join(".deepseek").join("requirements.toml"))
    }
}

pub(crate) fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix('~')
        && (stripped.is_empty() || stripped.starts_with('/') || stripped.starts_with('\\'))
        && let Some(mut home) = effective_home_dir()
    {
        let suffix = stripped.trim_start_matches(['/', '\\']);
        if !suffix.is_empty() {
            home.push(suffix);
        }
        return home;
    }

    let expanded = shellexpand::tilde(path);
    PathBuf::from(expanded.as_ref())
}

fn default_skills_dir() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("skills"))
}

fn default_mcp_config_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("mcp.json"))
}

fn default_notes_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("notes.txt"))
}

fn default_memory_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("memory.md"))
}

// === Environment Overrides ===

fn apply_env_overrides(config: &mut Config) {
    if let Ok(value) = std::env::var("DEEPSEEK_PROVIDER") {
        config.provider = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_BASE_URL") {
        if matches!(config.api_provider(), ApiProvider::NvidiaNim) {
            config
                .providers
                .get_or_insert_with(ProvidersConfig::default)
                .nvidia_nim
                .base_url = Some(value);
        } else {
            config.base_url = Some(value);
        }
    }
    if matches!(config.api_provider(), ApiProvider::NvidiaNim)
        && let Ok(value) = std::env::var("NVIDIA_NIM_BASE_URL")
            .or_else(|_| std::env::var("NIM_BASE_URL"))
            .or_else(|_| std::env::var("NVIDIA_BASE_URL"))
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .nvidia_nim
            .base_url = Some(value);
    }
    // OpenRouter / Novita are scoped only on their own provider entry — the
    // legacy root `base_url` keeps DeepSeek-only semantics.
    if matches!(config.api_provider(), ApiProvider::Openrouter)
        && let Ok(value) = std::env::var("OPENROUTER_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .openrouter
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Novita)
        && let Ok(value) = std::env::var("NOVITA_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .novita
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Fireworks)
        && let Ok(value) = std::env::var("FIREWORKS_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .fireworks
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Sglang)
        && let Ok(value) = std::env::var("SGLANG_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .sglang
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Vllm)
        && let Ok(value) = std::env::var("VLLM_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .vllm
            .base_url = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_HTTP_HEADERS")
        && let Ok(headers) = parse_http_headers(&value)
        && !headers.is_empty()
    {
        let mut root_headers = config.http_headers.clone().unwrap_or_default();
        root_headers.extend(headers.clone());
        config.http_headers = Some(root_headers);

        let provider = config.api_provider();
        let providers = config
            .providers
            .get_or_insert_with(ProvidersConfig::default);
        let entry = match provider {
            ApiProvider::Deepseek => &mut providers.deepseek,
            ApiProvider::DeepseekCN => &mut providers.deepseek_cn,
            ApiProvider::NvidiaNim => &mut providers.nvidia_nim,
            ApiProvider::Openrouter => &mut providers.openrouter,
            ApiProvider::Novita => &mut providers.novita,
            ApiProvider::Fireworks => &mut providers.fireworks,
            ApiProvider::Sglang => &mut providers.sglang,
            ApiProvider::Vllm => &mut providers.vllm,
            ApiProvider::Ollama => &mut providers.ollama,
        };
        let mut provider_headers = entry.http_headers.clone().unwrap_or_default();
        provider_headers.extend(headers);
        entry.http_headers = Some(provider_headers);
    }
    if matches!(config.api_provider(), ApiProvider::Ollama)
        && let Ok(value) = std::env::var("OLLAMA_BASE_URL")
        && !value.trim().is_empty()
    {
        config
            .providers
            .get_or_insert_with(ProvidersConfig::default)
            .ollama
            .base_url = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Sglang)
        && let Ok(value) = std::env::var("SGLANG_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Vllm)
        && let Ok(value) = std::env::var("VLLM_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::Ollama)
        && let Ok(value) = std::env::var("OLLAMA_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if let Ok(value) =
        std::env::var("DEEPSEEK_MODEL").or_else(|_| std::env::var("DEEPSEEK_DEFAULT_TEXT_MODEL"))
    {
        config.default_text_model = Some(value);
    }
    if matches!(config.api_provider(), ApiProvider::NvidiaNim)
        && let Ok(value) = std::env::var("NVIDIA_NIM_MODEL")
    {
        config.default_text_model = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SKILLS_DIR") {
        config.skills_dir = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MCP_CONFIG") {
        config.mcp_config_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_NOTES_PATH") {
        config.notes_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MEMORY_PATH") {
        config.memory_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MEMORY") {
        let on = matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "true" | "yes" | "y" | "enabled"
        );
        config
            .memory
            .get_or_insert_with(MemoryConfig::default)
            .enabled = Some(on);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_ALLOW_SHELL") {
        config.allow_shell = Some(value == "1" || value.eq_ignore_ascii_case("true"));
    }
    if let Ok(value) = std::env::var("DEEPSEEK_APPROVAL_POLICY") {
        config.approval_policy = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_MODE") {
        config.sandbox_mode = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_BACKEND") {
        config.sandbox_backend = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_URL") {
        config.sandbox_url = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_SANDBOX_API_KEY") {
        config.sandbox_api_key = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MANAGED_CONFIG_PATH") {
        config.managed_config_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_REQUIREMENTS_PATH") {
        config.requirements_path = Some(value);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_MAX_SUBAGENTS")
        && let Ok(parsed) = value.parse::<usize>()
    {
        config.max_subagents = Some(parsed.clamp(1, MAX_SUBAGENTS));
    }

    let capacity = config.capacity.get_or_insert(CapacityConfig {
        enabled: None,
        low_risk_max: None,
        medium_risk_max: None,
        severe_min_slack: None,
        severe_violation_ratio: None,
        refresh_cooldown_turns: None,
        replan_cooldown_turns: None,
        max_replay_per_turn: None,
        min_turns_before_guardrail: None,
        profile_window: None,
        deepseek_v3_2_chat_prior: None,
        deepseek_v3_2_reasoner_prior: None,
        deepseek_v4_pro_prior: None,
        deepseek_v4_flash_prior: None,
        fallback_default_prior: None,
    });

    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_ENABLED") {
        let val = value.trim().to_ascii_lowercase();
        capacity.enabled = Some(matches!(val.as_str(), "1" | "true" | "yes" | "on"));
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_LOW_RISK_MAX")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.low_risk_max = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MEDIUM_RISK_MAX")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.medium_risk_max = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_SEVERE_MIN_SLACK")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.severe_min_slack = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_SEVERE_VIOLATION_RATIO")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.severe_violation_ratio = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_REFRESH_COOLDOWN_TURNS")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.refresh_cooldown_turns = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_REPLAN_COOLDOWN_TURNS")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.replan_cooldown_turns = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MAX_REPLAY_PER_TURN")
        && let Ok(parsed) = value.parse::<usize>()
    {
        capacity.max_replay_per_turn = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_MIN_TURNS_BEFORE_GUARDRAIL")
        && let Ok(parsed) = value.parse::<u64>()
    {
        capacity.min_turns_before_guardrail = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PROFILE_WINDOW")
        && let Ok(parsed) = value.parse::<usize>()
    {
        capacity.profile_window = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_CHAT")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v3_2_chat_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_REASONER")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v3_2_reasoner_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_V4_PRO")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v4_pro_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_V4_FLASH")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.deepseek_v4_flash_prior = Some(parsed);
    }
    if let Ok(value) = std::env::var("DEEPSEEK_CAPACITY_PRIOR_FALLBACK")
        && let Ok(parsed) = value.parse::<f64>()
    {
        capacity.fallback_default_prior = Some(parsed);
    }

    if config.capacity.as_ref().is_some_and(|c| {
        c.enabled.is_none()
            && c.low_risk_max.is_none()
            && c.medium_risk_max.is_none()
            && c.severe_min_slack.is_none()
            && c.severe_violation_ratio.is_none()
            && c.refresh_cooldown_turns.is_none()
            && c.replan_cooldown_turns.is_none()
            && c.max_replay_per_turn.is_none()
            && c.min_turns_before_guardrail.is_none()
            && c.profile_window.is_none()
            && c.deepseek_v3_2_chat_prior.is_none()
            && c.deepseek_v3_2_reasoner_prior.is_none()
            && c.deepseek_v4_pro_prior.is_none()
            && c.deepseek_v4_flash_prior.is_none()
            && c.fallback_default_prior.is_none()
    }) {
        config.capacity = None;
    }
}

fn normalize_model_config(config: &mut Config) {
    if let Some(model) = config.default_text_model.as_deref()
        && !matches!(config.api_provider(), ApiProvider::Ollama)
        && let Some(normalized) = normalize_model_for_provider(config.api_provider(), model)
    {
        config.default_text_model = Some(normalized);
    }

    if let Some(providers) = config.providers.as_mut() {
        if let Some(model) = providers.deepseek.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Deepseek, model)
        {
            providers.deepseek.model = Some(normalized);
        }
        if let Some(model) = providers.deepseek_cn.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::DeepseekCN, model)
        {
            providers.deepseek_cn.model = Some(normalized);
        }
        if let Some(model) = providers.nvidia_nim.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::NvidiaNim, model)
        {
            providers.nvidia_nim.model = Some(normalized);
        }
        if let Some(model) = providers.openrouter.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Openrouter, model)
        {
            providers.openrouter.model = Some(normalized);
        }
        if let Some(model) = providers.novita.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Novita, model)
        {
            providers.novita.model = Some(normalized);
        }
        if let Some(model) = providers.fireworks.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Fireworks, model)
        {
            providers.fireworks.model = Some(normalized);
        }
        if let Some(model) = providers.sglang.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Sglang, model)
        {
            providers.sglang.model = Some(normalized);
        }
        if let Some(model) = providers.vllm.model.as_deref()
            && let Some(normalized) = normalize_model_for_provider(ApiProvider::Vllm, model)
        {
            providers.vllm.model = Some(normalized);
        }
    }
}

fn normalize_model_for_provider(provider: ApiProvider, model: &str) -> Option<String> {
    if matches!(provider, ApiProvider::Ollama) {
        return None;
    }
    normalize_model_name(model).map(|normalized| model_for_provider(provider, normalized))
}

fn model_for_provider(provider: ApiProvider, normalized: String) -> String {
    let lowered = normalized.to_ascii_lowercase();
    match (provider, lowered.as_str()) {
        (ApiProvider::NvidiaNim, "deepseek-v4-pro") => DEFAULT_NVIDIA_NIM_MODEL.to_string(),
        (ApiProvider::NvidiaNim, "deepseek-v4-flash") => DEFAULT_NVIDIA_NIM_FLASH_MODEL.to_string(),
        (ApiProvider::Openrouter, "deepseek-v4-pro") => DEFAULT_OPENROUTER_MODEL.to_string(),
        (ApiProvider::Openrouter, "deepseek-v4-flash") => {
            DEFAULT_OPENROUTER_FLASH_MODEL.to_string()
        }
        (ApiProvider::Novita, "deepseek-v4-pro") => DEFAULT_NOVITA_MODEL.to_string(),
        (ApiProvider::Novita, "deepseek-v4-flash") => DEFAULT_NOVITA_FLASH_MODEL.to_string(),
        (ApiProvider::Fireworks, "deepseek-v4-pro") => DEFAULT_FIREWORKS_MODEL.to_string(),
        (ApiProvider::Fireworks, "deepseek-v4-flash") => {
            // Flash not yet available on Fireworks; fall through to normalized name
            "accounts/fireworks/models/deepseek-v4-flash".to_string()
        }
        (ApiProvider::Sglang, "deepseek-v4-pro") => DEFAULT_SGLANG_MODEL.to_string(),
        (ApiProvider::Sglang, "deepseek-v4-flash") => DEFAULT_SGLANG_FLASH_MODEL.to_string(),
        (ApiProvider::Vllm, "deepseek-v4-pro") => DEFAULT_VLLM_MODEL.to_string(),
        (ApiProvider::Vllm, "deepseek-v4-flash") => DEFAULT_VLLM_FLASH_MODEL.to_string(),
        _ => normalized,
    }
}

fn normalize_base_url(base: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    let deepseek_domains = ["api.deepseek.com", "api.deepseeki.com"];
    if deepseek_domains
        .iter()
        .any(|domain| trimmed.contains(domain))
    {
        return trimmed.trim_end_matches("/v1").to_string();
    }
    trimmed.to_string()
}

fn parse_http_headers(raw: &str) -> Result<HashMap<String, String>> {
    let mut headers = HashMap::new();
    for pair in raw.trim().split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((name, value)) = pair.split_once('=') else {
            anyhow::bail!("invalid header pair '{pair}', expected name=value");
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            anyhow::bail!("header name cannot be empty");
        }
        if value.is_empty() {
            continue;
        }
        headers.insert(name.to_string(), value.to_string());
    }
    Ok(headers)
}

fn apply_profile(config: ConfigFile, profile: Option<&str>) -> Result<Config> {
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

fn merge_config(base: Config, override_cfg: Config) -> Config {
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
        compaction: override_cfg.compaction.or(base.compaction),
    }
}

fn merge_provider_config(base: ProviderConfig, override_cfg: ProviderConfig) -> ProviderConfig {
    ProviderConfig {
        api_key: override_cfg.api_key.or(base.api_key),
        base_url: override_cfg.base_url.or(base.base_url),
        model: override_cfg.model.or(base.model),
        http_headers: override_cfg.http_headers.or(base.http_headers),
    }
}

fn merge_providers(
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
            openrouter: merge_provider_config(base.openrouter, override_cfg.openrouter),
            novita: merge_provider_config(base.novita, override_cfg.novita),
            fireworks: merge_provider_config(base.fireworks, override_cfg.fireworks),
            sglang: merge_provider_config(base.sglang, override_cfg.sglang),
            vllm: merge_provider_config(base.vllm, override_cfg.vllm),
            ollama: merge_provider_config(base.ollama, override_cfg.ollama),
        }),
    }
}

fn load_single_config_file(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let parsed: ConfigFile = toml::from_str(&contents)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(parsed.base)
}

fn apply_managed_overrides(config: &mut Config) -> Result<()> {
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

fn apply_requirements(config: &mut Config) -> Result<()> {
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

    Ok(())
}

fn merge_features(
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

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        #[cfg(unix)]
        {
            // Tighten group/other bits on the parent dir as a hardening pass.
            // The dir lives under the user's home, so the chmod is best-effort:
            // filesystems that don't accept Unix permission bits (Docker
            // bind-mounts of NTFS, network shares, FAT, certain CI volumes —
            // see #897) return EPERM/ENOTSUP. The dir already exists by the
            // time we get here, so failing the whole save just because we
            // couldn't tighten perms strands the user mid-onboarding. Warn
            // loudly so a security-sensitive operator can still notice via
            // `RUST_LOG=warn`, then continue.
            if let Ok(meta) = fs::metadata(parent) {
                let mode = meta.permissions().mode();
                if mode & 0o077 != 0 {
                    let mut perms = meta.permissions();
                    perms.set_mode(mode & !0o077);
                    if let Err(err) = fs::set_permissions(parent, perms) {
                        tracing::warn!(
                            target: "deepseek::config",
                            path = %parent.display(),
                            error = %err,
                            "could not tighten parent dir permissions; \
                             filesystem may not support Unix chmod \
                             (Docker bind-mount, NTFS, network share). \
                             Continuing — the file will still be written."
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// Write content to a config file with restrictive permissions (owner-only read/write).
/// On Unix this sets mode 0o600 before writing.
fn write_config_file_secure(path: &Path, content: &str) -> Result<()> {
    #[cfg(unix)]
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(content.as_bytes())?;
        // The file was already opened with mode 0o600; the explicit
        // set_permissions re-asserts that on filesystems where mode-at-open
        // didn't take effect (or where the file already existed with broader
        // bits). Filesystems that don't accept Unix chmod at all (Docker
        // bind-mounts of NTFS, network shares — #897) return EPERM. Treat
        // that as a warning rather than failing the whole save: the file
        // contents are written, and on Windows/macOS hosts the parent file
        // system's native ACL model is doing the access control.
        if let Err(err) = file.set_permissions(fs::Permissions::from_mode(0o600)) {
            tracing::warn!(
                target: "deepseek::config",
                path = %path.display(),
                error = %err,
                "could not enforce 0o600 on config file; filesystem may \
                 not support Unix chmod. File contents written; rely on \
                 host ACLs for access control."
            );
        }
    }
    #[cfg(not(unix))]
    {
        fs::write(path, content)?;
    }
    Ok(())
}

/// Where a saved credential ended up. Returned by [`save_api_key`] so
/// the caller can show a confirmation message without leaking the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SavedCredential {
    /// Stored in **both** the OS keyring and the deepseek config file.
    /// This is the default outcome on platforms with a working keyring
    /// backend: writing both layers defeats the
    /// `keyring → env → config-file` resolution-order shadow that
    /// would otherwise let a stale OS-keyring entry from a previous
    /// install hide the freshly-entered key (#593). The `backend`
    /// label is the value of [`deepseek_secrets::Secrets::backend_name`]
    /// at write time so the toast text can name the actual backend
    /// (`"system keyring"`, `"file-based (~/.deepseek/secrets/)"`).
    KeyringAndConfigFile {
        /// `Secrets::backend_name()` at write time.
        backend: String,
        /// Absolute path to the config file that was also updated.
        path: PathBuf,
    },
    /// Stored in the deepseek config file only. Fallback when no
    /// keyring backend is reachable, or under `cfg(test)` so unit
    /// tests don't pollute the host keyring.
    ConfigFile(PathBuf),
}

impl SavedCredential {
    /// Human-readable description for status / log output. Never
    /// includes the key value.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::KeyringAndConfigFile { backend, path } => {
                format!("OS keyring ({backend}) and {}", path.display())
            }
            Self::ConfigFile(path) => path.display().to_string(),
        }
    }
}

/// Save the active provider's API key.
///
/// **Dual-write strategy (#593):** writes to `~/.deepseek/config.toml`
/// (always) and to the OS keyring via [`deepseek_secrets::Secrets`]
/// (when a backend is reachable). The runtime resolves credentials in
/// `keyring → env → config-file` order; writing to the config file
/// alone — as v0.8.8 through v0.8.10 did — let a stale keyring entry
/// from a prior install silently shadow the fresh value the user just
/// typed during in-TUI onboarding, producing the "no response" symptom
/// reported in #593.
///
/// The config file remains the inspectable durable record (works in
/// npm installs, IDE terminals, and headless boxes alike), and the
/// keyring acts as the layered override that defeats stale-shadow on
/// the resolution path. When the keyring write fails (no backend, OS
/// permission denied, etc.) the config-file write still stands and
/// the function reports a [`SavedCredential::ConfigFile`] outcome —
/// callers should not treat that as a failure.
///
/// Skipped under `cfg(test)` so the suite never touches the host
/// keyring. The `secrets` crate has its own test coverage for
/// keyring set/get.
pub fn save_api_key(api_key: &str) -> Result<SavedCredential> {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        anyhow::bail!("Refusing to save an empty API key.");
    }

    // Always write the inspectable copy first. The config file is the
    // durable record everyone — including macOS Keychain-prompted
    // first-run, headless CI, and IDE terminals — can rely on.
    let path = save_api_key_to_config_file(trimmed)?;

    // Then mirror to the OS keyring when one is reachable. This
    // overwrites any stale entry from a prior install so
    // `Secrets::resolve` (keyring → env → config-file) no longer
    // shadows the fresh key. Skipped under `cfg(test)` so unit tests
    // can't pollute the host keyring (macOS Always-Allow prompts,
    // cross-test contamination).
    #[cfg(not(test))]
    {
        let secrets = deepseek_secrets::Secrets::auto_detect();
        match secrets.set("deepseek", trimmed) {
            Ok(()) => {
                let backend = secrets.backend_name().to_string();
                log_sensitive_event(
                    "credential.save",
                    json!({
                        "backend": backend.clone(),
                        "config_path": path.display().to_string(),
                        "dual_write": true,
                    }),
                );
                return Ok(SavedCredential::KeyringAndConfigFile { backend, path });
            }
            Err(err) => {
                tracing::warn!("OS keyring write failed; key saved to config.toml only: {err}");
                // Fall through to the ConfigFile-only outcome below.
            }
        }
    }

    Ok(SavedCredential::ConfigFile(path))
}

/// Write the `api_key` slot directly to `config.toml`.
fn save_api_key_to_config_file(api_key: &str) -> Result<PathBuf> {
    fn is_api_key_assignment(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed
            .strip_prefix("api_key")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    }

    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;

    ensure_parent_dir(&config_path)?;

    let key_to_write = api_key.to_string();

    let content = if config_path.exists() {
        // Read existing config and update the api_key line
        let existing = fs::read_to_string(&config_path)?;
        if existing.contains("api_key") {
            // Replace existing api_key line
            let mut result = String::new();
            for line in existing.lines() {
                if is_api_key_assignment(line) {
                    let _ = writeln!(result, "api_key = \"{key_to_write}\"");
                } else {
                    result.push_str(line);
                    result.push('\n');
                }
            }
            result
        } else {
            // Prepend api_key to existing config
            format!("api_key = \"{key_to_write}\"\n{existing}")
        }
    } else {
        // Create new minimal config
        format!(
            r#"# DeepSeek TUI Configuration
# Get your API key from https://platform.deepseek.com
# Or set DEEPSEEK_API_KEY environment variable

api_key = "{key_to_write}"

# Base URL (default: https://api.deepseek.com/beta)
# Set https://api.deepseek.com to opt out of beta features.
# base_url = "https://api.deepseek.com/beta"

# Default model
default_text_model = "{default_model}"

# Thinking mode (DeepSeek V4 reasoning effort):
# "off" | "low" | "medium" | "high" | "max"
# Shift+Tab in the TUI cycles between off / high / max.
reasoning_effort = "max"
"#,
            default_model = DEFAULT_TEXT_MODEL
        )
    };

    write_config_file_secure(&config_path, &content)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    log_sensitive_event(
        "credential.save",
        json!({
            "backend": "config_file",
            "config_path": config_path.display().to_string(),
        }),
    );

    Ok(config_path)
}

/// Check if an API key is configured anywhere the runtime can resolve it.
///
/// Order of inspection:
///   1. `DEEPSEEK_API_KEY` env var (fast, no I/O, no OS prompts).
///   2. In-memory override on the config (set by onboarding / picker).
///   3. Config-file `api_key` slot (cheap file read already done by
///      the loaded `Config`).
///
/// Platform credential stores are intentionally not queried here.
/// Startup/onboarding checks must be cheap and prompt-free, so v0.8.8
/// keeps the default auth path to environment variables and
/// `~/.deepseek/config.toml`.
///
/// Used by [`crate::tui::app::App::new`] to decide whether to gate
/// the user behind the in-TUI api-key onboarding screen — getting
/// this wrong made users get prompted for credentials in situations
/// where normal env/config auth was already available.
pub fn has_api_key(config: &Config) -> bool {
    if std::env::var("DEEPSEEK_API_KEY").is_ok_and(|k| !k.trim().is_empty()) {
        return true;
    }
    if config
        .api_key
        .as_ref()
        .is_some_and(|k| !k.trim().is_empty() && k != API_KEYRING_SENTINEL)
    {
        return true;
    }
    false
}

#[must_use]
pub fn active_provider_has_config_api_key(config: &Config) -> bool {
    let provider = config.api_provider();

    if config
        .provider_config_for(provider)
        .and_then(|entry| entry.api_key.as_ref())
        .is_some_and(|k| !k.trim().is_empty() && k != API_KEYRING_SENTINEL)
    {
        return true;
    }

    matches!(provider, ApiProvider::Deepseek | ApiProvider::DeepseekCN)
        && config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.trim().is_empty() && k != API_KEYRING_SENTINEL)
}

#[must_use]
pub fn active_provider_has_env_api_key(config: &Config) -> bool {
    match config.api_provider() {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => {
            std::env::var("DEEPSEEK_API_KEY").is_ok_and(|k| !k.trim().is_empty())
        }
        ApiProvider::NvidiaNim => {
            std::env::var("NVIDIA_API_KEY").is_ok_and(|k| !k.trim().is_empty())
                || std::env::var("NVIDIA_NIM_API_KEY").is_ok_and(|k| !k.trim().is_empty())
        }
        ApiProvider::Openrouter => {
            std::env::var("OPENROUTER_API_KEY").is_ok_and(|k| !k.trim().is_empty())
        }
        ApiProvider::Novita => std::env::var("NOVITA_API_KEY").is_ok_and(|k| !k.trim().is_empty()),
        ApiProvider::Fireworks => {
            std::env::var("FIREWORKS_API_KEY").is_ok_and(|k| !k.trim().is_empty())
        }
        ApiProvider::Sglang => std::env::var("SGLANG_API_KEY").is_ok_and(|k| !k.trim().is_empty()),
        ApiProvider::Vllm => std::env::var("VLLM_API_KEY").is_ok_and(|k| !k.trim().is_empty()),
        ApiProvider::Ollama => std::env::var("OLLAMA_API_KEY").is_ok_and(|k| !k.trim().is_empty()),
    }
}

#[must_use]
pub fn active_provider_uses_env_only_api_key(config: &Config) -> bool {
    active_provider_has_env_api_key(config) && !active_provider_has_config_api_key(config)
}

/// Check whether the given provider has any usable API key — via env var,
/// provider/root config. Used by the `/provider` picker to decide whether to
/// prompt for a key inline.
#[must_use]
pub fn has_api_key_for(config: &Config, provider: ApiProvider) -> bool {
    let env_var = match provider {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => "DEEPSEEK_API_KEY",
        ApiProvider::NvidiaNim => "NVIDIA_API_KEY",
        ApiProvider::Openrouter => "OPENROUTER_API_KEY",
        ApiProvider::Novita => "NOVITA_API_KEY",
        ApiProvider::Fireworks => "FIREWORKS_API_KEY",
        ApiProvider::Sglang => "SGLANG_API_KEY",
        ApiProvider::Vllm => "VLLM_API_KEY",
        ApiProvider::Ollama => "OLLAMA_API_KEY",
    };
    if std::env::var(env_var).is_ok_and(|k| !k.trim().is_empty()) {
        return true;
    }
    if matches!(provider, ApiProvider::NvidiaNim)
        && std::env::var("NVIDIA_NIM_API_KEY").is_ok_and(|k| !k.trim().is_empty())
    {
        return true;
    }

    // Self-hosted providers typically run without authentication.
    if matches!(
        provider,
        ApiProvider::Sglang | ApiProvider::Vllm | ApiProvider::Ollama
    ) {
        return true;
    }

    if config
        .provider_config_for(provider)
        .and_then(|entry| entry.api_key.as_ref())
        .is_some_and(|k| !k.trim().is_empty() && k != API_KEYRING_SENTINEL)
    {
        return true;
    }

    if matches!(provider, ApiProvider::Deepseek | ApiProvider::DeepseekCN)
        && config
            .api_key
            .as_ref()
            .is_some_and(|k| !k.trim().is_empty() && k != API_KEYRING_SENTINEL)
    {
        return true;
    }

    false
}

/// Save an API key to the appropriate place for the given provider.
/// DeepSeek goes through [`save_api_key`]. Other providers write
/// `[providers.<name>] api_key = "..."` to `~/.deepseek/config.toml`.
/// Returns the config file path.
pub fn save_api_key_for(provider: ApiProvider, api_key: &str) -> Result<PathBuf> {
    if matches!(provider, ApiProvider::Deepseek | ApiProvider::DeepseekCN) {
        return match save_api_key(api_key)? {
            SavedCredential::KeyringAndConfigFile { path, .. }
            | SavedCredential::ConfigFile(path) => Ok(path),
        };
    }

    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;
    ensure_parent_dir(&config_path)?;

    let table_name = match provider {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => {
            return Err(anyhow::anyhow!(
                "save_api_key_for: DeepSeek variants must use the root api_key field, not provider-specific storage"
            ));
        }
        ApiProvider::NvidiaNim => "providers.nvidia_nim",
        ApiProvider::Openrouter => "providers.openrouter",
        ApiProvider::Novita => "providers.novita",
        ApiProvider::Fireworks => "providers.fireworks",
        ApiProvider::Sglang => "providers.sglang",
        ApiProvider::Vllm => "providers.vllm",
        ApiProvider::Ollama => "providers.ollama",
    };

    // Parse existing TOML (or start fresh) so we can edit the right table
    // without disturbing other sections.
    let mut doc: toml::Value = if config_path.exists() {
        let raw = fs::read_to_string(&config_path)?;
        toml::from_str(&raw)
            .with_context(|| format!("Failed to parse config at {}", config_path.display()))?
    } else {
        toml::Value::Table(toml::value::Table::new())
    };

    let table = doc
        .as_table_mut()
        .context("Config root must be a TOML table.")?;
    let providers = table
        .entry("providers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .context("`providers` must be a table.")?;
    let key_inside = match provider {
        ApiProvider::Deepseek | ApiProvider::DeepseekCN => {
            return Err(anyhow::anyhow!(
                "save_api_key_for: DeepSeek variants must use the root api_key field, not provider-specific storage"
            ));
        }
        ApiProvider::NvidiaNim => "nvidia_nim",
        ApiProvider::Openrouter => "openrouter",
        ApiProvider::Novita => "novita",
        ApiProvider::Fireworks => "fireworks",
        ApiProvider::Sglang => "sglang",
        ApiProvider::Vllm => "vllm",
        ApiProvider::Ollama => "ollama",
    };
    let entry = providers
        .entry(key_inside.to_string())
        .or_insert_with(|| toml::Value::Table(toml::value::Table::new()))
        .as_table_mut()
        .with_context(|| format!("`{table_name}` must be a table."))?;
    entry.insert(
        "api_key".to_string(),
        toml::Value::String(api_key.to_string()),
    );

    let serialized = toml::to_string_pretty(&doc).context("failed to serialize updated config")?;
    write_config_file_secure(&config_path, &serialized)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    log_sensitive_event(
        "credential.save",
        json!({
            "backend": "config_file",
            "provider": provider.as_str(),
            "config_path": config_path.display().to_string(),
        }),
    );

    Ok(config_path)
}

/// Clear the API key from config-file storage.
///
/// `/logout` calls this to wipe credentials so the next request can't
/// silently use a stale config key (#343). The function strips the legacy
/// root `api_key = ...` line *and* every `api_key` line nested in a
/// `[providers.<name>]` table.
///
/// Environment variables (`DEEPSEEK_API_KEY`, etc.) are intentionally
/// **not** unset — they are managed by the user's shell and outside the
/// CLI's purview. `Config::deepseek_api_key`'s explicit-override path
/// (Path 0) ensures a freshly-entered key still wins over a stale env
/// var that lingers from a previous session.
pub fn clear_api_key() -> Result<()> {
    // Strip api_key lines from config.toml, including provider-scoped nested
    // entries. Clearing a config file must not trigger platform credential
    // prompts.
    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;

    if !config_path.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(&config_path)?;
    let mut result = String::new();

    for line in existing.lines() {
        // Match `api_key`, `api_key =`, `  api_key=`, etc. — anywhere it
        // appears as the leading non-whitespace token.
        let trimmed = line.trim_start();
        if trimmed.strip_prefix("api_key").is_some_and(|rest| {
            let rest = rest.trim_start();
            rest.is_empty() || rest.starts_with('=')
        }) {
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }

    write_config_file_secure(&config_path, &result)
        .with_context(|| format!("Failed to write config to {}", config_path.display()))?;
    log_sensitive_event(
        "credential.clear",
        json!({
            "backend": "config_file",
            "config_path": config_path.display().to_string(),
            "scope": "root_and_provider_keys",
        }),
    );

    Ok(())
}

#[cfg(test)]
include!("tests.inc.rs");
