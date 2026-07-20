use std::fmt::Write;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde_json::json;

use crate::audit::log_sensitive_event;

use super::super::API_KEYRING_SENTINEL;
use super::super::providers::ApiProvider;
use super::super::types::Config;
use super::paths::{default_config_path, ensure_parent_dir, write_config_file_secure};

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
    /// label is the value of [`zagens_secrets::Secrets::backend_name`]
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
/// **Dual-write strategy (#593):** writes to `~/.zagens/config.toml`
/// (always) and to the OS keyring via [`zagens_secrets::Secrets`]
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
        let secrets = zagens_secrets::Secrets::auto_detect();
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
pub(crate) fn save_api_key_to_config_file(api_key: &str) -> Result<PathBuf> {
    fn is_api_key_assignment(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed
            .strip_prefix("api_key")
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    }

    let config_path = default_config_path()
        .context("Failed to resolve config path: home directory not found.")?;

    ensure_parent_dir(&config_path)?;

    if !config_path.exists() {
        super::paths::ensure_config_file_exists(Some(config_path.clone()))?;
    }

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
        anyhow::bail!(
            "config file missing after ensure_default_on_disk: {}",
            config_path.display()
        );
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
/// `~/.zagens/config.toml`.
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
    zagens_secrets::env_for(config.api_provider().keyring_slot()).is_some()
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
    if zagens_secrets::env_for(provider.keyring_slot()).is_some() {
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
/// `[providers.<name>] api_key = "..."` to `~/.zagens/config.toml`.
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
        ApiProvider::Openai => "providers.openai",
        ApiProvider::Openrouter => "providers.openrouter",
        ApiProvider::Novita => "providers.novita",
        ApiProvider::Fireworks => "providers.fireworks",
        ApiProvider::Sglang => "providers.sglang",
        ApiProvider::Vllm => "providers.vllm",
        ApiProvider::Ollama => "providers.ollama",
        ApiProvider::Agnes => "providers.agnes",
        ApiProvider::SenseNova => "providers.sensenova",
        ApiProvider::Moonshot => "providers.moonshot",
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
        ApiProvider::Openai => "openai",
        ApiProvider::Openrouter => "openrouter",
        ApiProvider::Novita => "novita",
        ApiProvider::Fireworks => "fireworks",
        ApiProvider::Sglang => "sglang",
        ApiProvider::Vllm => "vllm",
        ApiProvider::Ollama => "ollama",
        ApiProvider::Agnes => "agnes",
        ApiProvider::SenseNova => "sensenova",
        ApiProvider::Moonshot => "moonshot",
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
