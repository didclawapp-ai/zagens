//! Shared exact search/replace for `edit_file` and `batch_edit`.

use super::write::find_match_line_numbers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SearchReplaceMode {
    First,
    All,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SearchReplaceError {
    EmptySearch,
    NotFound,
    Ambiguous {
        count: usize,
        sample_lines: Vec<usize>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchReplaceOutcome {
    pub updated: String,
    pub match_count: usize,
}

fn normalize_for_file(search: &str, replace: &str, file_le: &str) -> (String, String) {
    if file_le == "\r\n" {
        let s = search.replace("\r\n", "\n").replace('\n', "\r\n");
        let r = replace.replace("\r\n", "\n").replace('\n', "\r\n");
        (s, r)
    } else {
        (search.to_string(), replace.to_string())
    }
}

/// Apply exact search/replace to whole-file `contents`.
///
/// When `implicit_all` is true (batch_edit), multiple matches replace without
/// requiring an explicit `replace_mode`. When false (edit_file), multiple
/// matches without `mode` return `Ambiguous`.
pub(crate) fn apply_search_replace(
    contents: &str,
    search: &str,
    replace: &str,
    mode: Option<SearchReplaceMode>,
    implicit_all: bool,
) -> Result<SearchReplaceOutcome, SearchReplaceError> {
    if search.trim().is_empty() {
        return Err(SearchReplaceError::EmptySearch);
    }

    let file_le = if contents.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let (search_norm, replace_norm) = normalize_for_file(search, replace, file_le);
    let count = contents.matches(&search_norm).count();

    if count == 0 {
        return Err(SearchReplaceError::NotFound);
    }

    let effective_mode = mode.or(if implicit_all {
        Some(SearchReplaceMode::All)
    } else {
        None
    });

    if count > 1 && effective_mode.is_none() {
        let sample_lines = find_match_line_numbers(contents, &search_norm, 3);
        return Err(SearchReplaceError::Ambiguous {
            count,
            sample_lines,
        });
    }

    let updated = match effective_mode {
        Some(SearchReplaceMode::First) => contents.replacen(&search_norm, &replace_norm, 1),
        Some(SearchReplaceMode::All) | None => contents.replace(&search_norm, &replace_norm),
    };

    Ok(SearchReplaceOutcome {
        updated,
        match_count: count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_search() {
        assert_eq!(
            apply_search_replace("hello", "  ", "x", None, true),
            Err(SearchReplaceError::EmptySearch)
        );
    }

    #[test]
    fn implicit_all_replaces_every_match() {
        let out = apply_search_replace("a a a", "a", "b", None, true).unwrap();
        assert_eq!(out.updated, "b b b");
        assert_eq!(out.match_count, 3);
    }

    #[test]
    fn edit_file_style_requires_mode_for_multiple() {
        assert!(matches!(
            apply_search_replace("a a", "a", "b", None, false),
            Err(SearchReplaceError::Ambiguous { count: 2, .. })
        ));
    }
}
