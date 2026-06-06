//! Path containment helpers shared by tool execution.

use std::path::{Component, Path, PathBuf};

/// Compare paths for prefix containment, normalizing Windows `\\?\` verbatim prefixes.
#[must_use]
pub fn path_has_prefix(path: &Path, prefix: &Path) -> bool {
    strip_verbatim_prefix(path).starts_with(strip_verbatim_prefix(prefix))
}

#[must_use]
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.display().to_string();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path.to_path_buf()
}

/// Normalize a path by resolving `.` / `..` without touching the filesystem.
#[must_use]
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut prefix: Option<std::ffi::OsString> = None;
    let mut is_root = false;
    let mut stack: Vec<std::ffi::OsString> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix_component) => {
                prefix = Some(prefix_component.as_os_str().to_owned());
            }
            Component::RootDir => {
                is_root = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                let parent = Component::ParentDir.as_os_str();
                if let Some(last) = stack.pop() {
                    if last == parent {
                        stack.push(last);
                        stack.push(parent.to_owned());
                    }
                } else if !is_root {
                    stack.push(parent.to_owned());
                }
            }
            Component::Normal(part) => {
                stack.push(part.to_owned());
            }
        }
    }

    let mut normalized = PathBuf::new();
    if let Some(prefix) = prefix {
        normalized.push(prefix);
    }
    if is_root {
        normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR));
    }
    for part in stack {
        normalized.push(part);
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_resolves_parent() {
        let normalized = normalize_path(Path::new("new/../safe.txt"));
        assert!(normalized.ends_with("safe.txt"));
    }
}
