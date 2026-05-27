use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::network_policy::NetworkPolicy;

use super::download::{candidate_urls, download_first_success, stage_tarball};
use super::local::source_spec_string;
use super::registry::InstalledFromMarker;
use super::types::{
    DownloadOutcome, InstallError, InstallOutcome, InstallSource, InstalledSkill, UpdateResult,
    UrlResolution, DEFAULT_REGISTRY_URL, INSTALLED_FROM_MARKER,
};

pub async fn install(
    source: InstallSource,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    update: bool,
) -> Result<InstallOutcome> {
    install_with_registry(
        source,
        skills_dir,
        max_size,
        network,
        update,
        DEFAULT_REGISTRY_URL,
    )
    .await
}

/// Same as [`install`] but lets the caller override the registry URL. Useful
/// for tests; the slash-command path always uses the configured registry.
pub async fn install_with_registry(
    source: InstallSource,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    update: bool,
    registry_url: &str,
) -> Result<InstallOutcome> {
    let urls = candidate_urls(&source, network, registry_url).await?;
    let urls = match urls {
        UrlResolution::Resolved(urls) => urls,
        UrlResolution::NeedsApproval(host) => return Ok(InstallOutcome::NeedsApproval(host)),
        UrlResolution::Denied(host) => return Ok(InstallOutcome::NetworkDenied(host)),
    };

    // Try each URL in order — GitHub returns 404 for `main` on master-only
    // repos, and we don't want to fail the install on that.
    let (bytes, source_url) = match download_first_success(&urls, network, max_size).await? {
        DownloadOutcome::Bytes { bytes, url } => (bytes, url),
        DownloadOutcome::NeedsApproval(host) => return Ok(InstallOutcome::NeedsApproval(host)),
        DownloadOutcome::Denied(host) => return Ok(InstallOutcome::NetworkDenied(host)),
    };

    // Compute a checksum before unpacking so [`update`] can detect upstream
    // no-op changes without redoing the extract.
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let checksum = format!("{:x}", hasher.finalize());

    let staged = stage_tarball(&bytes, skills_dir, max_size)?;

    // Move the staged dir into its final location. If `update` is set and the
    // destination exists, replace it; otherwise reject.
    let final_path = skills_dir.join(&staged.skill_name);
    if final_path.exists() {
        if !update {
            // Clean up the staging dir before returning the error.
            let _ = fs::remove_dir_all(&staged.staged_path);
            return Err(InstallError::AlreadyInstalled(staged.skill_name).into());
        }
        // Best-effort backup-then-replace; on failure we restore the original.
        let backup = skills_dir.join(format!("{}.bak", staged.skill_name));
        // If a previous failed update left a stale `.bak/`, drop it.
        if backup.exists() {
            fs::remove_dir_all(&backup).ok();
        }
        fs::rename(&final_path, &backup).with_context(|| {
            format!(
                "failed to backup existing skill at {}",
                final_path.display()
            )
        })?;
        if let Err(err) = fs::rename(&staged.staged_path, &final_path) {
            // Roll back: restore the backup so the user isn't left with an
            // empty skill directory.
            fs::rename(&backup, &final_path).ok();
            return Err(err).context("failed to install staged skill");
        }
        fs::remove_dir_all(&backup).ok();
    } else {
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create skills directory {}", parent.display())
            })?;
        }
        fs::rename(&staged.staged_path, &final_path).context("failed to install staged skill")?;
    }

    // Write the marker last so a partial install never leaves a stale
    // .installed-from on disk.
    let marker_body = serde_json::json!({
        "spec": source_spec_string(&source),
        "url": source_url,
        "checksum": checksum,
    })
    .to_string();
    fs::write(final_path.join(INSTALLED_FROM_MARKER), marker_body).with_context(|| {
        format!(
            "failed to write {} marker for skill {}",
            INSTALLED_FROM_MARKER, staged.skill_name
        )
    })?;

    Ok(InstallOutcome::Installed(InstalledSkill {
        name: staged.skill_name,
        path: final_path,
        source_checksum: checksum,
    }))
}
pub async fn update(
    name: &str,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
) -> Result<UpdateResult> {
    update_with_registry(name, skills_dir, max_size, network, DEFAULT_REGISTRY_URL).await
}

/// Same as [`update`] but lets the caller override the registry URL.
pub async fn update_with_registry(
    name: &str,
    skills_dir: &Path,
    max_size: u64,
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<UpdateResult> {
    let target = skills_dir.join(name);
    let marker_path = target.join(INSTALLED_FROM_MARKER);
    if !marker_path.exists() {
        return Err(InstallError::NotInstalledHere(name.to_string()).into());
    }
    let marker_body = fs::read_to_string(&marker_path)
        .with_context(|| format!("failed to read {}", marker_path.display()))?;
    let marker: InstalledFromMarker = serde_json::from_str(&marker_body)
        .with_context(|| format!("malformed {} for {name}", INSTALLED_FROM_MARKER))?;

    // Re-resolve the URL, taking the existing checksum as a short-circuit hint:
    // we still hit the network so the user gets a useful "no upstream change"
    // signal, but we skip the unpack step if the bytes match.
    let source = InstallSource::parse(&marker.spec)?;
    let urls = match candidate_urls(&source, network, registry_url).await? {
        UrlResolution::Resolved(urls) => urls,
        UrlResolution::NeedsApproval(host) => return Ok(UpdateResult::NeedsApproval(host)),
        UrlResolution::Denied(host) => return Ok(UpdateResult::NetworkDenied(host)),
    };
    let (bytes, _url) = match download_first_success(&urls, network, max_size).await? {
        DownloadOutcome::Bytes { bytes, url } => (bytes, url),
        DownloadOutcome::NeedsApproval(host) => return Ok(UpdateResult::NeedsApproval(host)),
        DownloadOutcome::Denied(host) => return Ok(UpdateResult::NetworkDenied(host)),
    };

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let checksum = format!("{:x}", hasher.finalize());
    if checksum == marker.checksum {
        return Ok(UpdateResult::NoChange);
    }

    // Bytes changed — fall back to the regular install path with `update = true`
    // so we get the same atomic-replace semantics.
    let outcome =
        install_with_registry(source, skills_dir, max_size, network, true, registry_url).await?;
    match outcome {
        InstallOutcome::Installed(installed) => Ok(UpdateResult::Updated(installed)),
        InstallOutcome::NeedsApproval(host) => Ok(UpdateResult::NeedsApproval(host)),
        InstallOutcome::NetworkDenied(host) => Ok(UpdateResult::NetworkDenied(host)),
    }
}
