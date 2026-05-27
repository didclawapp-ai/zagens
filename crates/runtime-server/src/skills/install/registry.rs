use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use reqwest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::network_policy::NetworkPolicy;
use deepseek_runtime_adapters::network_policy::host_from_url;
use deepseek_runtime_adapters::tools::{check_host_with_policy, host_policy_decision, NetworkGateError};

use super::download::stage_tarball;
use super::types::{
    InstallSource, RegistryDocument, RegistryEntry, RegistryFetchResult, SkillSyncOutcome,
    SyncResult,
};

pub(super) const SKILLS_NETWORK_TOOL: &str = "skills_install";

fn map_registry_gate_error(err: NetworkGateError) -> RegistryFetchResult {
    match err {
        NetworkGateError::Denied { host, .. } => RegistryFetchResult::Denied(host),
        NetworkGateError::PromptRequired { host, .. } => RegistryFetchResult::NeedsApproval(host),
    }
}
pub async fn fetch_registry(
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<RegistryFetchResult> {
    let host = match host_from_url(registry_url) {
        Some(host) => host,
        None => bail!("invalid registry url: {registry_url}"),
    };
    if let Err(err) = check_host_with_policy(network, SKILLS_NETWORK_TOOL, &host) {
        return Ok(map_registry_gate_error(err));
    }
    let body = reqwest::get(registry_url)
        .await
        .with_context(|| format!("failed to fetch registry {registry_url}"))?
        .error_for_status()
        .with_context(|| format!("registry {registry_url} returned an error status"))?
        .text()
        .await
        .with_context(|| format!("failed to read registry body from {registry_url}"))?;
    let parsed: RegistryDocument = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse registry json from {registry_url}"))?;
    Ok(RegistryFetchResult::Loaded(parsed))
}

/// Freshness metadata written alongside each cached skill so subsequent syncs
/// can skip unchanged content.
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct CacheMeta {
    /// ETag returned by the server for the primary asset, if any.
    #[serde(default)]
    etag: Option<String>,
    /// SHA-256 hex digest of the downloaded bytes.
    sha256: String,
    /// Source URL the asset was fetched from.
    url: String,
}
pub async fn sync_registry(
    network: &NetworkPolicy,
    registry_url: &str,
    cache_dir: &Path,
    max_size: u64,
) -> Result<SyncResult> {
    let doc = match fetch_registry(network, registry_url).await? {
        RegistryFetchResult::Loaded(doc) => doc,
        RegistryFetchResult::Denied(host) => return Ok(SyncResult::RegistryDenied(host)),
        RegistryFetchResult::NeedsApproval(host) => {
            return Ok(SyncResult::RegistryNeedsApproval(host));
        }
    };

    let mut outcomes = Vec::new();

    for (name, entry) in &doc.skills {
        let outcome = sync_one_skill(name, entry, network, cache_dir, max_size).await;
        outcomes.push(outcome);
    }

    Ok(SyncResult::Done { outcomes })
}
pub(super) async fn sync_one_skill(
    name: &str,
    entry: &RegistryEntry,
    network: &NetworkPolicy,
    cache_dir: &Path,
    max_size: u64,
) -> SkillSyncOutcome {
    // Resolve the source to a concrete URL list.
    let source = match InstallSource::parse(&entry.source) {
        Ok(s) => s,
        Err(err) => {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!("invalid source spec '{}': {err:#}", entry.source),
            };
        }
    };

    // Registry sources in index.json must not point back at another registry.
    if matches!(source, InstallSource::Registry(_)) {
        return SkillSyncOutcome::Failed {
            name: name.to_string(),
            reason: format!("registry entry for '{name}' must not point to another registry entry"),
        };
    }

    let urls = match &source {
        InstallSource::GitHubRepo(repo) => vec![
            format!("https://github.com/{repo}/archive/refs/heads/main.tar.gz"),
            format!("https://github.com/{repo}/archive/refs/heads/master.tar.gz"),
        ],
        InstallSource::DirectUrl(url) => vec![url.clone()],
        InstallSource::Registry(_) => unreachable!("guarded above"),
    };

    // Check the first downloadable URL against any cached meta.
    let skill_cache_dir = cache_dir.join(name);
    let meta_path = skill_cache_dir.join(".cache-meta.json");

    // Try each candidate URL in order.
    for url in &urls {
        let host = match host_from_url(url) {
            Some(h) => h,
            None => continue,
        };
        if let Err(err) = check_host_with_policy(network, SKILLS_NETWORK_TOOL, &host) {
            return match err {
                NetworkGateError::Denied { host, .. } => SkillSyncOutcome::Denied {
                    name: name.to_string(),
                    host,
                },
                NetworkGateError::PromptRequired { host, .. } => SkillSyncOutcome::NeedsApproval {
                    name: name.to_string(),
                    host,
                },
            };
        }

        // Perform a HEAD request (or conditional GET) for freshness. We use a
        // simple GET with If-None-Match when we have an ETag, falling back to
        // an unconditional GET for servers that don't support ETags.
        let existing_meta: Option<CacheMeta> = meta_path
            .exists()
            .then(|| {
                fs::read_to_string(&meta_path)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
            })
            .flatten();

        // Build the request — add If-None-Match if we have a cached ETag.
        let client = reqwest::Client::new();
        let mut req = client.get(url);
        if let Some(ref meta) = existing_meta
            && let Some(ref etag) = meta.etag
        {
            req = req.header("If-None-Match", etag);
        }

        let resp = match req.send().await {
            Ok(r) => r,
            Err(err) => {
                // Network error — try the next candidate URL.
                let _ = err;
                continue;
            }
        };

        let status = resp.status();

        // 304 Not Modified: cached copy is still fresh.
        if status == reqwest::StatusCode::NOT_MODIFIED {
            return SkillSyncOutcome::Fresh {
                name: name.to_string(),
            };
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            // Try next URL (main → master fallback).
            continue;
        }

        if !status.is_success() {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!("GET {url} returned HTTP {status}"),
            };
        }

        // Capture ETag before consuming the response body.
        let etag = resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        let compressed_cap = max_size.saturating_mul(4);
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(err) => {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to read body from {url}: {err:#}"),
                };
            }
        };
        if bytes.len() as u64 > compressed_cap {
            return SkillSyncOutcome::Failed {
                name: name.to_string(),
                reason: format!(
                    "download from {url} exceeds compressed size cap ({} bytes)",
                    compressed_cap
                ),
            };
        }

        // Compute SHA-256 of the downloaded bytes.
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());

        // Short-circuit: if the hash matches the cached one, we're fresh even
        // without a 304 (some CDNs strip ETags on redirects).
        if let Some(ref meta) = existing_meta
            && meta.sha256 == sha256
            && meta.url == *url
        {
            return SkillSyncOutcome::Fresh {
                name: name.to_string(),
            };
        }

        // Determine whether this is a tarball or a plain SKILL.md.
        // Heuristic: the URL ends with `.tar.gz` or `.tgz`, or the content
        // starts with the gzip magic bytes (0x1f 0x8b).
        let is_tarball =
            url.ends_with(".tar.gz") || url.ends_with(".tgz") || bytes.starts_with(&[0x1f, 0x8b]);

        let final_path: PathBuf = if is_tarball {
            // Extract into a temp staging dir, then rename atomically.
            let staged = match stage_tarball(&bytes, cache_dir, max_size) {
                Ok(s) => s,
                Err(err) => {
                    return SkillSyncOutcome::Failed {
                        name: name.to_string(),
                        reason: format!("tarball extraction failed: {err:#}"),
                    };
                }
            };
            // Move staged dir into its final location, replacing any prior cache.
            let dest = cache_dir.join(name);
            if dest.exists() {
                let _ = fs::remove_dir_all(&dest);
            }
            if let Err(err) = fs::rename(&staged.staged_path, &dest) {
                let _ = fs::remove_dir_all(&staged.staged_path);
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to move staged skill into cache: {err:#}"),
                };
            }
            dest
        } else {
            // Plain SKILL.md (or other companion text file). Write directly.
            if let Err(err) = fs::create_dir_all(&skill_cache_dir) {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to create cache dir: {err:#}"),
                };
            }
            let skill_md_path = skill_cache_dir.join("SKILL.md");
            if let Err(err) = fs::write(&skill_md_path, &bytes) {
                return SkillSyncOutcome::Failed {
                    name: name.to_string(),
                    reason: format!("failed to write SKILL.md to cache: {err:#}"),
                };
            }
            skill_cache_dir.clone()
        };

        // Write the updated freshness metadata.
        let meta = CacheMeta {
            etag,
            sha256,
            url: url.clone(),
        };
        let meta_json = serde_json::to_string(&meta).unwrap_or_default();
        let _ = fs::write(final_path.join(".cache-meta.json"), meta_json);

        return SkillSyncOutcome::Downloaded {
            name: name.to_string(),
            path: final_path,
        };
    }

    // All candidate URLs exhausted without a successful response.
    SkillSyncOutcome::Failed {
        name: name.to_string(),
        reason: format!(
            "all candidate URLs for '{}' failed or were not found",
            entry.source
        ),
    }
}
#[derive(Debug, Deserialize)]
pub(super) struct InstalledFromMarker {
    pub(super) spec: String,
    #[serde(default)]
    pub(super) checksum: String,
}
