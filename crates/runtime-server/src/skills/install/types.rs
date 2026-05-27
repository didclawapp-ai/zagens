use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use thiserror::Error;

pub fn default_cache_skills_dir() -> PathBuf {
    dirs::home_dir().map_or_else(
        || PathBuf::from("/tmp/deepseek/cache/skills"),
        |p| p.join(".deepseek").join("cache").join("skills"),
    )
}

/// Default registry. Falls back to a community-curated `index.json` hosted on
/// GitHub raw; users can override via `[skills] registry_url` in config.toml.
pub const DEFAULT_REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/Hmbown/deepseek-skills/main/index.json";

/// Default per-skill size cap (5 MiB). Honored at unpack time so a malicious
/// gzip bomb can't blow up RAM.
pub const DEFAULT_MAX_SIZE_BYTES: u64 = 5 * 1024 * 1024;

/// File written under each installed skill so [`update`] / [`uninstall`] can
/// recover the original [`InstallSource`] without re-parsing user input.
pub const INSTALLED_FROM_MARKER: &str = ".installed-from";

/// File written under each trusted skill. Currently advisory (the install path
/// never auto-runs anything) — the runtime tool-invocation gate consults this
/// marker before executing scripts that ship with the skill.
pub const TRUSTED_MARKER: &str = ".trusted";

// ─────────────────────────────────────────────────────────────────────────────
// Source parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Where a skill is being installed from. See [`InstallSource::parse`] for the
/// accepted spec syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    /// `github:owner/repo`. Resolved to
    /// `https://github.com/<owner>/<repo>/archive/refs/heads/main.tar.gz`
    /// with a `master.tar.gz` fallback on 404.
    GitHubRepo(String),
    /// Raw `http(s)://…` tarball URL. Used as-is.
    DirectUrl(String),
    /// Curated registry lookup key. Looked up via the configured `registry_url`.
    Registry(String),
}

impl InstallSource {
    /// Parse a user-supplied spec. Empty / whitespace-only input is rejected.
    ///
    /// * `github:owner/repo` → [`InstallSource::GitHubRepo`]
    /// * `https://github.com/owner/repo[.git]` (no path past the repo) →
    ///   [`InstallSource::GitHubRepo`]
    /// * any other `http://` or `https://` prefix → [`InstallSource::DirectUrl`]
    /// * anything else → [`InstallSource::Registry`]
    pub fn parse(spec: &str) -> Result<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() {
            bail!("install source must not be empty");
        }
        if let Some(rest) = trimmed.strip_prefix("github:") {
            let rest = rest.trim();
            // Reject obviously bogus values up front. We intentionally accept
            // case-insensitive owner/repo so `github:Hmbown/Foo` works.
            let (owner, repo) = rest.split_once('/').with_context(|| {
                format!("github source must be 'github:owner/repo' (got {spec})")
            })?;
            let owner = owner.trim();
            let repo = repo.trim().trim_end_matches('/');
            if owner.is_empty() || repo.is_empty() {
                bail!("github source must be 'github:owner/repo' (got {spec})");
            }
            if owner.contains('/') || repo.contains('/') {
                bail!("github source must be 'github:owner/repo' (got {spec})");
            }
            return Ok(Self::GitHubRepo(format!("{owner}/{repo}")));
        }
        if trimmed.starts_with("https://") || trimmed.starts_with("http://") {
            if let Some(repo) = parse_github_browser_url(trimmed) {
                return Ok(Self::GitHubRepo(repo));
            }
            return Ok(Self::DirectUrl(trimmed.to_string()));
        }
        Ok(Self::Registry(trimmed.to_string()))
    }
}

/// Detect bare `https://github.com/<owner>/<repo>` URLs (with or without a
/// trailing `.git`) and return `owner/repo`. Returns `None` for any URL that
/// already points at a specific archive / blob / tree path — those are real
/// direct URLs and the caller fetches them as-is.
pub(super) fn parse_github_browser_url(url: &str) -> Option<String> {
    let after_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, rest) = after_scheme.split_once('/')?;
    if !host.eq_ignore_ascii_case("github.com") && !host.eq_ignore_ascii_case("www.github.com") {
        return None;
    }
    let trimmed = rest.trim_end_matches('/');
    let mut parts = trimmed.splitn(3, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim().trim_end_matches(".git");
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    // If there is a third segment, the URL points at a sub-resource
    // (`/archive/...`, `/blob/...`, `/tree/...`). Treat that as a real direct
    // URL — the user explicitly wants whatever lives at that path.
    if parts.next().is_some() {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}
/// Outcome of an install attempt.
#[derive(Debug)]
pub enum InstallOutcome {
    /// The skill was installed (or already present and idempotent).
    Installed(InstalledSkill),
    /// The host requires user approval before the install can proceed. The
    /// caller should surface this through whatever approval pathway it has and
    /// retry once approved (typically by adding the host to the policy's
    /// allow list).
    NeedsApproval(String),
    /// The host is denied by network policy. The install is aborted.
    NetworkDenied(String),
}

/// Metadata for a successfully installed skill.
#[derive(Debug, Clone)]
pub struct InstalledSkill {
    /// Skill name (taken from SKILL.md frontmatter).
    pub name: String,
    /// Final on-disk path: `<skills_dir>/<name>/`.
    pub path: PathBuf,
    /// SHA-256 over the downloaded tarball bytes. Used by [`update`] to detect
    /// upstream changes without re-extracting; also surfaced for telemetry /
    /// future signature-verification work.
    #[allow(dead_code)]
    pub source_checksum: String,
}

/// Result of an [`update`] call.
#[derive(Debug)]
pub enum UpdateResult {
    /// Upstream tarball is byte-identical to the on-disk checksum; no action.
    NoChange,
    /// Upstream changed and the on-disk install was atomically replaced.
    Updated(InstalledSkill),
    /// Network policy short-circuited the update. Same semantics as
    /// [`InstallOutcome::NeedsApproval`].
    NeedsApproval(String),
    /// Network policy denied the update.
    NetworkDenied(String),
}

/// Errors that can happen during install. Most variants are flattened into
/// `anyhow::Error` at the public boundary; this enum is used internally so
/// tests can pattern-match without parsing strings.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error("entry escapes destination directory: {0}")]
    PathTraversal(String),
    #[error("entry is too large; uncompressed total would exceed {limit} bytes")]
    OversizedTarball { limit: u64 },
    #[error("missing SKILL.md in archive")]
    MissingSkillMd,
    #[error("SKILL.md frontmatter missing required field: {0}")]
    MissingFrontmatterField(&'static str),
    #[error("symlinks are not allowed in skill tarballs")]
    SymlinkRejected,
    #[error("skill '{0}' is already installed; use update or remove it first")]
    AlreadyInstalled(String),
    #[error("skill '{0}' was not installed via /skill install (no .installed-from marker)")]
    NotInstalledHere(String),
}
#[derive(Debug)]
pub enum SkillSyncOutcome {
    /// Skill downloaded and written to the cache directory.
    Downloaded { name: String, path: PathBuf },
    /// Cached bytes match the upstream ETag / SHA-256; nothing written.
    Fresh { name: String },
    /// Skill download failed; the error is non-fatal so the sync continues.
    Failed { name: String, reason: String },
    /// Network policy blocked the download host.
    Denied { name: String, host: String },
    /// Network policy requires user approval for the download host.
    NeedsApproval { name: String, host: String },
}

/// Overall result of [`sync_registry`].
#[derive(Debug)]
pub enum SyncResult {
    /// Sync completed. `outcomes` contains one entry per skill in the index.
    Done { outcomes: Vec<SkillSyncOutcome> },
    /// The registry fetch was blocked by network policy.
    RegistryDenied(String),
    /// The registry fetch requires user approval.
    RegistryNeedsApproval(String),
}
/// Curated-registry document. The shape is intentionally minimal so adding
/// optional metadata later (homepage, version, signature) is forward-compatible.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryDocument {
    /// Map of skill name → entry.
    #[serde(default)]
    pub skills: std::collections::BTreeMap<String, RegistryEntry>,
}

/// One row in the curated registry. `description` is optional so old indices
/// keep parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryEntry {
    /// Source spec (e.g. `github:owner/repo`).
    pub source: String,
    /// Optional human-readable description.
    #[serde(default)]
    pub description: Option<String>,
}

/// Successful registry fetch result. Same shape as [`InstallOutcome`] for the
/// network-policy outcomes so the caller can drop directly into approval flow.
#[derive(Debug)]
pub enum RegistryFetchResult {
    Loaded(RegistryDocument),
    NeedsApproval(String),
    Denied(String),
}

pub(super) enum UrlResolution {
    Resolved(Vec<String>),
    NeedsApproval(String),
    Denied(String),
}

pub(super) enum DownloadOutcome {
    Bytes { bytes: Vec<u8>, url: String },
    NeedsApproval(String),
    Denied(String),
}
pub(super) enum DownloadAttempt {
    Bytes(Vec<u8>),
    NotFound(reqwest::StatusCode),
}
