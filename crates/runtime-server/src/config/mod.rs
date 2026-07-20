//! Configuration loading and defaults for DeepSeek TUI.

pub const DEFAULT_MAX_SUBAGENTS: usize = 10;
pub const MAX_SUBAGENTS: usize = 20;
pub const DEFAULT_TEXT_MODEL: &str = "deepseek-v4-pro";
pub const DEFAULT_DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/beta";
pub const DEFAULT_NVIDIA_NIM_MODEL: &str = "deepseek-ai/deepseek-v4-pro";
pub const DEFAULT_NVIDIA_NIM_FLASH_MODEL: &str = "deepseek-ai/deepseek-v4-flash";
pub const DEFAULT_NVIDIA_NIM_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
pub const DEFAULT_OPENROUTER_MODEL: &str = "deepseek/deepseek-v4-pro";
pub const DEFAULT_OPENROUTER_FLASH_MODEL: &str = "deepseek/deepseek-v4-flash";
pub const DEFAULT_OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
pub const DEFAULT_NOVITA_MODEL: &str = "deepseek/deepseek-v4-pro";
pub const DEFAULT_NOVITA_FLASH_MODEL: &str = "deepseek/deepseek-v4-flash";
pub const DEFAULT_NOVITA_BASE_URL: &str = "https://api.novita.ai/v1";
pub const DEFAULT_FIREWORKS_MODEL: &str = "accounts/fireworks/models/deepseek-v4-pro";
pub const DEFAULT_FIREWORKS_BASE_URL: &str = "https://api.fireworks.ai/inference/v1";
pub const DEFAULT_SGLANG_MODEL: &str = "deepseek-ai/DeepSeek-V4-Pro";
pub const DEFAULT_SGLANG_FLASH_MODEL: &str = "deepseek-ai/DeepSeek-V4-Flash";
pub const DEFAULT_SGLANG_BASE_URL: &str = "http://localhost:30000/v1";
pub const DEFAULT_VLLM_MODEL: &str = "deepseek-ai/DeepSeek-V4-Pro";
pub const DEFAULT_VLLM_FLASH_MODEL: &str = "deepseek-ai/DeepSeek-V4-Flash";
pub const DEFAULT_VLLM_BASE_URL: &str = "http://localhost:8000/v1";
pub const DEFAULT_OLLAMA_MODEL: &str = "deepseek-coder:1.3b";
pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434/v1";
pub const DEFAULT_AGNES_MODEL: &str = "agnes-2.0-flash";
pub const DEFAULT_AGNES_BASE_URL: &str = "https://apihub.agnes-ai.com/v1";
pub const DEFAULT_SENSENOVA_MODEL: &str = "sensenova-6.7-flash-lite";
pub const DEFAULT_SENSENOVA_BASE_URL: &str = "https://token.sensenova.cn/v1";
pub const DEFAULT_MOONSHOT_MODEL: &str = "kimi-k3";
pub const DEFAULT_MOONSHOT_BASE_URL: &str = "https://api.moonshot.cn/v1";
// OpenAI-compatible endpoint (M0.5). Defaults mirror the facade
// (`zagens_config::DEFAULT_OPENAI_*`); drift is caught by tests in providers.rs.
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4.1";
pub const DEFAULT_OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const DEFAULT_DEEPSEEKCN_BASE_URL: &str = "https://api.deepseeki.com";
pub const COMMON_DEEPSEEK_MODELS: &[&str] = &[
    "deepseek-v4-pro",
    "deepseek-v4-flash",
    "deepseek-ai/deepseek-v4-pro",
    "deepseek-ai/deepseek-v4-flash",
    "deepseek/deepseek-v4-pro",
    "deepseek/deepseek-v4-flash",
];

pub(super) const API_KEYRING_SENTINEL: &str = "__KEYRING__";

mod effective;
mod load;
mod providers;
mod types;
mod windows_sandbox;

pub use effective::{
    EffectiveLhtComposerMode, config_effective_view, resolve_effective_config,
    resolve_lht_composer_mode,
};
pub use load::*;
pub use providers::*;
pub use types::*;
#[cfg(windows)]
pub use windows_sandbox::{
    effective_windows_sandbox_execution_label, effective_windows_sandbox_execution_mode,
    exec_shell_sandbox_env_marker, windows_sandbox_configured_label,
};
pub use windows_sandbox::{
    parse_windows_sandbox_mode, resolve_windows_sandbox_mode,
    resolve_windows_sandbox_private_desktop,
};
