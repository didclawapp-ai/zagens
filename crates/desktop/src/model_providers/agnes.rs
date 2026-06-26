//! Agnes AI catalog quirks (B-tier: chat-only filter + output limit table).

use super::spec::CatalogEntry;

const AGNES_CHAT_MAX_OUTPUT_TOKENS: u32 = 65_536;

static AGNES_OUTPUT_LIMITS: &[(&str, u32)] = &[("agnes-", AGNES_CHAT_MAX_OUTPUT_TOKENS)];

pub fn agnes_keep(entry: &CatalogEntry) -> bool {
    is_agnes_chat_model_id(&entry.id)
}

pub fn is_agnes_chat_model_id(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    if lower.trim().is_empty() {
        return false;
    }
    !(lower.contains("embed")
        || lower.contains("moderation")
        || lower.contains("image")
        || lower.contains("video"))
}

pub fn agnes_output_limit(entry: &CatalogEntry) -> Option<u32> {
    if let Some(v) = entry
        .max_output_length
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v > 0)
    {
        return Some(v);
    }
    for (prefix, limit) in AGNES_OUTPUT_LIMITS {
        if entry.id.starts_with(prefix) {
            return Some(*limit);
        }
    }
    is_agnes_chat_model_id(&entry.id).then_some(AGNES_CHAT_MAX_OUTPUT_TOKENS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_providers::spec::CatalogEntry;

    #[test]
    fn chat_model_filter_excludes_image_and_video() {
        assert!(is_agnes_chat_model_id("agnes-2.0-flash"));
        assert!(!is_agnes_chat_model_id("agnes-image-2.0-flash"));
        assert!(!is_agnes_chat_model_id("agnes-video-v2.0"));
    }

    #[test]
    fn output_limit_prefers_api_field() {
        let entry = CatalogEntry {
            id: "agnes-2.0-flash".into(),
            name: "flash".into(),
            context_length: None,
            max_output_length: Some(32_768),
            description: None,
            raw: serde_json::json!({}),
        };
        assert_eq!(agnes_output_limit(&entry), Some(32_768));
    }
}
