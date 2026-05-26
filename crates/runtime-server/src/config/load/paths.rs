use std::fs;
#[cfg(unix)]
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use super::super::DEFAULT_TEXT_MODEL;

// === Defaults ===

pub(crate) fn default_config_path() -> Option<PathBuf> {
    env_config_path().or_else(home_config_path)
}

pub(crate) fn effective_home_dir() -> Option<PathBuf> {
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

pub(crate) fn home_config_path() -> Option<PathBuf> {
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

pub(crate) fn workspace_trust_level_from_doc<'a>(doc: &'a toml::Value, workspace: &Path) -> Option<&'a str> {
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

pub(crate) fn is_trusted_level(level: &str) -> bool {
    level.trim().eq_ignore_ascii_case("trusted")
}

pub(crate) fn workspace_config_key(workspace: &Path) -> String {
    canonicalize_or_keep(workspace)
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn canonicalize_or_keep(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn env_config_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("DEEPSEEK_CONFIG_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return Some(expand_path(trimmed));
        }
    }
    None
}

pub(crate) fn expand_pathbuf(path: PathBuf) -> PathBuf {
    if let Some(raw) = path.to_str() {
        return expand_path(raw);
    }
    path
}

pub(crate) fn resolve_load_config_path(path: Option<PathBuf>) -> Option<PathBuf> {
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

pub(crate) fn default_managed_config_path() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        Some(PathBuf::from("/etc/deepseek/managed_config.toml"))
    }
    #[cfg(not(unix))]
    {
        effective_home_dir().map(|home| home.join(".deepseek").join("managed_config.toml"))
    }
}

pub(crate) fn default_requirements_path() -> Option<PathBuf> {
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

pub(crate) fn default_skills_dir() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("skills"))
}

pub(crate) fn default_mcp_config_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("mcp.json"))
}

pub(crate) fn default_notes_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("notes.txt"))
}

pub(crate) fn default_memory_path() -> Option<PathBuf> {
    effective_home_dir().map(|home| home.join(".deepseek").join("memory.md"))
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
pub(crate) fn write_config_file_secure(path: &Path, content: &str) -> Result<()> {
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
