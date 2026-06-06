//! Community-skill installer (#140).
//!
//! Pulls user-authored skills from GitHub or direct tarball URLs, validates them
//! against a path-traversal- and size-bounded extractor, and writes them into
//! `<skills_dir>/<name>/`. No backend service, no auto-execution: every install
//! is gated by the per-domain [`crate::network_policy::NetworkPolicy`] and
//! validation rejects any tarball entry that escapes the destination directory.
//!
//! Public surface:
//!
//! * [`InstallSource`] — `github:owner/repo`, raw URL, or curated registry
//!   name. Parsed from a single string with [`InstallSource::parse`].
//! * [`install`] / [`update`] / [`uninstall`] — async install, atomic update,
//!   and clean uninstall. All three preserve a `.installed-from` marker so the
//!   bundled `skill-creator` (which lacks the marker) is never touched.
//! * [`InstallOutcome`] — `Installed` / `NeedsApproval(host)` /
//!   `NetworkDenied(host)`. The `NeedsApproval` variant is returned without
//!   side effects so the caller (slash-command, runtime API, etc.) can route
//!   through its own approval flow.
//!
//! # Hard rules
//!
//! * Validation extracts to a temp directory first. The destination path is
//!   only created (via atomic rename) once the tarball clears every check.
//!   Half-installed skills can never appear on disk.
//! * Path traversal rejection covers both `..` segments and absolute paths.
//!   Symlinks inside the selected skill subtree are rejected — there's no use
//!   case for them in a SKILL.md bundle and they're a notorious foothold for
//!   escape. Multi-skill repository archives may contain unrelated symlinks
//!   outside that selected subtree; those entries are ignored and never
//!   extracted.
//! * No `+x` is granted on extracted files. The optional `/skill trust <name>`
//!   command writes a `.trusted` marker; tool-execution gating is a separate
//!   concern that lives next to the tool registry.

mod api;
mod download;
mod local;
mod registry;
mod types;

#[cfg(test)]
mod tests;

pub use api::{install, install_with_registry, update, update_with_registry};
pub use local::{import_local_directory, trust, uninstall};
pub use registry::{fetch_registry, sync_registry};
pub use types::{
    DEFAULT_MAX_SIZE_BYTES, DEFAULT_REGISTRY_URL, INSTALLED_FROM_MARKER, InstallError,
    InstallOutcome, InstallSource, InstalledSkill, RegistryDocument, RegistryEntry,
    RegistryFetchResult, SkillSyncOutcome, SyncResult, TRUSTED_MARKER, UpdateResult,
    default_cache_skills_dir,
};
