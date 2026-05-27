use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use super::download::{is_safe_path, parse_frontmatter_name};
use super::types::{InstallError, InstalledSkill, InstallSource, INSTALLED_FROM_MARKER, TRUSTED_MARKER};

pub fn import_local_directory(
    source_directory: &Path,
    skills_dir: &Path,
    replace: bool,
    max_size: u64,
) -> Result<InstalledSkill> {
    let src = source_directory
        .canonicalize()
        .with_context(|| format!("source directory {}", source_directory.display()))?;
    if !src.is_dir() {
        bail!("source path is not a directory");
    }
    let skill_md = src.join("SKILL.md");
    if !skill_md.is_file() {
        bail!(
            "source directory must contain SKILL.md (got {})",
            skill_md.display()
        );
    }
    let skill_md_bytes = fs::read(&skill_md).with_context(|| skill_md.display().to_string())?;
    let skill_name = parse_frontmatter_name(&skill_md_bytes)?;

    let mut hasher = Sha256::new();
    hasher.update(&skill_md_bytes);
    let checksum = format!("{:x}", hasher.finalize());

    let final_path = skills_dir.join(&skill_name);
    if final_path.exists() {
        if !replace {
            return Err(InstallError::AlreadyInstalled(skill_name.clone()).into());
        }
        fs::remove_dir_all(&final_path).with_context(|| {
            format!(
                "failed to remove existing skill at {}",
                final_path.display()
            )
        })?;
    }
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create skills directory {}", parent.display())
        })?;
    }
    fs::create_dir_all(&final_path)
        .with_context(|| format!("failed to create {}", final_path.display()))?;

    let mut total_size: u64 = 0;
    copy_local_skill_tree(&src, &final_path, &src, &mut total_size, max_size)?;

    let marker_body = serde_json::json!({
        "spec": format!("local:{}", src.display()),
        "url": "",
        "checksum": checksum,
    })
    .to_string();
    fs::write(final_path.join(INSTALLED_FROM_MARKER), marker_body).with_context(|| {
        format!(
            "failed to write {} marker for skill {}",
            INSTALLED_FROM_MARKER, skill_name
        )
    })?;

    Ok(InstalledSkill {
        name: skill_name,
        path: final_path,
        source_checksum: checksum,
    })
}

pub(super) fn copy_local_skill_tree(
    src_root: &Path,
    dest_root: &Path,
    current: &Path,
    total_size: &mut u64,
    max_size: u64,
) -> Result<()> {
    for entry in fs::read_dir(current)
        .with_context(|| format!("failed to read directory {}", current.display()))?
    {
        let entry = entry.with_context(|| current.display().to_string())?;
        let file_type = entry.file_type().with_context(|| entry.path().display().to_string())?;
        if file_type.is_symlink() {
            return Err(InstallError::SymlinkRejected.into());
        }
        let path = entry.path();
        let rel = path
            .strip_prefix(src_root)
            .with_context(|| format!("path {} escapes source root", path.display()))?;
        if !is_safe_path(rel) {
            return Err(InstallError::PathTraversal(rel.display().to_string()).into());
        }
        let dest = dest_root.join(rel);
        let dest_components: Vec<_> = dest.components().collect();
        let dest_root_components: Vec<_> = dest_root.components().collect();
        if !dest_components.starts_with(dest_root_components.as_slice()) {
            return Err(InstallError::PathTraversal(rel.display().to_string()).into());
        }

        if file_type.is_dir() {
            fs::create_dir_all(&dest)
                .with_context(|| format!("failed to create dir {}", dest.display()))?;
            copy_local_skill_tree(src_root, dest_root, &path, total_size, max_size)?;
            continue;
        }
        if file_type.is_file() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create dir {}", parent.display()))?;
            }
            let bytes = fs::read(&path).with_context(|| path.display().to_string())?;
            *total_size = total_size.saturating_add(bytes.len() as u64);
            if *total_size > max_size {
                return Err(InstallError::OversizedTarball { limit: max_size }.into());
            }
            fs::write(&dest, &bytes).with_context(|| dest.display().to_string())?;
        }
    }
    Ok(())
}

/// Re-fetch a previously installed skill and replace it on disk if the
/// upstream tarball changed.
///
/// Reads `.installed-from` to recover the original [`InstallSource`], so
/// a skill installed via `/skill install github:foo/bar` can be updated via
/// `/skill update bar` without the user re-typing the spec.
///
/// Convenience wrapper over [`update_with_registry`].
pub fn uninstall(name: &str, skills_dir: &Path) -> Result<()> {
    let target = skills_dir.join(name);
    if !target.exists() {
        bail!("skill '{name}' is not installed at {}", target.display());
    }
    if !target.join(INSTALLED_FROM_MARKER).exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    fs::remove_dir_all(&target)
        .with_context(|| format!("failed to remove {}", target.display()))?;
    Ok(())
}

/// Mark a community-installed skill as trusted. Currently a marker file only;
/// callers that wire tool execution against `<name>/scripts/` consult the file
/// before invoking anything. No-op if already trusted.
///
/// Refuses to mark system skills (no `.installed-from`) so the bundled
/// `skill-creator` doesn't accidentally inherit elevated tool privileges.
pub fn trust(name: &str, skills_dir: &Path) -> Result<()> {
    let target = skills_dir.join(name);
    if !target.exists() {
        bail!("skill '{name}' is not installed at {}", target.display());
    }
    if !target.join(INSTALLED_FROM_MARKER).exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    let marker = target.join(TRUSTED_MARKER);
    if !marker.exists() {
        fs::write(
            &marker,
            "Skill scripts/ are user-trusted. Delete this file to revoke.\n",
        )
        .with_context(|| format!("failed to write {}", marker.display()))?;
    }
    Ok(())
}
pub(super) fn source_spec_string(source: &InstallSource) -> String {
    match source {
        InstallSource::GitHubRepo(repo) => format!("github:{repo}"),
        InstallSource::DirectUrl(url) => url.clone(),
        InstallSource::Registry(name) => name.clone(),
    }
}
