//! Configuration loading and defaults for DeepSeek TUI.

pub const DEFAULT_MAX_SUBAGENTS: usize = 10;
pub const MAX_SUBAGENTS: usize = 20;

mod generated;
pub use generated::{
    DEFAULT_AGNES_BASE_URL, DEFAULT_AGNES_MODEL, DEFAULT_DEEPSEEK_BASE_URL,
    DEFAULT_DEEPSEEK_CN_BASE_URL, DEFAULT_DEEPSEEK_CN_MODEL, DEFAULT_DEEPSEEK_MODEL,
    DEFAULT_FIREWORKS_BASE_URL, DEFAULT_FIREWORKS_MODEL, DEFAULT_MOONSHOT_BASE_URL,
    DEFAULT_MOONSHOT_MODEL, DEFAULT_NOVITA_BASE_URL, DEFAULT_NOVITA_FLASH_MODEL,
    DEFAULT_NOVITA_MODEL, DEFAULT_NVIDIA_NIM_BASE_URL, DEFAULT_NVIDIA_NIM_FLASH_MODEL,
    DEFAULT_NVIDIA_NIM_MODEL, DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL,
    DEFAULT_OPENAI_BASE_URL, DEFAULT_OPENAI_MODEL, DEFAULT_OPENROUTER_BASE_URL,
    DEFAULT_OPENROUTER_FLASH_MODEL, DEFAULT_OPENROUTER_MODEL, DEFAULT_SENSENOVA_BASE_URL,
    DEFAULT_SENSENOVA_MODEL, DEFAULT_SGLANG_BASE_URL, DEFAULT_SGLANG_FLASH_MODEL,
    DEFAULT_SGLANG_MODEL, DEFAULT_VLLM_BASE_URL, DEFAULT_VLLM_FLASH_MODEL, DEFAULT_VLLM_MODEL,
};

/// Default text model — same SSOT as `DEFAULT_DEEPSEEK_MODEL` from providers.toml.
pub const DEFAULT_TEXT_MODEL: &str = DEFAULT_DEEPSEEK_MODEL;

/// Legacy spelling retained for call sites (`DeepseekCN` base URL).
pub const DEFAULT_DEEPSEEKCN_BASE_URL: &str = DEFAULT_DEEPSEEK_CN_BASE_URL;

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
