//! Shared path normalization for evidence citations and claim matching.

/// Normalize a repo path for comparison / anchor keys.
///
/// - `\` → `/`
/// - strip Windows verbatim (`//?/` / `//./`) prefixes
/// - strip leading `./`
/// - keep drive-letter absolute forms for suffix matching (callers should
///   prefer `ends_with` against workspace-relative keys)
#[must_use]
pub fn normalize_repo_path(path: &str) -> String {
    let mut s = path
        .trim()
        .trim_matches(|c: char| matches!(c, '`' | '"' | '\'' | ',' | ')' | '('))
        .replace('\\', "/");

    for prefix in ["//?/", "//./", "/?/"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }

    while s.starts_with("./") {
        s = s[2..].to_string();
    }

    // Drop a trailing slash (except root-like leftovers).
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }

    s
}

/// True when `cited` and `claimed` refer to the same path under suffix rules.
#[must_use]
pub fn repo_paths_match(cited: &str, claimed: &str) -> bool {
    let cited = normalize_repo_path(cited);
    let claimed = normalize_repo_path(claimed);
    if cited.is_empty() || claimed.is_empty() {
        return false;
    }
    if cited == claimed {
        return true;
    }
    // After stripping drive (`F:/repo/crates/a.rs` vs `crates/a.rs`).
    let cited_tail = strip_windows_drive(&cited);
    let claimed_tail = strip_windows_drive(&claimed);
    if cited_tail == claimed_tail {
        return true;
    }
    let (longer, shorter) = if cited_tail.len() >= claimed_tail.len() {
        (cited_tail, claimed_tail)
    } else {
        (claimed_tail, cited_tail)
    };
    if longer.ends_with(shorter) {
        let prefix_len = longer.len() - shorter.len();
        return prefix_len == 0 || longer.as_bytes().get(prefix_len - 1) == Some(&b'/');
    }
    false
}

fn strip_windows_drive(path: &str) -> &str {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let rest = &path[2..];
        return rest.strip_prefix('/').unwrap_or(rest);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_verbatim_prefix() {
        assert_eq!(
            normalize_repo_path("//?/F:/DeepSeek-TUI-desktop/crates/core/src/lib.rs"),
            "F:/DeepSeek-TUI-desktop/crates/core/src/lib.rs"
        );
    }

    #[test]
    fn matches_absolute_to_relative() {
        assert!(repo_paths_match(
            "//?/F:/repo/crates/core/src/engine/citation_auditor.rs",
            "citation_auditor.rs"
        ));
        assert!(repo_paths_match(
            "F:/repo/crates/core/src/lib.rs",
            "crates/core/src/lib.rs"
        ));
        assert!(!repo_paths_match("src/ba.rs", "a.rs"));
    }
}
