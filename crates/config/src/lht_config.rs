//! `[long_horizon]` / `[long_horizon.completion_gate]` on-disk schema for Zagens config.toml.
//!
//! Mirrors `zagens_core::long_horizon` for serde I/O without a core dependency cycle.

use serde::{Deserialize, Serialize};

/// `[long_horizon]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LongHorizonConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_nudges_per_item: Option<u32>,
    #[serde(default)]
    pub blocked_nudges_without_progress: Option<u32>,
    #[serde(default)]
    pub reinject_every_steps: Option<u32>,
    #[serde(default)]
    pub progress_via_git: Option<bool>,
    #[serde(default)]
    pub auto_continue: Option<bool>,
    #[serde(default)]
    pub max_auto_continue_rounds: Option<u32>,
    #[serde(default)]
    pub completion_gate: Option<CompletionGateConfigToml>,
    #[serde(default)]
    pub macro_loop: Option<MacroLoopConfigToml>,
}

/// `[long_horizon.macro_loop]` table (Phase 4 macro review cycle).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MacroLoopConfigToml {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub max_macro_cycles: Option<u32>,
    #[serde(default)]
    pub max_craft_rounds_per_cycle: Option<u32>,
    #[serde(default)]
    pub auto_enter_craft: Option<String>,
    #[serde(default)]
    pub craft_on_small_tasks: Option<bool>,
    #[serde(default)]
    pub min_checklist_items_for_craft: Option<u32>,
}

/// `[long_horizon.completion_gate]` table.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateConfigToml {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub max_manifest_rounds: Option<u32>,
    #[serde(default)]
    pub max_audit_rounds: Option<u32>,
    #[serde(default)]
    pub max_infra_strikes: Option<u32>,
    #[serde(default)]
    pub verify: Vec<CompletionGateVerifyToml>,
    #[serde(default)]
    pub deliverable: Vec<CompletionGateDeliverableToml>,
    #[serde(default)]
    pub auto_verify_replay: Option<String>,
    #[serde(default)]
    pub toolchain_gate: Option<String>,
    #[serde(default)]
    pub stub_gate: Option<String>,
    #[serde(default)]
    pub min_lines: Option<MinLinesGateToml>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MinLinesGateToml {
    #[serde(default)]
    pub frontend: Option<u32>,
    #[serde(default)]
    pub backend: Option<u32>,
    #[serde(default)]
    pub frontend_glob: Option<String>,
    #[serde(default)]
    pub backend_glob: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateVerifyToml {
    pub id: String,
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub argv: Option<Vec<String>>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionGateDeliverableToml {
    pub id: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub glob: Option<String>,
    #[serde(default)]
    pub optional_verify_cmd: Option<String>,
    #[serde(default)]
    pub tracked: Option<bool>,
}

/// Product defaults for first-run `config.toml` and UI fallbacks.
#[must_use]
pub fn product_defaults() -> LongHorizonConfigToml {
    LongHorizonConfigToml {
        enabled: Some(true),
        mode: Some("auto".into()),
        max_nudges_per_item: Some(5),
        blocked_nudges_without_progress: Some(3),
        reinject_every_steps: Some(0),
        progress_via_git: Some(true),
        auto_continue: Some(false),
        max_auto_continue_rounds: Some(16),
        completion_gate: Some(CompletionGateConfigToml {
            auto_verify_replay: Some("observe".into()),
            toolchain_gate: Some("observe".into()),
            stub_gate: Some("observe".into()),
            max_manifest_rounds: Some(5),
            max_audit_rounds: Some(5),
            max_infra_strikes: Some(3),
            ..CompletionGateConfigToml::default()
        }),
        macro_loop: Some(MacroLoopConfigToml {
            enabled: Some(false),
            max_macro_cycles: Some(3),
            max_craft_rounds_per_cycle: Some(2),
            auto_enter_craft: Some("user_confirm".into()),
            craft_on_small_tasks: Some(false),
            min_checklist_items_for_craft: Some(3),
        }),
    }
}

#[must_use]
pub fn resolve_lht(cfg: &Option<LongHorizonConfigToml>) -> LongHorizonConfigToml {
    cfg.clone().unwrap_or_else(product_defaults)
}

/// Keep `config.toml` `[long_horizon]` aligned with the composer tri-state in `settings.toml`.
///
/// - **off** → `enabled=false`, `macro_loop.enabled=false` (baseline for auto after off).
/// - **strict** → `enabled=true`, `mode=strict`, and product gates → `enforce`
///   (matches Desktop copy: no hand-edited verify TOML required).
/// - **auto** → no change (inherits panel baseline).
pub fn sync_long_horizon_with_composer_mode(mode: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    match mode.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "false" => {}
        "strict" => {}
        _ => return Ok(()),
    }

    let mut store = crate::ConfigStore::load(None).context("load config for LHT composer sync")?;
    let mut lh = resolve_lht(&store.config.long_horizon);

    match mode.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "false" => {
            lh.enabled = Some(false);
            let macro_loop = lh.macro_loop.clone().unwrap_or_default();
            lh.macro_loop = Some(MacroLoopConfigToml {
                enabled: Some(false),
                ..macro_loop
            });
        }
        "strict" => {
            lh.enabled = Some(true);
            lh.mode = Some("strict".into());
            let mut gate = lh.completion_gate.take().unwrap_or_default();
            gate.auto_verify_replay = Some("enforce".into());
            gate.toolchain_gate = Some("enforce".into());
            gate.stub_gate = Some("enforce".into());
            lh.completion_gate = Some(gate);
        }
        _ => unreachable!(),
    }

    store.config.long_horizon = Some(lh);
    store
        .save()
        .context("save config after LHT composer sync")?;
    Ok(())
}

#[must_use]
pub fn normalize_gate_mode(raw: &str) -> String {
    match raw.trim().to_ascii_lowercase().as_str() {
        "enforce" => "enforce".into(),
        "observe" => "observe".into(),
        _ => "off".into(),
    }
}

#[must_use]
pub fn normalize_lht_mode(raw: &str) -> String {
    if raw.trim().eq_ignore_ascii_case("strict") {
        "strict".into()
    } else {
        "auto".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn sync_off_writes_disabled_long_horizon_to_config() {
        let _guard = env_lock();
        let dir = std::env::temp_dir().join(format!("zagens-lht-sync-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            r#"
[long_horizon]
enabled = true
mode = "strict"

[long_horizon.macro_loop]
enabled = true
"#,
        )
        .expect("write config");

        let prev_config = std::env::var("ZAGENS_CONFIG_PATH").ok();
        unsafe {
            std::env::set_var(
                "ZAGENS_CONFIG_PATH",
                config_path.to_string_lossy().to_string(),
            );
        }

        sync_long_horizon_with_composer_mode("off").expect("sync off");

        let store = crate::ConfigStore::load(None).expect("reload");
        let lh = resolve_lht(&store.config.long_horizon);
        assert_eq!(lh.enabled, Some(false));
        assert_eq!(lh.macro_loop.as_ref().and_then(|m| m.enabled), Some(false));

        unsafe {
            match prev_config {
                Some(v) => std::env::set_var("ZAGENS_CONFIG_PATH", v),
                None => std::env::remove_var("ZAGENS_CONFIG_PATH"),
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sync_strict_writes_enforce_product_gates() {
        let _guard = env_lock();
        let dir =
            std::env::temp_dir().join(format!("zagens-lht-sync-strict-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        let config_path = dir.join("config.toml");
        fs::write(
            &config_path,
            r#"
[long_horizon]
enabled = true
mode = "auto"

[long_horizon.completion_gate]
auto_verify_replay = "observe"
toolchain_gate = "observe"
stub_gate = "observe"
"#,
        )
        .expect("write config");

        let prev_config = std::env::var("ZAGENS_CONFIG_PATH").ok();
        unsafe {
            std::env::set_var(
                "ZAGENS_CONFIG_PATH",
                config_path.to_string_lossy().to_string(),
            );
        }

        sync_long_horizon_with_composer_mode("strict").expect("sync strict");

        let store = crate::ConfigStore::load(None).expect("reload");
        let lh = resolve_lht(&store.config.long_horizon);
        assert_eq!(lh.enabled, Some(true));
        assert_eq!(lh.mode.as_deref(), Some("strict"));
        let gate = lh.completion_gate.as_ref().expect("gate");
        assert_eq!(gate.auto_verify_replay.as_deref(), Some("enforce"));
        assert_eq!(gate.toolchain_gate.as_deref(), Some("enforce"));
        assert_eq!(gate.stub_gate.as_deref(), Some("enforce"));

        unsafe {
            match prev_config {
                Some(v) => std::env::set_var("ZAGENS_CONFIG_PATH", v),
                None => std::env::remove_var("ZAGENS_CONFIG_PATH"),
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
