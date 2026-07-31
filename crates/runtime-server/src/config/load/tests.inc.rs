use crate::config::*;
use crate::test_support::lock_test_env;
use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::env;
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

struct EnvGuard {
    home: Option<OsString>,
    userprofile: Option<OsString>,
    zagens_config_path: Option<OsString>,
    legacy_config_path: Option<OsString>,
    deepseek_provider: Option<OsString>,
    deepseek_api_key: Option<OsString>,
    deepseek_base_url: Option<OsString>,
    deepseek_http_headers: Option<OsString>,
    deepseek_model: Option<OsString>,
    deepseek_default_text_model: Option<OsString>,
    nvidia_api_key: Option<OsString>,
    nvidia_nim_api_key: Option<OsString>,
    nim_base_url: Option<OsString>,
    nvidia_base_url: Option<OsString>,
    nvidia_nim_base_url: Option<OsString>,
    nvidia_nim_model: Option<OsString>,
    openrouter_api_key: Option<OsString>,
    openrouter_base_url: Option<OsString>,
    novita_api_key: Option<OsString>,
    novita_base_url: Option<OsString>,
    fireworks_api_key: Option<OsString>,
    fireworks_base_url: Option<OsString>,
    sglang_api_key: Option<OsString>,
    sglang_base_url: Option<OsString>,
    sglang_model: Option<OsString>,
    vllm_api_key: Option<OsString>,
    vllm_base_url: Option<OsString>,
    vllm_model: Option<OsString>,
    ollama_api_key: Option<OsString>,
    ollama_base_url: Option<OsString>,
    ollama_model: Option<OsString>,
}

impl EnvGuard {
    fn new(home: &Path) -> Self {
        let home_str = OsString::from(home.as_os_str());
        let config_path = home.join(".zagens").join("config.toml");
        let config_str = OsString::from(config_path.as_os_str());
        let home_prev = env::var_os("HOME");
        let userprofile_prev = env::var_os("USERPROFILE");
        let legacy_config_prev = env::var_os("DEEPSEEK_CONFIG_PATH");
        let zagens_config_prev = env::var_os("ZAGENS_CONFIG_PATH");
        let deepseek_provider_prev = env::var_os("DEEPSEEK_PROVIDER");
        let api_key_prev = env::var_os("DEEPSEEK_API_KEY");
        let base_url_prev = env::var_os("DEEPSEEK_BASE_URL");
        let http_headers_prev = env::var_os("DEEPSEEK_HTTP_HEADERS");
        let model_prev = env::var_os("DEEPSEEK_MODEL");
        let default_text_model_prev = env::var_os("DEEPSEEK_DEFAULT_TEXT_MODEL");
        let nvidia_api_key_prev = env::var_os("NVIDIA_API_KEY");
        let nvidia_nim_api_key_prev = env::var_os("NVIDIA_NIM_API_KEY");
        let nim_base_url_prev = env::var_os("NIM_BASE_URL");
        let nvidia_base_url_prev = env::var_os("NVIDIA_BASE_URL");
        let nvidia_nim_base_url_prev = env::var_os("NVIDIA_NIM_BASE_URL");
        let nvidia_nim_model_prev = env::var_os("NVIDIA_NIM_MODEL");
        let openrouter_api_key_prev = env::var_os("OPENROUTER_API_KEY");
        let openrouter_base_url_prev = env::var_os("OPENROUTER_BASE_URL");
        let novita_api_key_prev = env::var_os("NOVITA_API_KEY");
        let novita_base_url_prev = env::var_os("NOVITA_BASE_URL");
        let fireworks_api_key_prev = env::var_os("FIREWORKS_API_KEY");
        let fireworks_base_url_prev = env::var_os("FIREWORKS_BASE_URL");
        let sglang_api_key_prev = env::var_os("SGLANG_API_KEY");
        let sglang_base_url_prev = env::var_os("SGLANG_BASE_URL");
        let sglang_model_prev = env::var_os("SGLANG_MODEL");
        let vllm_api_key_prev = env::var_os("VLLM_API_KEY");
        let vllm_base_url_prev = env::var_os("VLLM_BASE_URL");
        let vllm_model_prev = env::var_os("VLLM_MODEL");
        let ollama_api_key_prev = env::var_os("OLLAMA_API_KEY");
        let ollama_base_url_prev = env::var_os("OLLAMA_BASE_URL");
        let ollama_model_prev = env::var_os("OLLAMA_MODEL");
        // Safety: test-only environment mutation guarded by a global mutex.
        unsafe {
            env::set_var("HOME", &home_str);
            env::set_var("USERPROFILE", &home_str);
            env::set_var("ZAGENS_CONFIG_PATH", &config_str);
            env::set_var("DEEPSEEK_CONFIG_PATH", &config_str);
            env::remove_var("DEEPSEEK_PROVIDER");
            env::remove_var("DEEPSEEK_API_KEY");
            env::remove_var("DEEPSEEK_BASE_URL");
            env::remove_var("DEEPSEEK_HTTP_HEADERS");
            env::remove_var("DEEPSEEK_MODEL");
            env::remove_var("DEEPSEEK_DEFAULT_TEXT_MODEL");
            env::remove_var("NVIDIA_API_KEY");
            env::remove_var("NVIDIA_NIM_API_KEY");
            env::remove_var("NIM_BASE_URL");
            env::remove_var("NVIDIA_BASE_URL");
            env::remove_var("NVIDIA_NIM_BASE_URL");
            env::remove_var("NVIDIA_NIM_MODEL");
            env::remove_var("OPENROUTER_API_KEY");
            env::remove_var("OPENROUTER_BASE_URL");
            env::remove_var("NOVITA_API_KEY");
            env::remove_var("NOVITA_BASE_URL");
            env::remove_var("FIREWORKS_API_KEY");
            env::remove_var("FIREWORKS_BASE_URL");
            env::remove_var("SGLANG_API_KEY");
            env::remove_var("SGLANG_BASE_URL");
            env::remove_var("SGLANG_MODEL");
            env::remove_var("VLLM_API_KEY");
            env::remove_var("VLLM_BASE_URL");
            env::remove_var("VLLM_MODEL");
            env::remove_var("OLLAMA_API_KEY");
            env::remove_var("OLLAMA_BASE_URL");
            env::remove_var("OLLAMA_MODEL");
        }
        Self {
            home: home_prev,
            userprofile: userprofile_prev,
            zagens_config_path: zagens_config_prev,
            legacy_config_path: legacy_config_prev,
            deepseek_provider: deepseek_provider_prev,
            deepseek_api_key: api_key_prev,
            deepseek_base_url: base_url_prev,
            deepseek_http_headers: http_headers_prev,
            deepseek_model: model_prev,
            deepseek_default_text_model: default_text_model_prev,
            nvidia_api_key: nvidia_api_key_prev,
            nvidia_nim_api_key: nvidia_nim_api_key_prev,
            nim_base_url: nim_base_url_prev,
            nvidia_base_url: nvidia_base_url_prev,
            nvidia_nim_base_url: nvidia_nim_base_url_prev,
            nvidia_nim_model: nvidia_nim_model_prev,
            openrouter_api_key: openrouter_api_key_prev,
            openrouter_base_url: openrouter_base_url_prev,
            novita_api_key: novita_api_key_prev,
            novita_base_url: novita_base_url_prev,
            fireworks_api_key: fireworks_api_key_prev,
            fireworks_base_url: fireworks_base_url_prev,
            sglang_api_key: sglang_api_key_prev,
            sglang_base_url: sglang_base_url_prev,
            sglang_model: sglang_model_prev,
            vllm_api_key: vllm_api_key_prev,
            vllm_base_url: vllm_base_url_prev,
            vllm_model: vllm_model_prev,
            ollama_api_key: ollama_api_key_prev,
            ollama_base_url: ollama_base_url_prev,
            ollama_model: ollama_model_prev,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        // Safety: test-only environment mutation guarded by a global mutex.
        unsafe {
            Self::restore_var("HOME", self.home.take());
            Self::restore_var("USERPROFILE", self.userprofile.take());
            Self::restore_var("DEEPSEEK_CONFIG_PATH", self.legacy_config_path.take());
            Self::restore_var("ZAGENS_CONFIG_PATH", self.zagens_config_path.take());
            Self::restore_var("DEEPSEEK_PROVIDER", self.deepseek_provider.take());
            Self::restore_var("DEEPSEEK_API_KEY", self.deepseek_api_key.take());
            Self::restore_var("DEEPSEEK_BASE_URL", self.deepseek_base_url.take());
            Self::restore_var("DEEPSEEK_HTTP_HEADERS", self.deepseek_http_headers.take());
            Self::restore_var("DEEPSEEK_MODEL", self.deepseek_model.take());
            Self::restore_var(
                "DEEPSEEK_DEFAULT_TEXT_MODEL",
                self.deepseek_default_text_model.take(),
            );
            Self::restore_var("NVIDIA_API_KEY", self.nvidia_api_key.take());
            Self::restore_var("NVIDIA_NIM_API_KEY", self.nvidia_nim_api_key.take());
            Self::restore_var("NIM_BASE_URL", self.nim_base_url.take());
            Self::restore_var("NVIDIA_BASE_URL", self.nvidia_base_url.take());
            Self::restore_var("NVIDIA_NIM_BASE_URL", self.nvidia_nim_base_url.take());
            Self::restore_var("NVIDIA_NIM_MODEL", self.nvidia_nim_model.take());
            Self::restore_var("OPENROUTER_API_KEY", self.openrouter_api_key.take());
            Self::restore_var("OPENROUTER_BASE_URL", self.openrouter_base_url.take());
            Self::restore_var("NOVITA_API_KEY", self.novita_api_key.take());
            Self::restore_var("NOVITA_BASE_URL", self.novita_base_url.take());
            Self::restore_var("FIREWORKS_API_KEY", self.fireworks_api_key.take());
            Self::restore_var("FIREWORKS_BASE_URL", self.fireworks_base_url.take());
            Self::restore_var("SGLANG_API_KEY", self.sglang_api_key.take());
            Self::restore_var("SGLANG_BASE_URL", self.sglang_base_url.take());
            Self::restore_var("SGLANG_MODEL", self.sglang_model.take());
            Self::restore_var("VLLM_API_KEY", self.vllm_api_key.take());
            Self::restore_var("VLLM_BASE_URL", self.vllm_base_url.take());
            Self::restore_var("VLLM_MODEL", self.vllm_model.take());
            Self::restore_var("OLLAMA_API_KEY", self.ollama_api_key.take());
            Self::restore_var("OLLAMA_BASE_URL", self.ollama_base_url.take());
            Self::restore_var("OLLAMA_MODEL", self.ollama_model.take());
        }
    }
}

impl EnvGuard {
    /// Restore an env var to its prior value (or remove it if it was unset).
    ///
    /// # Safety
    /// Must only be called from test code guarded by a global mutex.
    unsafe fn restore_var(key: &str, prev: Option<OsString>) {
        if let Some(value) = prev {
            unsafe { env::set_var(key, value) };
        } else {
            unsafe { env::remove_var(key) };
        }
    }
}

#[test]
fn max_subagents_defaults_to_ten() {
    assert_eq!(Config::default().max_subagents(), DEFAULT_MAX_SUBAGENTS);
    assert_eq!(DEFAULT_MAX_SUBAGENTS, 10);
}

#[test]
fn subagent_step_timeout_reads_subagents_table() {
    let config = Config {
        subagents: Some(SubagentsConfig {
            step_timeout_secs: Some(300),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(config.subagent_step_timeout().as_secs(), 300);
    assert_eq!(config.subagent_step_timeout_ms(), 300_000);
    let clamped = Config {
        subagents: Some(SubagentsConfig {
            step_timeout_secs: Some(9999),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(
        clamped.subagent_step_timeout().as_secs(),
        MAX_SUBAGENT_STEP_TIMEOUT_SECS
    );
}

#[test]
fn subagent_heartbeat_timeout_reads_subagents_table() {
    let config = Config {
        subagents: Some(SubagentsConfig {
            heartbeat_timeout_secs: Some(120),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(config.subagent_heartbeat_timeout().as_secs(), 120);
    let clamped = Config {
        subagents: Some(SubagentsConfig {
            heartbeat_timeout_secs: Some(10),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(
        clamped.subagent_heartbeat_timeout().as_secs(),
        MIN_SUBAGENT_HEARTBEAT_TIMEOUT_SECS
    );
}

#[test]
fn subagents_max_concurrent_overrides_top_level_cap() {
    let config = Config {
        max_subagents: Some(3),
        subagents: Some(SubagentsConfig {
            max_concurrent: Some(12),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };

    assert_eq!(config.max_subagents(), 12);
}

#[test]
fn max_subagents_clamps_subagents_max_concurrent() {
    let low = Config {
        subagents: Some(SubagentsConfig {
            max_concurrent: Some(0),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(low.max_subagents(), 1);

    let high = Config {
        subagents: Some(SubagentsConfig {
            max_concurrent: Some(MAX_SUBAGENTS + 10),
            ..SubagentsConfig::default()
        }),
        ..Config::default()
    };
    assert_eq!(high.max_subagents(), MAX_SUBAGENTS);
}

#[test]
fn save_api_key_writes_config_file_under_cfg_test() -> Result<()> {
    // `save_api_key` writes to the shared user config file. This
    // pins the boring v0.8.8 setup path and avoids platform
    // credential prompts during onboarding.
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let saved = save_api_key("test-key")?;
    let expected = temp_root.join(".zagens").join("config.toml");
    assert_eq!(saved, SavedCredential::ConfigFile(expected.clone()));
    assert_eq!(saved.describe(), expected.display().to_string());

    let contents = fs::read_to_string(&expected)?;
    assert!(contents.contains("api_key = \""));

    #[cfg(unix)]
    {
        assert_eq!(fs::metadata(&expected)?.permissions().mode() & 0o777, 0o600);
        let parent = expected.parent().expect("config has parent dir");
        assert_eq!(fs::metadata(parent)?.permissions().mode() & 0o077, 0);

        fs::set_permissions(&expected, fs::Permissions::from_mode(0o644))?;
        save_api_key("second-test-key")?;
        assert_eq!(fs::metadata(&expected)?.permissions().mode() & 0o777, 0o600);
    }
    Ok(())
}

#[test]
fn ensure_config_file_exists_creates_first_run_template() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-first-run-config-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let created = ensure_config_file_exists(None)?.expect("should create config");
    let content = fs::read_to_string(&created)?;

    assert_eq!(created, temp_root.join(".zagens").join("config.toml"));
    assert!(content.contains("default_text_model = \"deepseek-v4-pro\""));
    assert!(content.contains("reasoning_effort = \"max\""));
    assert!(content.contains("skills_dir = \"~/.zagens/skills\""));
    assert!(!content.contains("api_key ="));
    assert!(ensure_config_file_exists(None)?.is_none());
    Ok(())
}

#[test]
fn workspace_trust_round_trips_through_global_config() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-workspace-trust-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);
    let workspace = temp_root.join("project");
    fs::create_dir_all(&workspace)?;

    assert!(!is_workspace_trusted(&workspace));
    let saved = save_workspace_trust(&workspace)?;

    assert_eq!(saved, temp_root.join(".zagens").join("config.toml"));
    assert!(is_workspace_trusted(&workspace));
    assert!(is_workspace_trusted(&workspace));
    assert!(
        !workspace.join(".zagens").exists(),
        "trust persistence must not create a project-local .zagens directory"
    );

    let parsed: toml::Value = toml::from_str(&fs::read_to_string(saved)?)?;
    assert_eq!(
        workspace_trust_level_from_doc(&parsed, &workspace),
        Some("trusted")
    );
    Ok(())
}

#[test]
fn workspace_trust_reads_existing_projects_table() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-existing-project-trust-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);
    let workspace = temp_root.join("project");
    fs::create_dir_all(&workspace)?;
    let config_path = temp_root.join(".deepseek").join("config.toml");
    fs::create_dir_all(config_path.parent().unwrap())?;
    fs::write(
        &config_path,
        format!(
            "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            workspace_config_key(&workspace)
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
        ),
    )?;

    assert!(is_workspace_trusted(&workspace));
    assert!(is_workspace_trusted(&workspace));
    Ok(())
}

#[test]
fn save_api_key_rejects_empty_input() {
    let _lock = lock_test_env();
    let err = save_api_key("   ").expect_err("empty should bail");
    assert!(
        err.to_string().contains("empty"),
        "expected error to mention empty, got: {err}"
    );
}

#[test]
fn saved_credential_describe_returns_config_file_path() {
    let cf = SavedCredential::ConfigFile(PathBuf::from("/tmp/x.toml"));
    assert_eq!(cf.describe(), "/tmp/x.toml");
}

/// #593: the dual-write outcome describes both targets so the
/// onboarding toast (`API key saved to {describe}`) tells the user
/// the key landed in *both* the keyring and the config file 鈥?    /// which is the whole point of the fix (defeats stale-keyring
/// shadow while keeping the config file inspectable).
#[test]
fn saved_credential_describe_lists_both_targets_for_keyring_and_config() {
    let dual = SavedCredential::KeyringAndConfigFile {
        backend: "system keyring".to_string(),
        path: PathBuf::from("/tmp/x.toml"),
    };
    assert_eq!(
        dual.describe(),
        "OS keyring (system keyring) and /tmp/x.toml"
    );
}

#[test]
fn has_api_key_detects_in_memory_override_and_env_var() -> Result<()> {
    // Pins the v0.8.8 contract: `has_api_key` covers the prompt-free
    // sources used by `Config::deepseek_api_key` (in-memory override,
    // env var, config-file slot).
    let _lock = lock_test_env();
    // Explicit in-memory key wins over every other source per
    // `Config::deepseek_api_key`'s "Path 0" override.
    let cfg = Config {
        api_key: Some("sk-in-memory-override".to_string()),
        ..Default::default()
    };
    assert!(
        has_api_key(&cfg),
        "in-memory override must be detected as a usable key"
    );

    // Env var path.
    let env_cfg = Config::default();
    unsafe {
        std::env::set_var("DEEPSEEK_API_KEY", "sk-test-from-env");
    }
    assert!(
        has_api_key(&env_cfg),
        "env-var key must be detected even with empty config"
    );
    unsafe {
        std::env::remove_var("DEEPSEEK_API_KEY");
    }
    Ok(())
}

/// Regression for #343: clear_api_key strips both the root `api_key`
/// and any nested `[providers.<name>].api_key` lines from config.toml
/// so a stale credential can't shadow a fresh login.
#[test]
fn clear_api_key_strips_root_and_provider_scoped_keys() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-clear-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_dir = temp_root.join(".deepseek");
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.toml");
    fs::write(
        &config_path,
        r#"api_key = "old-root-key"
default_text_model = "deepseek-v4-flash"

[providers.deepseek]
api_key = "old-provider-key"
base_url = "https://api.deepseek.com"

[providers.openrouter]
api_key = "old-openrouter-key"
"#,
    )?;

    clear_api_key()?;

    let after = fs::read_to_string(&config_path)?;
    assert!(
        !after.contains("old-root-key"),
        "root api_key must be stripped: {after}"
    );
    assert!(
        !after.contains("old-provider-key"),
        "provider-scoped deepseek key must be stripped: {after}"
    );
    assert!(
        !after.contains("old-openrouter-key"),
        "provider-scoped openrouter key must be stripped: {after}"
    );
    // Non-credential lines must survive.
    assert!(after.contains("default_text_model"));
    assert!(after.contains("base_url"));
    Ok(())
}

/// Regression for #343: explicit in-memory `api_key` (non-empty,
/// non-sentinel) wins over env/config so a freshly-typed onboarding
/// key takes effect immediately.
#[test]
fn deepseek_api_key_prefers_explicit_in_memory_override() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-override-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        api_key: Some("freshly-typed-key".to_string()),
        ..Config::default()
    };
    let resolved = config
        .deepseek_api_key()
        .expect("explicit override must resolve");
    assert_eq!(resolved, "freshly-typed-key");
    Ok(())
}

#[test]
fn deepseek_api_key_prefers_saved_config_over_stale_env() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-config-over-env-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    unsafe {
        env::set_var("DEEPSEEK_API_KEY", "stale-env-key");
    }
    let config = Config {
        api_key: Some("fresh-config-key".to_string()),
        ..Config::default()
    };
    assert_eq!(config.deepseek_api_key()?, "fresh-config-key");
    unsafe {
        env::remove_var("DEEPSEEK_API_KEY");
    }
    Ok(())
}

#[test]
fn active_provider_detects_env_only_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let temp_root =
        env::temp_dir().join(format!("deepseek-tui-env-only-key-{}", std::process::id()));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    unsafe {
        env::set_var("DEEPSEEK_API_KEY", "env-only-key");
    }
    let mut config = Config::default();
    assert!(active_provider_has_env_api_key(&config));
    assert!(!active_provider_has_config_api_key(&config));
    assert!(active_provider_uses_env_only_api_key(&config));

    config.api_key = Some("config-key".to_string());
    assert!(active_provider_has_config_api_key(&config));
    assert!(!active_provider_uses_env_only_api_key(&config));

    unsafe {
        env::remove_var("DEEPSEEK_API_KEY");
    }
    Ok(())
}

#[test]
fn deepseek_api_key_ignores_sentinel_placeholder() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-sentinel-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        api_key: Some(API_KEYRING_SENTINEL.to_string()),
        ..Config::default()
    };
    // Sentinel must not be treated as a real key 鈥?the resolver should
    // fall through to env / config-provider and ultimately bail out
    // with a "key not found" error.
    let _err = config
        .deepseek_api_key()
        .expect_err("sentinel placeholder must not satisfy the API key check");
    Ok(())
}

#[test]
fn test_tilde_expansion_in_paths() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-tilde-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        skills_dir: Some("~/.deepseek/skills".to_string()),
        ..Default::default()
    };
    let expected_skills = temp_root.join(".deepseek").join("skills");
    let actual_skills = config.skills_dir();
    assert_eq!(
        actual_skills.components().collect::<Vec<_>>(),
        expected_skills.components().collect::<Vec<_>>()
    );

    Ok(())
}

#[test]
fn test_load_uses_tilde_expanded_zagens_config_path() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-load-tilde-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".custom-deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(&config_path, "api_key = \"test-key\"\n")?;

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::remove_var("ZAGENS_CONFIG_PATH");
        env::set_var("DEEPSEEK_CONFIG_PATH", "~/.custom-deepseek/config.toml");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_key.as_deref(), Some("test-key"));
    Ok(())
}

#[test]
fn test_load_falls_back_to_home_config_when_env_path_missing() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-load-fallback-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let home_config = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&home_config)?;
    fs::write(&home_config, "api_key = \"home-key\"\n")?;

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var(
            "DEEPSEEK_CONFIG_PATH",
            temp_root.join("missing-config.toml").as_os_str(),
        );
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_key.as_deref(), Some("home-key"));
    Ok(())
}

#[test]
fn test_nonexistent_profile_error() {
    let mut profiles = HashMap::new();
    profiles.insert("work".to_string(), Config::default());
    let config = ConfigFile {
        base: Config::default(),
        profiles: Some(profiles),
    };

    let err = apply_profile(config, Some("nonexistent")).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("Profile 'nonexistent' not found"));
    assert!(message.contains("Available profiles"));
    assert!(message.contains("work"));
}

#[test]
fn test_profile_with_no_profiles_section() {
    let config = ConfigFile {
        base: Config::default(),
        profiles: None,
    };

    let err = apply_profile(config, Some("missing")).unwrap_err();
    assert!(err.to_string().contains("Available profiles: none"));
}

#[test]
fn test_save_api_key_doesnt_match_similar_keys() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-api-key-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        "api_key_backup = \"old\"\napi_key = \"current\"\n",
    )?;

    let saved = save_api_key("new-key")?;
    assert_eq!(saved, SavedCredential::ConfigFile(config_path.clone()));

    let contents = fs::read_to_string(&config_path)?;
    assert!(contents.contains("api_key_backup = \"old\""));
    assert!(contents.contains("api_key = \""));
    Ok(())
}

#[test]
fn test_empty_api_key_rejected() {
    let config = Config {
        api_key: Some("   ".to_string()),
        ..Default::default()
    };
    assert!(config.validate().is_err());
}

#[test]
fn test_missing_api_key_allowed() -> Result<()> {
    let config = Config::default();
    config.validate()?;
    Ok(())
}

#[test]
fn apply_env_overrides_ignores_empty_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-empty-key-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Simulate a fresh user who copied .env.example to .env without
    // filling in DEEPSEEK_API_KEY: dotenv loads it as the empty string.
    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_API_KEY", "");
    }

    let mut config = Config {
        api_key: Some("from-config-file".to_string()),
        ..Default::default()
    };
    apply_env_overrides(&mut config);

    assert_eq!(config.api_key.as_deref(), Some("from-config-file"));
    config.validate()?;
    Ok(())
}

#[test]
fn apply_env_overrides_does_not_copy_api_key_into_config() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-env-key-not-config-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    unsafe {
        env::set_var("DEEPSEEK_API_KEY", "env-key");
    }
    let mut config = Config::default();
    apply_env_overrides(&mut config);

    assert_eq!(config.api_key, None);
    assert_eq!(config.deepseek_api_key()?, "env-key");
    unsafe {
        env::remove_var("DEEPSEEK_API_KEY");
    }
    Ok(())
}

#[test]
fn normalize_model_name_preserves_v_series_snapshots() {
    // v4 canonical forms still resolve
    assert_eq!(
        normalize_model_name("deepseek-v4-pro").as_deref(),
        Some("deepseek-v4-pro")
    );
    assert_eq!(
        normalize_model_name("deepseek-v4pro").as_deref(),
        Some("deepseek-v4-pro")
    );
    // Official Flash-0731 version label remaps to the live API id.
    assert_eq!(
        normalize_model_name("deepseek-v4-flash-0731").as_deref(),
        Some("deepseek-v4-flash")
    );
    assert_eq!(
        normalize_model_name("DeepSeek-V4-Flash-0731").as_deref(),
        Some("deepseek-v4-flash")
    );
    // v-series dated snapshots pass through unchanged
    assert_eq!(
        normalize_model_name("deepseek-v4-flash-20260423").as_deref(),
        Some("deepseek-v4-flash-20260423")
    );
    // future v-series identities pass through
    assert_eq!(
        normalize_model_name("deepseek-v5-pro-20270101").as_deref(),
        Some("deepseek-v5-pro-20270101")
    );
    // legacy names pass through unchanged 鈥?server decides
    assert_eq!(
        normalize_model_name("deepseek-chat").as_deref(),
        Some("deepseek-chat")
    );
    // cross-provider names still normalize
    assert_eq!(
        normalize_model_name("deepseek-ai/deepseek-v4-pro").as_deref(),
        Some("deepseek-ai/deepseek-v4-pro")
    );
    // preserve exact case for providers that require case-sensitive model IDs
    assert_eq!(
        normalize_model_name("DeepSeek-V4-Pro").as_deref(),
        Some("DeepSeek-V4-Pro")
    );
    assert_eq!(
        normalize_model_name("deepseek-ai/DeepSeek-V4-Pro").as_deref(),
        Some("deepseek-ai/DeepSeek-V4-Pro")
    );
}

#[test]
fn normalize_model_for_provider_keeps_provider_remaps_when_case_is_preserved() {
    assert_eq!(
        normalize_model_for_provider(ApiProvider::Deepseek, "DeepSeek-V4-Pro").as_deref(),
        Some("DeepSeek-V4-Pro")
    );
    assert_eq!(
        normalize_model_for_provider(ApiProvider::NvidiaNim, "DeepSeek-V4-Pro").as_deref(),
        Some(DEFAULT_NVIDIA_NIM_MODEL)
    );
}

#[test]
fn normalize_model_name_rejects_invalid_or_non_deepseek_ids() {
    assert!(normalize_model_name("gpt-4o").is_none());
    assert!(normalize_model_name("deepseek v4").is_none());
    assert!(normalize_model_name("").is_none());
}

#[test]
fn normalize_model_name_accepts_provider_prefixed_deepseek_ids() {
    assert_eq!(
        normalize_model_name("accounts/fireworks/models/deepseek-v4-flash").as_deref(),
        Some("accounts/fireworks/models/deepseek-v4-flash")
    );
    assert_eq!(
        normalize_model_name("provider/deepseek-ai/deepseek-v4-pro").as_deref(),
        Some("provider/deepseek-ai/deepseek-v4-pro")
    );
}

#[test]
fn default_context_seams_are_opt_in() {
    let config = Config::default();
    assert!(!config.context.enabled.unwrap_or(false));
    assert_eq!(config.context.l1_threshold.unwrap_or(192_000), 192_000);
    assert_eq!(config.context.cycle_threshold.unwrap_or(768_000), 768_000);
    assert_eq!(
        config
            .context
            .seam_model
            .as_deref()
            .unwrap_or("deepseek-v4-flash"),
        "deepseek-v4-flash"
    );
}

#[test]
fn cycle_runtime_config_default_uses_scaled_baseline_for_v4() {
    let config = Config::default();
    assert_eq!(
        config
            .cycle_runtime_config("deepseek-v4-pro")
            .threshold_for("deepseek-v4-pro"),
        750_000
    );
}

#[test]
fn cycle_runtime_config_global_override_rewrites_seeded_models() {
    // The default CycleConfig seeds per-model entries for the V4 models, and
    // `threshold_for` checks per_model first — so a global `[context]
    // cycle_threshold` must also rewrite those seeds or it would be shadowed
    // for exactly the V4 models.
    let config = Config {
        context: ContextConfig {
            cycle_threshold: Some(120_000),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        config
            .cycle_runtime_config("deepseek-v4-pro")
            .threshold_for("deepseek-v4-pro"),
        120_000
    );
    assert_eq!(
        config
            .cycle_runtime_config("some-other-model")
            .threshold_for("some-other-model"),
        120_000
    );
}

#[test]
fn cycle_runtime_config_per_model_override_wins_over_global() {
    let mut per_model = HashMap::new();
    per_model.insert(
        "deepseek-v4-pro".to_string(),
        PerModelContextConfig {
            l1_threshold: None,
            l2_threshold: None,
            l3_threshold: None,
            cycle_threshold: Some(90_000),
        },
    );
    let config = Config {
        context: ContextConfig {
            cycle_threshold: Some(120_000),
            per_model: Some(per_model),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        config
            .cycle_runtime_config("deepseek-v4-pro")
            .threshold_for("deepseek-v4-pro"),
        90_000
    );
    // A model without a per-model entry still gets the global override.
    assert_eq!(
        config
            .cycle_runtime_config("deepseek-v4-flash")
            .threshold_for("deepseek-v4-flash"),
        120_000
    );
}

#[test]
fn profile_without_context_does_not_disable_base_context() {
    let mut profiles = HashMap::new();
    profiles.insert("work".to_string(), Config::default());
    let config = ConfigFile {
        base: Config {
            context: ContextConfig {
                enabled: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
        profiles: Some(profiles),
    };

    let merged = apply_profile(config, Some("work")).expect("profile");
    assert_eq!(merged.context.enabled, Some(true));
}

#[test]
fn validate_accepts_future_deepseek_model_id() -> Result<()> {
    let config = Config {
        default_text_model: Some("deepseek-v4".to_string()),
        ..Default::default()
    };
    config.validate()?;
    Ok(())
}

#[test]
fn validate_accepts_auto_default_text_model() -> Result<()> {
    let config = Config {
        default_text_model: Some("auto".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.default_model(), "auto");
    Ok(())
}

#[test]
fn deepseek_provider_defaults_to_beta_endpoint() {
    let config = Config::default();

    assert_eq!(config.api_provider(), ApiProvider::Deepseek);
    assert_eq!(config.deepseek_base_url(), DEFAULT_DEEPSEEK_BASE_URL);
}

#[test]
fn explicit_deepseek_base_url_overrides_beta_default() {
    let config = Config {
        base_url: Some("https://api.deepseek.com".to_string()),
        ..Default::default()
    };

    assert_eq!(config.api_provider(), ApiProvider::Deepseek);
    assert_eq!(config.deepseek_base_url(), "https://api.deepseek.com");
}

#[test]
fn deepseek_model_env_overrides_default_text_model() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-model-env-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_MODEL", "deepseek-v4-flash-20260423");
    }

    let config = Config::load(None, None)?;
    // v-series snapshots pass through unchanged 鈥?no alias folding
    assert_eq!(
        config.default_text_model.as_deref(),
        Some("deepseek-v4-flash-20260423")
    );
    Ok(())
}

#[test]
fn http_headers_load_from_root_config() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-http-headers-root-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"
api_key = "test-key"
http_headers = { "X-Model-Provider-Id" = "tongyi" }
"#,
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(
        config
            .http_headers()
            .get("X-Model-Provider-Id")
            .map(String::as_str),
        Some("tongyi")
    );
    Ok(())
}

#[test]
fn provider_http_headers_extend_and_override_root_config() {
    let mut providers = ProvidersConfig::default();
    providers.deepseek.http_headers = Some(HashMap::from([
        ("X-Model-Provider-Id".to_string(), "tongyi".to_string()),
        ("X-Shared".to_string(), "provider".to_string()),
    ]));
    let config = Config {
        http_headers: Some(HashMap::from([
            ("X-Root".to_string(), "root".to_string()),
            ("X-Shared".to_string(), "root".to_string()),
        ])),
        providers: Some(providers),
        ..Default::default()
    };

    let headers = config.http_headers();
    assert_eq!(
        headers.get("X-Model-Provider-Id").map(String::as_str),
        Some("tongyi")
    );
    assert_eq!(headers.get("X-Root").map(String::as_str), Some("root"));
    assert_eq!(
        headers.get("X-Shared").map(String::as_str),
        Some("provider")
    );
}

#[test]
fn http_headers_env_overrides_config() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-http-headers-env-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"
api_key = "test-key"
http_headers = { "X-Model-Provider-Id" = "from-file" }
"#,
    )?;
    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_HTTP_HEADERS", "X-Model-Provider-Id=from-env");
    }

    let config = Config::load(None, None)?;
    assert_eq!(
        config
            .http_headers()
            .get("X-Model-Provider-Id")
            .map(String::as_str),
        Some("from-env")
    );
    Ok(())
}

#[test]
fn nvidia_nim_provider_uses_nim_defaults() -> Result<()> {
    let config = Config {
        provider: Some("nvidia-nim".to_string()),
        ..Default::default()
    };

    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(config.default_model(), DEFAULT_NVIDIA_NIM_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_NVIDIA_NIM_BASE_URL);
    Ok(())
}

#[test]
fn nvidia_nim_provider_normalizes_deepseek_v4_pro_alias() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-nim-model-alias-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        "provider = \"nvidia-nim\"\ndefault_text_model = \"deepseek-v4-pro\"\napi_key = \"nim-key\"\n",
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(
        config.default_text_model.as_deref(),
        Some(DEFAULT_NVIDIA_NIM_MODEL)
    );
    Ok(())
}

#[test]
fn nvidia_nim_provider_normalizes_deepseek_v4_flash_alias() -> Result<()> {
    let config = Config {
        provider: Some("nvidia-nim".to_string()),
        default_text_model: Some("deepseek-v4-flash".to_string()),
        ..Default::default()
    };

    config.validate()?;
    assert_eq!(config.default_model(), DEFAULT_NVIDIA_NIM_FLASH_MODEL);
    Ok(())
}

#[test]
fn nvidia_nim_env_overrides_provider_and_credentials() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-nim-env-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "nvidia-nim");
        env::set_var("NVIDIA_API_KEY", "nim-env-key");
        env::set_var("NVIDIA_NIM_MODEL", "deepseek-ai/deepseek-v4-pro");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(config.deepseek_api_key()?, "nim-env-key");
    assert_eq!(config.default_model(), DEFAULT_NVIDIA_NIM_MODEL);
    Ok(())
}

#[test]
fn nvidia_nim_env_accepts_short_nim_base_url_alias() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-nim-base-url-alias-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "nvidia-nim");
        env::set_var("NIM_BASE_URL", "https://short-nim.example/v1");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(config.deepseek_base_url(), "https://short-nim.example/v1");
    Ok(())
}

#[test]
fn nvidia_nim_env_accepts_facade_base_url_forwarding() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-nim-forwarded-base-url-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "nvidia-nim");
        env::set_var("DEEPSEEK_BASE_URL", "https://forwarded-nim.example/v1");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(
        config.deepseek_base_url(),
        "https://forwarded-nim.example/v1"
    );
    Ok(())
}

#[test]
fn openrouter_provider_uses_canonical_defaults() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-or-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("openrouter".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Openrouter);
    assert_eq!(config.default_model(), DEFAULT_OPENROUTER_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_OPENROUTER_BASE_URL);
    Ok(())
}

#[test]
fn openrouter_custom_model_passes_through() -> Result<()> {
    let config = Config {
        provider: Some("openrouter".to_string()),
        default_text_model: Some("openrouter/owl-alpha".to_string()),
        providers: Some(ProvidersConfig {
            openrouter: ProviderConfig {
                model: Some("openrouter/owl-alpha".to_string()),
                ..Default::default()
            },
            ..Default::default()
        }),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.default_model(), "openrouter/owl-alpha");
    Ok(())
}

#[test]
fn novita_provider_uses_canonical_defaults() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-novita-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("novita".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Novita);
    assert_eq!(config.default_model(), DEFAULT_NOVITA_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_NOVITA_BASE_URL);
    Ok(())
}

#[test]
fn fireworks_provider_uses_canonical_defaults() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-fireworks-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("fireworks".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Fireworks);
    assert_eq!(config.default_model(), DEFAULT_FIREWORKS_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_FIREWORKS_BASE_URL);
    Ok(())
}

/// Kernel-v2 M0.5 regression: `provider = "openai"` previously failed to
/// parse and silently fell back to DeepSeek defaults (base URL + key).
#[test]
fn openai_provider_uses_canonical_defaults() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-openai-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("openai".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Openai);
    assert_eq!(config.default_model(), DEFAULT_OPENAI_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_OPENAI_BASE_URL);
    Ok(())
}

/// OpenAI model identifiers are free-form: the configured model must pass
/// through untouched instead of being DeepSeek-normalized away.
#[test]
fn openai_provider_keeps_free_form_model_names() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-openai-model-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let mut providers = ProvidersConfig::default();
    providers.openai.model = Some("o4-mini".to_string());
    providers.openai.base_url = Some("https://example.com/v1".to_string());
    let config = Config {
        provider: Some("openai".to_string()),
        providers: Some(providers),
        ..Default::default()
    };
    assert_eq!(config.default_model(), "o4-mini");
    assert_eq!(config.deepseek_base_url(), "https://example.com/v1");
    Ok(())
}

#[test]
fn sglang_provider_works_without_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-sglang-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("sglang".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Sglang);
    assert_eq!(config.default_model(), DEFAULT_SGLANG_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_SGLANG_BASE_URL);
    assert_eq!(config.deepseek_api_key()?, "");
    assert!(has_api_key_for(&config, ApiProvider::Sglang));
    Ok(())
}

#[test]
fn ollama_provider_uses_local_defaults_without_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-ollama-defaults-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config = Config {
        provider: Some("ollama".to_string()),
        ..Default::default()
    };
    config.validate()?;
    assert_eq!(config.api_provider(), ApiProvider::Ollama);
    assert_eq!(config.default_model(), DEFAULT_OLLAMA_MODEL);
    assert_eq!(config.deepseek_base_url(), DEFAULT_OLLAMA_BASE_URL);
    assert_eq!(config.deepseek_api_key()?, "");
    assert!(has_api_key_for(&config, ApiProvider::Ollama));
    Ok(())
}

#[test]
fn ollama_model_is_passed_through_verbatim() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-ollama-model-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"provider = "ollama"

[providers.ollama]
base_url = "http://127.0.0.1:11434/v1"
model = "qwen2.5-coder:7b"
"#,
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Ollama);
    assert_eq!(config.default_model(), "qwen2.5-coder:7b");
    assert_eq!(config.deepseek_base_url(), "http://127.0.0.1:11434/v1");
    Ok(())
}

#[test]
fn ollama_env_overrides_base_url_and_model() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-ollama-env-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "ollama-local");
        env::set_var("OLLAMA_BASE_URL", "http://ollama.example/v1");
        env::set_var("OLLAMA_MODEL", "deepseek-coder-v2:16b");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Ollama);
    assert_eq!(config.deepseek_base_url(), "http://ollama.example/v1");
    assert_eq!(config.default_model(), "deepseek-coder-v2:16b");
    Ok(())
}

#[test]
fn openrouter_env_api_key_resolves_via_deepseek_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-or-env-key-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "openrouter");
        env::set_var("OPENROUTER_API_KEY", "or-env-key");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Openrouter);
    assert_eq!(config.deepseek_api_key()?, "or-env-key");
    Ok(())
}

#[test]
fn novita_env_api_key_resolves_via_deepseek_api_key() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-novita-env-key-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "novita");
        env::set_var("NOVITA_API_KEY", "novita-env-key");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Novita);
    assert_eq!(config.deepseek_api_key()?, "novita-env-key");
    Ok(())
}

#[test]
fn openrouter_base_url_env_overrides_default() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-or-base-url-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("DEEPSEEK_PROVIDER", "openrouter");
        env::set_var("OPENROUTER_BASE_URL", "https://or-mirror.example/v1");
    }

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Openrouter);
    assert_eq!(config.deepseek_base_url(), "https://or-mirror.example/v1");
    Ok(())
}

#[test]
fn openrouter_reads_provider_table_from_config_file() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-or-table-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"provider = "openrouter"

[providers.openrouter]
api_key = "or-table-key"
base_url = "https://or-table.example/v1"
"#,
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Openrouter);
    assert_eq!(config.deepseek_api_key()?, "or-table-key");
    assert_eq!(config.deepseek_base_url(), "https://or-table.example/v1");
    Ok(())
}

#[test]
fn novita_reads_provider_table_from_config_file() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-novita-table-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"provider = "novita"

[providers.novita]
api_key = "novita-table-key"
"#,
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::Novita);
    assert_eq!(config.deepseek_api_key()?, "novita-table-key");
    assert_eq!(config.deepseek_base_url(), DEFAULT_NOVITA_BASE_URL);
    Ok(())
}

#[test]
fn has_api_key_for_detects_env_and_config_per_provider() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-has-key-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let mut config = Config::default();
    assert!(!has_api_key_for(&config, ApiProvider::Openrouter));
    assert!(
        has_api_key_for(&config, ApiProvider::Sglang),
        "SGLang is self-hosted and does not require a key by default"
    );
    assert!(
        has_api_key_for(&config, ApiProvider::Vllm),
        "vLLM is self-hosted and does not require a key by default"
    );

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::set_var("OPENROUTER_API_KEY", "or-env");
    }
    assert!(has_api_key_for(&config, ApiProvider::Openrouter));
    assert!(!has_api_key_for(&config, ApiProvider::Novita));

    // Safety: test-only environment mutation guarded by a global mutex.
    unsafe {
        env::remove_var("OPENROUTER_API_KEY");
    }
    let mut providers = ProvidersConfig::default();
    providers.novita.api_key = Some("file-novita".to_string());
    config.providers = Some(providers);
    assert!(has_api_key_for(&config, ApiProvider::Novita));
    assert!(!has_api_key_for(&config, ApiProvider::Openrouter));
    Ok(())
}

#[test]
fn has_api_key_for_uses_deepseek_cn_provider_table() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-has-key-cn-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let mut providers = ProvidersConfig::default();
    providers.deepseek_cn.api_key = Some("cn-file-key".to_string());
    let config = Config {
        providers: Some(providers),
        ..Config::default()
    };

    assert!(has_api_key_for(&config, ApiProvider::DeepseekCN));
    Ok(())
}

#[test]
fn save_api_key_for_openrouter_writes_provider_table() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-save-key-or-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let path = save_api_key_for(ApiProvider::Openrouter, "or-saved-key")?;
    let contents = fs::read_to_string(&path)?;
    let parsed: toml::Value = toml::from_str(&contents)?;
    assert_eq!(
        parsed
            .get("providers")
            .and_then(|p| p.get("openrouter"))
            .and_then(|t| t.get("api_key"))
            .and_then(toml::Value::as_str),
        Some("or-saved-key")
    );
    // Re-saving must not duplicate or wipe sibling tables.
    save_api_key_for(ApiProvider::Novita, "novita-saved-key")?;
    let contents = fs::read_to_string(&path)?;
    let parsed: toml::Value = toml::from_str(&contents)?;
    assert_eq!(
        parsed
            .get("providers")
            .and_then(|p| p.get("openrouter"))
            .and_then(|t| t.get("api_key"))
            .and_then(toml::Value::as_str),
        Some("or-saved-key")
    );
    assert_eq!(
        parsed
            .get("providers")
            .and_then(|p| p.get("novita"))
            .and_then(|t| t.get("api_key"))
            .and_then(toml::Value::as_str),
        Some("novita-saved-key")
    );
    save_api_key_for(ApiProvider::Fireworks, "fireworks-saved-key")?;
    save_api_key_for(ApiProvider::Sglang, "sglang-saved-key")?;
    let contents = fs::read_to_string(&path)?;
    let parsed: toml::Value = toml::from_str(&contents)?;
    assert_eq!(
        parsed
            .get("providers")
            .and_then(|p| p.get("fireworks"))
            .and_then(|t| t.get("api_key"))
            .and_then(toml::Value::as_str),
        Some("fireworks-saved-key")
    );
    assert_eq!(
        parsed
            .get("providers")
            .and_then(|p| p.get("sglang"))
            .and_then(|t| t.get("api_key"))
            .and_then(toml::Value::as_str),
        Some("sglang-saved-key")
    );
    Ok(())
}

#[test]
fn save_api_key_for_deepseek_cn_uses_root_deepseek_storage() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-save-key-cn-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let path = save_api_key_for(ApiProvider::DeepseekCN, "cn-saved-key")?;
    let contents = fs::read_to_string(&path)?;
    let parsed: toml::Value = toml::from_str(&contents)?;

    assert_eq!(
        parsed.get("api_key").and_then(toml::Value::as_str),
        Some("cn-saved-key")
    );
    Ok(())
}

#[test]
fn nvidia_nim_reads_facade_provider_table() -> Result<()> {
    let _lock = lock_test_env();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp_root = env::temp_dir().join(format!(
        "deepseek-tui-nim-provider-table-test-{}-{}",
        std::process::id(),
        nanos
    ));
    fs::create_dir_all(&temp_root)?;
    let _guard = EnvGuard::new(&temp_root);

    let config_path = temp_root.join(".deepseek").join("config.toml");
    ensure_parent_dir(&config_path)?;
    fs::write(
        &config_path,
        r#"provider = "nvidia-nim"
default_text_model = "deepseek-v4-flash"

[providers.nvidia_nim]
api_key = "nim-table-key"
base_url = "https://nim-table.example/v1"
model = "deepseek-v4-pro"
"#,
    )?;

    let config = Config::load(None, None)?;
    assert_eq!(config.api_provider(), ApiProvider::NvidiaNim);
    assert_eq!(config.deepseek_api_key()?, "nim-table-key");
    assert_eq!(config.deepseek_base_url(), "https://nim-table.example/v1");
    assert_eq!(config.default_model(), DEFAULT_NVIDIA_NIM_MODEL);
    Ok(())
}

// ========================================================================
// Provider Capability Matrix tests
// ========================================================================

#[test]
fn provider_capability_deepseek_v4_pro_has_1m_window_and_thinking() {
    let cap = provider_capability(ApiProvider::Deepseek, "deepseek-v4-pro");
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(cap.cache_telemetry_supported);
    assert_eq!(
        cap.request_payload_mode,
        RequestPayloadMode::ChatCompletions
    );
}

#[test]
fn provider_capability_deepseek_v4_flash_has_1m_window_and_thinking() {
    let cap = provider_capability(ApiProvider::Deepseek, "deepseek-v4-flash");
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(cap.cache_telemetry_supported);
}

#[test]
fn provider_capability_nvidia_nim_v4_pro_maps_correctly() {
    let cap = provider_capability(ApiProvider::NvidiaNim, DEFAULT_NVIDIA_NIM_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(cap.cache_telemetry_supported);
    assert_eq!(
        cap.request_payload_mode,
        RequestPayloadMode::ChatCompletions
    );
}

#[test]
fn provider_capability_nvidia_nim_v4_flash_maps_correctly() {
    let cap = provider_capability(ApiProvider::NvidiaNim, DEFAULT_NVIDIA_NIM_FLASH_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(cap.cache_telemetry_supported);
}

#[test]
fn provider_capability_openrouter_v4_pro_has_thinking_no_cache() {
    let cap = provider_capability(ApiProvider::Openrouter, DEFAULT_OPENROUTER_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    // OpenRouter does not return DeepSeek prompt-cache telemetry.
    assert!(!cap.cache_telemetry_supported);
    assert_eq!(
        cap.request_payload_mode,
        RequestPayloadMode::ChatCompletions
    );
}

#[test]
fn provider_capability_novita_v4_pro_has_thinking_no_cache() {
    let cap = provider_capability(ApiProvider::Novita, DEFAULT_NOVITA_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(!cap.cache_telemetry_supported);
}

#[test]
fn provider_capability_fireworks_v4_pro_has_thinking_no_cache() {
    let cap = provider_capability(ApiProvider::Fireworks, DEFAULT_FIREWORKS_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(!cap.cache_telemetry_supported);
}

#[test]
fn provider_capability_sglang_v4_pro_has_thinking_no_cache() {
    let cap = provider_capability(ApiProvider::Sglang, DEFAULT_SGLANG_MODEL);
    assert_eq!(
        cap.context_window,
        crate::models::DEEPSEEK_V4_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, zagens_core::chat::DEEPSEEK_V4_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
    assert!(!cap.cache_telemetry_supported);
}

#[test]
fn provider_capability_ollama_is_openai_compatible_without_thinking() {
    // Local Ollama tag with DeepSeek v3 — legacy family, no thinking.
    let cap = provider_capability(ApiProvider::Ollama, "deepseek-v3.1:671b");
    assert_eq!(
        cap.context_window,
        crate::models::LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, crate::models::DEFAULT_MAX_OUTPUT_TOKENS);
    assert!(!cap.thinking_supported);
    assert!(!cap.cache_telemetry_supported);
    assert_eq!(
        cap.request_payload_mode,
        RequestPayloadMode::ChatCompletions
    );
}

#[test]
fn provider_capability_non_v4_model_has_smaller_window() {
    let cap = provider_capability(ApiProvider::Deepseek, "deepseek-coder");
    assert_eq!(
        cap.context_window,
        crate::models::LEGACY_DEEPSEEK_CONTEXT_WINDOW_TOKENS
    );
    assert_eq!(cap.max_output, crate::models::DEFAULT_MAX_OUTPUT_TOKENS);
    assert!(!cap.thinking_supported);
}

#[test]
fn provider_capability_agnes_chat_uses_catalog_256k() {
    let cap = provider_capability(ApiProvider::Agnes, "agnes-2.0-flash");
    assert_eq!(cap.context_window, 256_000);
    assert_eq!(cap.max_output, crate::models::DEFAULT_MAX_OUTPUT_TOKENS);
    assert!(!cap.thinking_supported);
}

#[test]
fn provider_capability_kimi_k3_always_thinking() {
    let cap = provider_capability(ApiProvider::Moonshot, "kimi-k3");
    assert_eq!(cap.context_window, crate::models::KIMI_K3_CONTEXT_WINDOW_TOKENS);
    assert_eq!(cap.max_output, crate::models::KIMI_K3_MAX_OUTPUT_TOKENS);
    assert!(cap.thinking_supported);
}

#[test]
fn provider_capability_roundtrip_serialization() {
    let cap = provider_capability(ApiProvider::Deepseek, "deepseek-v4-pro");
    let json = serde_json::to_value(&cap).unwrap();
    let deserialized: ProviderCapability = serde_json::from_value(json).unwrap();
    assert_eq!(cap, deserialized);
}

#[test]
fn instructions_paths_auto_discovers_project_rules_and_cursor_mdc() {
    use std::fs;
    use tempfile::tempdir;

    let tmp = tempdir().expect("tempdir");
    let workspace = tmp.path();
    fs::write(workspace.join("PROJECT_RULES.md"), "# rules").unwrap();
    let rules_dir = workspace.join(".cursor").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("security.mdc"), "security").unwrap();
    fs::write(rules_dir.join("alpha.mdc"), "alpha").unwrap();

    let cfg = Config::default();
    let paths = cfg.instructions_paths(workspace);
    assert_eq!(paths.len(), 3);
    assert_eq!(paths[0], workspace.join("PROJECT_RULES.md"));
    assert_eq!(paths[1], rules_dir.join("alpha.mdc"));
    assert_eq!(paths[2], rules_dir.join("security.mdc"));
}

#[test]
fn instructions_paths_explicit_list_skips_auto_discovery() {
    use std::fs;
    use tempfile::tempdir;

    let tmp = tempdir().expect("tempdir");
    let workspace = tmp.path();
    fs::write(workspace.join("PROJECT_RULES.md"), "# rules").unwrap();

    let mut cfg = Config::default();
    cfg.instructions = Some(vec!["custom.md".to_string()]);
    let paths = cfg.instructions_paths(workspace);
    assert_eq!(paths.len(), 1);
    assert!(paths[0].ends_with("custom.md"));
}

#[test]
fn instructions_paths_empty_explicit_list_falls_back_to_auto_discovery() {
    use std::fs;
    use tempfile::tempdir;

    let tmp = tempdir().expect("tempdir");
    let workspace = tmp.path();
    fs::write(workspace.join("PROJECT_RULES.md"), "# rules").unwrap();

    let mut cfg = Config::default();
    cfg.instructions = Some(vec!["".to_string(), "  ".to_string()]);
    let paths = cfg.instructions_paths(workspace);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0], workspace.join("PROJECT_RULES.md"));
}
