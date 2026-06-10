//! Resolve and materialize bundled sandbox helper executables into `.sandbox-bin`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, anyhow};

use crate::logging::log_note;
use crate::paths::{sandbox_bin_dir, shared_sandbox_bin_dir};

pub(crate) const RESOURCES_DIRNAME: &str = "zagens-resources";

const DEV_BUILD_VERSION_SENTINEL: &str = "0.0.0";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum HelperExecutable {
    Setup,
    CommandRunner,
}

impl HelperExecutable {
    pub(crate) fn file_name(self) -> &'static str {
        match self {
            Self::Setup => "zagens-sandbox-setup.exe",
            Self::CommandRunner => "zagens-command-runner.exe",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Setup => "sandbox-setup",
            Self::CommandRunner => "command-runner",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CopyOutcome {
    Reused,
    ReCopied,
}

static HELPER_PATH_CACHE: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

pub(crate) fn resolve_helper_for_launch(
    kind: HelperExecutable,
    zagens_home: &Path,
    log_dir: Option<&Path>,
) -> PathBuf {
    match copy_helper_if_needed(kind, zagens_home, log_dir) {
        Ok(path) => path,
        Err(err) => {
            let fallback = legacy_lookup(kind);
            log_note(
                &format!(
                    "helper copy failed for {}: {err:#}; falling back to {}",
                    kind.label(),
                    fallback.display()
                ),
                log_dir,
            );
            fallback
        }
    }
}

pub(crate) fn bundled_executable_path_for_exe(exe: &Path, file_name: &str) -> Option<PathBuf> {
    let mut best: Option<(PathBuf, std::time::SystemTime)> = None;
    let mut dir = exe.parent();
    for _ in 0..5 {
        let Some(current) = dir else { break };
        for candidate in std::iter::once(current.join(file_name)).chain(
            resource_search_dirs(current)
                .into_iter()
                .map(|d| d.join(file_name)),
        ) {
            if !candidate.is_file() {
                continue;
            }
            let modified = std::fs::metadata(&candidate)
                .ok()
                .and_then(|m| m.modified().ok());
            let replace = match (&best, modified) {
                (None, Some(_)) => true,
                (Some((_, prev)), Some(ts)) => ts > *prev,
                (None, None) => true,
                _ => false,
            };
            if replace {
                best = Some((candidate, modified.unwrap_or(std::time::UNIX_EPOCH)));
            }
        }
        dir = current.parent();
    }
    best.map(|(path, _)| path)
}

fn resource_search_dirs(exe_dir: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![exe_dir.join(RESOURCES_DIRNAME)];
    if exe_dir.file_name() == Some(OsStr::new("bin"))
        && let Some(parent) = exe_dir.parent()
    {
        dirs.push(parent.join(RESOURCES_DIRNAME));
    }
    if let Some(parent) = exe_dir.parent() {
        dirs.push(parent.join(RESOURCES_DIRNAME));
    }
    dirs
}

fn legacy_lookup(kind: HelperExecutable) -> PathBuf {
    if let Ok(exe) = std::env::current_exe()
        && let Some(candidate) = bundled_executable_path_for_exe(&exe, kind.file_name())
    {
        return candidate;
    }
    PathBuf::from(kind.file_name())
}

fn helper_destination_dir(kind: HelperExecutable, zagens_home: &Path) -> PathBuf {
    match kind {
        HelperExecutable::CommandRunner => shared_sandbox_bin_dir(),
        HelperExecutable::Setup => sandbox_bin_dir(zagens_home),
    }
}

fn find_materialized_helper_in_sandbox_bin(
    kind: HelperExecutable,
    zagens_home: &Path,
) -> Option<PathBuf> {
    let bin_dir = helper_destination_dir(kind, zagens_home);
    let Ok(entries) = std::fs::read_dir(&bin_dir) else {
        return None;
    };
    let prefix = kind.file_name().trim_end_matches(".exe");
    let mut matches: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".exe"))
        })
        .collect();
    matches.sort();
    matches.pop()
}

fn copy_helper_if_needed(
    kind: HelperExecutable,
    zagens_home: &Path,
    log_dir: Option<&Path>,
) -> Result<PathBuf> {
    let cache_key = format!("{}|{}", kind.file_name(), zagens_home.display());
    if let Some(path) = cached_helper_path(&cache_key) {
        return Ok(path);
    }

    let source = sibling_source_path(kind)?;
    let destination = helper_destination_for_source(kind, zagens_home, &source)?;
    if let Some(existing) = find_materialized_helper_in_sandbox_bin(kind, zagens_home)
        && existing == destination
        && destination_is_fresh(&source, &existing).unwrap_or(false)
    {
        store_helper_path(cache_key, existing.clone());
        return Ok(existing);
    }
    let outcome = copy_from_source_if_needed(&source, &destination)?;
    log_note(
        &format!(
            "helper {} {} -> {}",
            match outcome {
                CopyOutcome::Reused => "reused",
                CopyOutcome::ReCopied => "copied",
            },
            kind.label(),
            destination.display()
        ),
        log_dir,
    );
    store_helper_path(cache_key, destination.clone());
    Ok(destination)
}

fn cached_helper_path(cache_key: &str) -> Option<PathBuf> {
    let cache = HELPER_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    cache.lock().ok()?.get(cache_key).cloned()
}

fn store_helper_path(cache_key: String, path: PathBuf) {
    let cache = HELPER_PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(cache_key, path);
    }
}

fn sibling_source_path(kind: HelperExecutable) -> Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current executable for helper lookup")?;
    bundled_executable_path_for_exe(&exe, kind.file_name()).ok_or_else(|| {
        anyhow!(
            "helper not found next to {} or under {RESOURCES_DIRNAME}",
            exe.display()
        )
    })
}

fn helper_destination_for_source(
    kind: HelperExecutable,
    zagens_home: &Path,
    source: &Path,
) -> Result<PathBuf> {
    let suffix = helper_version_suffix(source)?;
    Ok(helper_destination_dir(kind, zagens_home).join(materialized_file_name(kind, &suffix)))
}

fn materialized_file_name(kind: HelperExecutable, suffix: &str) -> String {
    let source_name = kind.file_name();
    let path = Path::new(source_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(source_name);
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| format!(".{ext}"))
        .unwrap_or_default();
    format!("{stem}-{suffix}{extension}")
}

fn helper_version_suffix(source: &Path) -> Result<String> {
    let version = env!("CARGO_PKG_VERSION");
    if version == DEV_BUILD_VERSION_SENTINEL {
        dev_build_suffix(source)
    } else {
        Ok(version.to_string())
    }
}

fn dev_build_suffix(source: &Path) -> Result<String> {
    let metadata = fs::metadata(source)
        .with_context(|| format!("read helper metadata {}", source.display()))?;
    let modified = metadata
        .modified()
        .with_context(|| format!("read helper mtime {}", source.display()))?;
    let duration = modified
        .duration_since(UNIX_EPOCH)
        .with_context(|| format!("convert helper mtime {}", source.display()))?;
    Ok(format!("{}-{:x}", metadata.len(), duration.as_secs()))
}

fn copy_from_source_if_needed(source: &Path, destination: &Path) -> Result<CopyOutcome> {
    if destination_is_fresh(source, destination)? {
        return Ok(CopyOutcome::Reused);
    }

    let destination_dir = destination.parent().ok_or_else(|| {
        anyhow!(
            "helper destination has no parent: {}",
            destination.display()
        )
    })?;
    fs::create_dir_all(destination_dir).with_context(|| {
        format!(
            "create helper destination dir {}",
            destination_dir.display()
        )
    })?;

    let tmp = destination.with_extension("tmp");
    fs::copy(source, &tmp)
        .with_context(|| format!("copy helper from {} to {}", source.display(), tmp.display()))?;
    if tmp.is_file() {
        if destination.exists() {
            fs::remove_file(destination).ok();
        }
        fs::rename(&tmp, destination).with_context(|| {
            format!(
                "rename helper temp {} -> {}",
                tmp.display(),
                destination.display()
            )
        })?;
    }
    Ok(CopyOutcome::ReCopied)
}

fn destination_is_fresh(source: &Path, destination: &Path) -> Result<bool> {
    if !destination.is_file() {
        return Ok(false);
    }
    let src_meta = fs::metadata(source)?;
    let dst_meta = fs::metadata(destination)?;
    Ok(src_meta.len() == dst_meta.len() && src_meta.modified()? <= dst_meta.modified()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn materialized_name_includes_version_suffix() {
        let name = materialized_file_name(HelperExecutable::CommandRunner, "0.7.4");
        assert_eq!(name, "zagens-command-runner-0.7.4.exe");
    }
}
