//! OpenAI-compatible provider base URL normalization (shared by runtime + desktop).

/// True when the URL already ends with a known API version segment (`/v1`, `/v4`, `/beta`, …).
#[must_use]
pub fn has_trailing_api_version_segment(base_url: &str) -> bool {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/beta") {
        return true;
    }
    trimmed
        .rsplit('/')
        .next()
        .is_some_and(is_numeric_version_segment)
}

fn is_numeric_version_segment(segment: &str) -> bool {
    let Some(digits) = segment.strip_prefix('v') else {
        return false;
    };
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

/// Ensure an OpenAI-compatible base URL ends with a version segment.
///
/// Bare hosts get `/v1` appended; URLs that already end with `/v1`, `/v4`, `/beta`, etc. are kept.
#[must_use]
pub fn versioned_openai_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if has_trailing_api_version_segment(trimmed) {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

/// Strip a trailing `/beta` or `/vN` segment so DeepSeek-style `beta/…` paths can be rooted.
#[must_use]
pub fn unversioned_openai_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/beta") {
        return trimmed.strip_suffix("/beta").unwrap_or(trimmed).to_string();
    }
    if let Some(segment) = trimmed.rsplit('/').next()
        && is_numeric_version_segment(segment)
        && let Some((prefix, _)) = trimmed.rsplit_once('/')
        && !prefix.is_empty()
    {
        return prefix.to_string();
    }
    trimmed.to_string()
}

/// Build an OpenAI-compatible request URL from a configured base URL and relative path.
#[must_use]
pub fn openai_compatible_api_url(base_url: &str, path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.starts_with("beta/") {
        return format!(
            "{}/{}",
            unversioned_openai_base_url(base_url).trim_end_matches('/'),
            path
        );
    }
    format!(
        "{}/{}",
        versioned_openai_base_url(base_url).trim_end_matches('/'),
        path
    )
}

/// Resolve the `/models` probe URL for an OpenAI-compatible provider base URL.
#[must_use]
pub fn openai_compatible_models_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    let is_deepseek_host = trimmed
        .split("://")
        .nth(1)
        .is_some_and(|rest| rest.starts_with("api.deepseek.com"));
    if is_deepseek_host {
        let root = trimmed
            .strip_suffix("/v1")
            .or_else(|| trimmed.strip_suffix("/beta"))
            .unwrap_or(trimmed);
        return format!("{root}/models");
    }
    if has_trailing_api_version_segment(trimmed) {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_trailing_version_segments() {
        assert!(has_trailing_api_version_segment(
            "https://open.bigmodel.cn/api/paas/v4"
        ));
        assert!(has_trailing_api_version_segment(
            "https://api.example.com/v1"
        ));
        assert!(has_trailing_api_version_segment(
            "https://api.deepseek.com/beta"
        ));
        assert!(!has_trailing_api_version_segment("https://api.example.com"));
    }

    #[test]
    fn versioned_base_url_keeps_v4_and_appends_v1_for_bare_hosts() {
        assert_eq!(
            versioned_openai_base_url("https://open.bigmodel.cn/api/paas/v4"),
            "https://open.bigmodel.cn/api/paas/v4"
        );
        assert_eq!(
            versioned_openai_base_url("https://api.example.com"),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn api_url_supports_zhipu_v4_chat_completions() {
        assert_eq!(
            openai_compatible_api_url("https://open.bigmodel.cn/api/paas/v4", "chat/completions"),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
    }

    #[test]
    fn api_url_preserves_deepseek_beta_routing() {
        assert_eq!(
            openai_compatible_api_url("https://api.deepseek.com", "chat/completions"),
            "https://api.deepseek.com/v1/chat/completions"
        );
        assert_eq!(
            openai_compatible_api_url("https://api.deepseek.com/beta", "chat/completions"),
            "https://api.deepseek.com/beta/chat/completions"
        );
        assert_eq!(
            openai_compatible_api_url("https://api.deepseek.com", "beta/completions"),
            "https://api.deepseek.com/beta/completions"
        );
    }

    #[test]
    fn models_url_supports_v4_and_deepseek_root_models() {
        assert_eq!(
            openai_compatible_models_url("https://open.bigmodel.cn/api/paas/v4"),
            "https://open.bigmodel.cn/api/paas/v4/models"
        );
        assert_eq!(
            openai_compatible_models_url("https://api.deepseek.com/beta"),
            "https://api.deepseek.com/models"
        );
    }
}
