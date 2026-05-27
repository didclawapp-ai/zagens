use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;

use crate::network_policy::NetworkPolicy;
use deepseek_runtime_adapters::network_policy::{Decision, host_from_url};
use deepseek_runtime_adapters::tools::{check_host_with_policy, host_policy_decision, NetworkGateError};

use super::registry::fetch_registry;
use super::registry::SKILLS_NETWORK_TOOL;
use super::types::{
    DownloadAttempt, DownloadOutcome, InstallError, InstallSource, RegistryDocument,
    RegistryFetchResult, UrlResolution,
};

/// Resolve the source spec into one or more candidate URLs to try in order.
pub(super) async fn candidate_urls(
    source: &InstallSource,
    network: &NetworkPolicy,
    registry_url: &str,
) -> Result<UrlResolution> {
    match source {
        InstallSource::GitHubRepo(repo) => {
            // GitHub's archive endpoint lives on `codeload.github.com` after
            // the redirect, but the public URL we hit is `github.com`. Both
            // typically appear in user allow lists; we check the canonical
            // host.
            Ok(UrlResolution::Resolved(vec![
                format!("https://github.com/{repo}/archive/refs/heads/main.tar.gz"),
                format!("https://github.com/{repo}/archive/refs/heads/master.tar.gz"),
            ]))
        }
        InstallSource::DirectUrl(url) => Ok(UrlResolution::Resolved(vec![url.clone()])),
        InstallSource::Registry(name) => {
            match fetch_registry(network, registry_url).await? {
                RegistryFetchResult::Loaded(doc) => {
                    let entry = doc
                        .skills
                        .get(name)
                        .with_context(|| format!("skill '{name}' not found in registry"))?
                        .clone();
                    let inner = InstallSource::parse(&entry.source).with_context(|| {
                        format!(
                            "registry entry for '{name}' has invalid source: {}",
                            entry.source
                        )
                    })?;
                    // Recurse only one level — registry pointing at registry is
                    // disallowed to avoid cycles.
                    if matches!(inner, InstallSource::Registry(_)) {
                        bail!("registry entry for '{name}' must not point to another registry");
                    }
                    // Reuse this function for the inner source so GitHub fallback
                    // still applies.
                    Box::pin(candidate_urls(&inner, network, registry_url)).await
                }
                RegistryFetchResult::NeedsApproval(host) => Ok(UrlResolution::NeedsApproval(host)),
                RegistryFetchResult::Denied(host) => Ok(UrlResolution::Denied(host)),
            }
        }
    }
}

/// Download the first URL whose host the policy allows and which returns 2xx.
/// Returns `NeedsApproval` if every candidate hit `Prompt`, or `Denied` if every
/// candidate was denied.
pub(super) async fn download_first_success(
    urls: &[String],
    network: &NetworkPolicy,
    max_size: u64,
) -> Result<DownloadOutcome> {
    let mut last_status: Option<reqwest::StatusCode> = None;
    let mut prompt_host: Option<String> = None;
    let mut denied_host: Option<String> = None;
    for url in urls {
        let host = match host_from_url(url) {
            Some(h) => h,
            None => bail!("invalid download url: {url}"),
        };
        match host_policy_decision(network, &host) {
            Decision::Allow => {}
            Decision::Deny => {
                denied_host.get_or_insert(host);
                continue;
            }
            Decision::Prompt => {
                prompt_host.get_or_insert(host);
                continue;
            }
        }
        match download_with_cap(url, max_size).await? {
            DownloadAttempt::Bytes(bytes) => {
                return Ok(DownloadOutcome::Bytes {
                    bytes,
                    url: url.clone(),
                });
            }
            DownloadAttempt::NotFound(status) => {
                last_status = Some(status);
                continue;
            }
        }
    }
    if let Some(host) = denied_host {
        return Ok(DownloadOutcome::Denied(host));
    }
    if let Some(host) = prompt_host {
        return Ok(DownloadOutcome::NeedsApproval(host));
    }
    bail!(
        "failed to download skill (last status: {})",
        last_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    );
}

/// Stream a URL into memory with a size cap. Aborts on the first read that
/// would push the buffer over `max_size * 4` (the *4 accounts for compression;
/// the unpack step still enforces `max_size` on the *uncompressed* bytes).
pub(super) async fn download_with_cap(url: &str, max_size: u64) -> Result<DownloadAttempt> {
    let resp = reqwest::get(url)
        .await
        .with_context(|| format!("failed to GET {url}"))?;
    let status = resp.status();
    if !status.is_success() {
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(DownloadAttempt::NotFound(status));
        }
        bail!("download {url} returned {status}");
    }
    // Soft cap on the *compressed* download — well above max_size to allow
    // for highly compressible payloads but still bounded.
    let compressed_cap = max_size.saturating_mul(4);
    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("failed to read body of {url}"))?;
    if (bytes.len() as u64) > compressed_cap {
        bail!("download {url} exceeds compressed size cap of {compressed_cap} bytes");
    }
    Ok(DownloadAttempt::Bytes(bytes.to_vec()))
}

pub(super) struct StagedSkill {
    pub(super) skill_name: String,
    pub(super) staged_path: PathBuf,
}

/// Validate a tarball and extract it into `<skills_dir>/<name>.tmp/`.
pub(super) fn stage_tarball(bytes: &[u8], skills_dir: &Path, max_size: u64) -> Result<StagedSkill> {
    fs::create_dir_all(skills_dir)
        .with_context(|| format!("failed to create skills directory {}", skills_dir.display()))?;

    // Two passes: first determine the skill name (and therefore the staged
    // dir) by finding the SKILL.md, then extract under that staged dir.
    // Both passes share the same archive bytes; we reset by wrapping fresh
    // decoders.

    let scan = scan_tarball(bytes, max_size)?;

    // Prepare staged directory. Use a `.tmp` suffix so a crashed install
    // never collides with a real name; remove any leftover from a prior
    // failed attempt.
    let staged_path = skills_dir.join(format!("{}.tmp", scan.skill_name));
    if staged_path.exists() {
        fs::remove_dir_all(&staged_path).with_context(|| {
            format!(
                "failed to clean stale staging dir {}",
                staged_path.display()
            )
        })?;
    }
    fs::create_dir_all(&staged_path)
        .with_context(|| format!("failed to create staging dir {}", staged_path.display()))?;

    // Second pass — extract.
    let result = extract_into(&scan, bytes, &staged_path, max_size);
    if let Err(err) = result {
        // Cleanup on failure so a half-staged directory doesn't survive.
        let _ = fs::remove_dir_all(&staged_path);
        return Err(err);
    }

    Ok(StagedSkill {
        skill_name: scan.skill_name,
        staged_path,
    })
}

struct TarballScan {
    /// Skill name from SKILL.md frontmatter.
    skill_name: String,
    /// Archive prefix to strip from each entry (e.g. `repo-main/`). May be empty.
    prefix: String,
    /// Sub-directory inside `prefix` that the SKILL.md lives in (`""` if root,
    /// or `skills/<name>` for repos that bundle multiple skills).
    skill_root: String,
}

/// First pass: locate SKILL.md, validate frontmatter, compute total size,
/// reject path-traversal entries and symlinks inside the selected install
/// subtree. We do not write anything in this pass; that's the second pass's job.
pub(super) fn scan_tarball(bytes: &[u8], max_size: u64) -> Result<TarballScan> {
    let cursor = std::io::Cursor::new(bytes);
    let gz = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    let mut total_size: u64 = 0;
    let mut prefix: Option<String> = None;
    let mut skill_md_relative: Option<(String, Vec<u8>)> = None;
    let mut link_paths: Vec<String> = Vec::new();

    for entry in archive
        .entries()
        .context("failed to read tar entries (corrupt archive?)")?
    {
        let mut entry = entry.context("failed to read tar entry")?;
        let header = entry.header().clone();
        let entry_type = header.entry_type();
        let path = entry
            .path()
            .context("tar entry has invalid path")?
            .to_path_buf();
        let path_str = path.to_string_lossy().into_owned();
        if !is_safe_path(&path) {
            return Err(InstallError::PathTraversal(path_str).into());
        }

        // Track total size against `max_size` (uncompressed). We honor `header
        // .size` rather than streaming-read every file; tar archives are
        // self-describing so this is reliable for non-malicious inputs and
        // catches the gzip-bomb case.
        if let Ok(size) = header.size() {
            total_size = total_size.saturating_add(size);
            if total_size > max_size {
                return Err(InstallError::OversizedTarball { limit: max_size }.into());
            }
        }

        // Detect prefix from the first entry. GitHub archives wrap everything
        // in `<repo>-<branch>/`; direct tarballs may have no prefix. We treat
        // the first path component as the prefix iff the archive has more than
        // one entry under it, but for SKILL.md detection we just strip the
        // first component if every entry shares it.
        if prefix.is_none() {
            if let Some(Component::Normal(first)) = path.components().next() {
                let candidate = first.to_string_lossy().into_owned();
                // Only treat the first component as a prefix if it's a
                // directory-like (no extension and the path has more
                // components). Otherwise leave prefix empty.
                if path.components().count() > 1 {
                    prefix = Some(candidate);
                } else {
                    prefix = Some(String::new());
                }
            } else {
                prefix = Some(String::new());
            }
        }

        if entry_type.is_symlink() || entry_type.is_hard_link() {
            link_paths.push(path_str);
            continue;
        }

        // SKILL.md detection. Match either:
        //   * `<prefix>/SKILL.md`
        //   * `<prefix>/skills/<name>/SKILL.md`
        if entry_type.is_file() {
            let stripped = strip_prefix(&path_str, prefix.as_deref().unwrap_or(""));
            if stripped.eq_ignore_ascii_case("SKILL.md")
                || stripped.starts_with("skills/")
                    && stripped.ends_with("/SKILL.md")
                    && stripped.matches('/').count() == 2
            {
                let mut buf = Vec::new();
                entry
                    .read_to_end(&mut buf)
                    .context("failed to read SKILL.md from archive")?;
                // Prefer the first match — we don't support multi-skill
                // archives where a tarball ships several SKILL.mds at once.
                if skill_md_relative.is_none() {
                    skill_md_relative = Some((stripped.to_string(), buf));
                }
            }
        }
    }

    let prefix = prefix.unwrap_or_default();
    let (skill_md_path, skill_md_bytes) = skill_md_relative
        .ok_or(InstallError::MissingSkillMd)
        .map_err(anyhow::Error::from)?;

    let skill_root = if skill_md_path == "SKILL.md" {
        String::new()
    } else {
        // strip trailing /SKILL.md
        skill_md_path
            .strip_suffix("/SKILL.md")
            .unwrap_or("")
            .to_string()
    };

    for link_path in link_paths {
        if is_within_selected_root(&link_path, &prefix, &skill_root) {
            return Err(InstallError::SymlinkRejected.into());
        }
    }

    // Parse frontmatter to extract the skill name. We reuse the same parser
    // shape as `SkillRegistry::parse_skill` but inline it here so we don't
    // depend on the discovery module's private function.
    let name = parse_frontmatter_name(&skill_md_bytes)?;

    Ok(TarballScan {
        skill_name: name,
        prefix,
        skill_root,
    })
}

pub(super) fn extract_into(scan: &TarballScan, bytes: &[u8], dest: &Path, max_size: u64) -> Result<()> {
    let cursor = std::io::Cursor::new(bytes);
    let gz = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(gz);

    let mut total_size: u64 = 0;
    let prefix_with_root = if scan.skill_root.is_empty() {
        scan.prefix.clone()
    } else if scan.prefix.is_empty() {
        scan.skill_root.clone()
    } else {
        format!("{}/{}", scan.prefix, scan.skill_root)
    };

    for entry in archive
        .entries()
        .context("failed to read tar entries (corrupt archive?)")?
    {
        let mut entry = entry.context("failed to read tar entry")?;
        let header = entry.header().clone();
        let entry_type = header.entry_type();
        let path = entry
            .path()
            .context("tar entry has invalid path")?
            .to_path_buf();
        let path_str = path.to_string_lossy().into_owned();
        if !is_safe_path(&path) {
            return Err(InstallError::PathTraversal(path_str).into());
        }

        // Only extract entries that live under our skill root. For simple
        // tarballs (`SKILL.md` at root) that's everything; for multi-skill
        // repos it's the `skills/<name>/` slice.
        let stripped = strip_prefix(&path_str, &prefix_with_root).into_owned();
        if stripped.is_empty() && entry_type.is_dir() {
            // The root directory itself — already created.
            continue;
        }
        if stripped == path_str && !prefix_with_root.is_empty() {
            // Nothing to strip => entry is outside our subtree, skip.
            continue;
        }
        // Defense-in-depth: re-validate the stripped path.
        let stripped_path = Path::new(&stripped);
        if !is_safe_path(stripped_path) {
            return Err(InstallError::PathTraversal(stripped).into());
        }
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(InstallError::SymlinkRejected.into());
        }

        let target = dest.join(stripped_path);
        // Final paranoia check: ensure the resolved target stays under dest.
        // We can't canonicalize (target doesn't exist yet), so we walk
        // components one more time after composing.
        let target_components: Vec<_> = target.components().collect();
        let dest_components: Vec<_> = dest.components().collect();
        if !target_components.starts_with(dest_components.as_slice()) {
            return Err(InstallError::PathTraversal(stripped).into());
        }

        if entry_type.is_dir() {
            fs::create_dir_all(&target)
                .with_context(|| format!("failed to create dir {}", target.display()))?;
            continue;
        }
        if entry_type.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create dir {}", parent.display()))?;
            }
            // Read into a buffer so we can enforce `max_size`. Files inside
            // a SKILL bundle are small; copying through a buffer is fine.
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .with_context(|| format!("failed to read {}", path.display()))?;
            total_size = total_size.saturating_add(buf.len() as u64);
            if total_size > max_size {
                return Err(InstallError::OversizedTarball { limit: max_size }.into());
            }
            let mut out = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&target)
                .with_context(|| format!("failed to create {}", target.display()))?;
            out.write_all(&buf)
                .with_context(|| format!("failed to write {}", target.display()))?;
        }
    }
    Ok(())
}

pub(super) fn selected_root(prefix: &str, skill_root: &str) -> String {
    if skill_root.is_empty() {
        prefix.to_string()
    } else if prefix.is_empty() {
        skill_root.to_string()
    } else {
        format!("{prefix}/{skill_root}")
    }
}

pub(super) fn is_within_selected_root(path: &str, prefix: &str, skill_root: &str) -> bool {
    let root = selected_root(prefix, skill_root);
    if root.is_empty() {
        return true;
    }
    path == root || path.starts_with(&format!("{root}/"))
}

/// Ensure a tar path has no `..` segments and is not absolute.
pub(super) fn is_safe_path(path: &Path) -> bool {
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            Component::ParentDir => return false,
            Component::Prefix(_) | Component::RootDir => return false,
            _ => {}
        }
    }
    true
}

/// Strip a leading directory prefix (e.g. `repo-main/`) from a tarball path.
pub(super) fn strip_prefix<'a>(path: &'a str, prefix: &str) -> std::borrow::Cow<'a, str> {
    if prefix.is_empty() {
        return std::borrow::Cow::Borrowed(path);
    }
    let with_slash = format!("{prefix}/");
    if let Some(rest) = path.strip_prefix(&with_slash) {
        std::borrow::Cow::Owned(rest.to_string())
    } else if path == prefix {
        std::borrow::Cow::Borrowed("")
    } else {
        std::borrow::Cow::Borrowed(path)
    }
}

/// Extract `name:` and ensure `description:` exist in the SKILL.md frontmatter.
/// Also verifies the leading `---` fence so we reject malformed files early.
pub(super) fn parse_frontmatter_name(bytes: &[u8]) -> Result<String> {
    let content = std::str::from_utf8(bytes).context("SKILL.md is not valid UTF-8")?;
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        bail!("SKILL.md is missing the leading '---' frontmatter fence");
    }
    let after_open = &trimmed[3..];
    let close = after_open.find("---").ok_or_else(|| {
        anyhow::anyhow!("SKILL.md is missing the closing '---' frontmatter fence")
    })?;
    let frontmatter = &after_open[..close];

    let mut name: Option<String> = None;
    let mut has_description = false;
    for raw in frontmatter.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match key.as_str() {
                "name" if !value.is_empty() => name = Some(value),
                "description" if !value.is_empty() => has_description = true,
                _ => {}
            }
        }
    }

    let name = name.ok_or(InstallError::MissingFrontmatterField("name"))?;
    if !has_description {
        return Err(InstallError::MissingFrontmatterField("description").into());
    }
    // Sanity check: name must be a single path-safe segment.
    if name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.contains(' ')
    {
        bail!("SKILL.md `name` must be a single path-safe segment (got '{name}')");
    }
    Ok(name)
}
