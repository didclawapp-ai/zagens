//! Zagens user-level data directory (`~/.zagens/`) and legacy `~/.deepseek/` migration.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::CONFIG_FILE_NAME;

/// Global user data directory for Zagens (not workspace-local `.zagens/`).
pub const USER_DATA_DIR_NAME: &str = ".zagens";

/// Legacy directory shared with upstream deepseek-tui / CLI.
pub const LEGACY_USER_DATA_DIR_NAME: &str = ".deepseek";

/// Per-workspace metadata directory (rules, scratchpad, symbol index, blackboards, …).
pub const WORKSPACE_META_DIR_NAME: &str = ".zagens";

/// Legacy per-workspace metadata directory (pre-Zagens desktop branding).
pub const LEGACY_WORKSPACE_META_DIR_NAME: &str = ".deepseek";

/// Resolve `~/.zagens/`.
pub fn user_data_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("failed to resolve home directory")?;
    Ok(home.join(USER_DATA_DIR_NAME))
}

/// Resolve `~/.deepseek/` (legacy).
pub fn legacy_user_data_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("failed to resolve home directory")?;
    Ok(home.join(LEGACY_USER_DATA_DIR_NAME))
}

/// Resolve a path under `~/.zagens/` (e.g. `sessions`, `skills/mcp.json`).
pub fn user_data_path(relative: &str) -> Result<PathBuf> {
    Ok(user_data_root()?.join(relative))
}

/// Same as [`user_data_path`], but falls back to `.{USER_DATA_DIR_NAME}/<relative>` when home is unknown.
#[must_use]
pub fn user_data_path_or_relative(relative: &str) -> PathBuf {
    user_data_path(relative).unwrap_or_else(|_| PathBuf::from(USER_DATA_DIR_NAME).join(relative))
}

/// TOML-friendly path with `~/` prefix.
#[must_use]
pub fn tilde_user_data_path(relative: &str) -> String {
    format!("~/{USER_DATA_DIR_NAME}/{relative}")
}

/// `$WORKSPACE/.zagens/` — target for all new workspace-local metadata writes.
#[must_use]
pub fn workspace_meta_dir(workspace: &Path) -> PathBuf {
    workspace.join(WORKSPACE_META_DIR_NAME)
}

#[must_use]
pub fn legacy_workspace_meta_dir(workspace: &Path) -> PathBuf {
    workspace.join(LEGACY_WORKSPACE_META_DIR_NAME)
}

/// Resolve workspace meta directory for reads: prefer `.zagens/` when present, else legacy `.deepseek/`, else default `.zagens/`.
#[must_use]
pub fn workspace_meta_dir_read(workspace: &Path) -> PathBuf {
    let zagens = workspace_meta_dir(workspace);
    if zagens.is_dir() {
        return zagens;
    }
    let legacy = legacy_workspace_meta_dir(workspace);
    if legacy.is_dir() {
        return legacy;
    }
    zagens
}

/// Resolve a file under workspace meta for reads (`.zagens/` first, then legacy).
#[must_use]
pub fn workspace_meta_file_read(workspace: &Path, relative: &str) -> PathBuf {
    let zagens = workspace_meta_dir(workspace).join(relative);
    if zagens.exists() {
        return zagens;
    }
    let legacy = legacy_workspace_meta_dir(workspace).join(relative);
    if legacy.exists() {
        return legacy;
    }
    zagens
}

/// Path for a new/updated file under `$WORKSPACE/.zagens/`.
#[must_use]
pub fn workspace_meta_file_write(workspace: &Path, relative: &str) -> PathBuf {
    workspace_meta_dir(workspace).join(relative)
}

/// Workspace-relative path string for display (always `.zagens/…` for new artifacts).
#[must_use]
pub fn workspace_meta_rel(relative: &str) -> String {
    format!("{WORKSPACE_META_DIR_NAME}/{relative}")
}

/// Default `~/.zagens/config.toml`.
pub fn default_config_path() -> Result<PathBuf> {
    Ok(user_data_root()?.join(CONFIG_FILE_NAME))
}

/// Legacy `~/.deepseek/config.toml`.
pub fn legacy_config_path() -> Result<PathBuf> {
    Ok(legacy_user_data_root()?.join(CONFIG_FILE_NAME))
}

/// Copy legacy `config.toml` and optional non-session assets when `~/.zagens/` is fresh.
pub fn migrate_legacy_user_data_if_needed(config_dest: &Path) -> Result<bool> {
    if config_dest.exists() {
        return Ok(false);
    }
    let canonical = default_config_path()?;
    if config_dest != canonical {
        return Ok(false);
    }
    let legacy_root = legacy_user_data_root()?;
    let zagens_root = user_data_root()?;
    migrate_legacy_user_data_at(config_dest, &legacy_root, &zagens_root)
}

fn migrate_legacy_user_data_at(
    config_dest: &Path,
    legacy_root: &Path,
    zagens_root: &Path,
) -> Result<bool> {
    let legacy_config = legacy_root.join(CONFIG_FILE_NAME);
    if !legacy_config.is_file() {
        return Ok(false);
    }

    if let Some(parent) = config_dest.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create config directory {}", parent.display())
        })?;
    }
    fs::copy(&legacy_config, config_dest).with_context(|| {
        format!(
            "failed to copy legacy config from {} to {}",
            legacy_config.display(),
            config_dest.display()
        )
    })?;

    copy_legacy_file_if_missing(legacy_root, zagens_root, "mcp.json")?;
    copy_legacy_file_if_missing(legacy_root, zagens_root, "notes.txt")?;
    copy_legacy_file_if_missing(legacy_root, zagens_root, "memory.md")?;
    copy_legacy_file_if_missing(legacy_root, zagens_root, "secrets/secrets.json")?;
    copy_legacy_dir_if_missing(legacy_root, zagens_root, "skills")?;

    Ok(true)
}

fn copy_legacy_file_if_missing(legacy_root: &Path, zagens_root: &Path, rel: &str) -> Result<()> {
    let src = legacy_root.join(rel);
    let dst = zagens_root.join(rel);
    if dst.exists() || !src.is_file() {
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&src, &dst).with_context(|| {
        format!(
            "failed to migrate legacy file {} → {}",
            src.display(),
            dst.display()
        )
    })?;
    Ok(())
}

fn copy_legacy_dir_if_missing(legacy_root: &Path, zagens_root: &Path, rel: &str) -> Result<()> {
    let src = legacy_root.join(rel);
    let dst = zagens_root.join(rel);
    if dst.exists() || !src.is_dir() {
        return Ok(());
    }
    copy_dir_recursive(&src, &dst)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn migrate_legacy_config_skips_sessions() -> Result<()> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "zagens-path-migrate-{}-{}",
            std::process::id(),
            nanos
        ));
        let legacy = home.join(LEGACY_USER_DATA_DIR_NAME);
        let zagens = home.join(USER_DATA_DIR_NAME);
        fs::create_dir_all(&legacy)?;
        fs::create_dir_all(legacy.join("sessions"))?;
        fs::write(legacy.join("sessions").join("sessions.db"), b"legacy")?;
        fs::write(
            legacy.join(CONFIG_FILE_NAME),
            b"default_text_model = \"deepseek-v4-pro\"\n",
        )?;

        let dest = zagens.join(CONFIG_FILE_NAME);
        let migrated = migrate_legacy_user_data_at(&dest, &legacy, &zagens)?;
        assert!(migrated);
        assert!(dest.exists());
        assert!(
            !zagens.join("sessions").exists(),
            "sessions must not be migrated"
        );

        let _ = fs::remove_dir_all(home);
        Ok(())
    }
}
