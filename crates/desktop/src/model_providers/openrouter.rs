//! OpenRouter catalog quirks (B-tier: free/paid + top_provider output limits).

use super::spec::CatalogEntry;

pub fn openrouter_keep(entry: &CatalogEntry) -> bool {
    let id_lower = entry.id.to_ascii_lowercase();
    !id_lower.contains("embed") && !id_lower.contains("moderation")
}

fn pricing_is_zero(raw: &str) -> bool {
    match raw.trim().parse::<f64>() {
        Ok(v) => v == 0.0,
        Err(_) => raw.trim() == "0",
    }
}

pub fn openrouter_entry_is_free(entry: &CatalogEntry) -> bool {
    let id_lower = entry.id.to_ascii_lowercase();
    if id_lower.contains(":free") || id_lower.ends_with("-free") {
        return true;
    }
    let Some(pricing) = entry.raw.get("pricing") else {
        return false;
    };
    let prompt_free = pricing
        .get("prompt")
        .and_then(|v| v.as_str())
        .is_some_and(pricing_is_zero);
    let completion_free = pricing
        .get("completion")
        .and_then(|v| v.as_str())
        .is_some_and(pricing_is_zero);
    prompt_free && completion_free
}

pub fn openrouter_output_limit(entry: &CatalogEntry) -> Option<u32> {
    entry
        .raw
        .get("top_provider")
        .and_then(|tp| tp.get("max_completion_tokens"))
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v > 0 && v <= 1_000_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_providers::spec::CatalogEntry;

    fn entry(id: &str, raw: serde_json::Value) -> CatalogEntry {
        CatalogEntry {
            id: id.to_string(),
            name: id.to_string(),
            context_length: None,
            max_output_length: None,
            description: None,
            raw,
        }
    }

    #[test]
    fn free_when_pricing_zero() {
        let e = entry(
            "meta-llama/llama-3.3-70b",
            serde_json::json!({"pricing": {"prompt": "0", "completion": "0"}}),
        );
        assert!(openrouter_entry_is_free(&e));
    }

    #[test]
    fn free_when_id_suffix() {
        let e = entry("deepseek/deepseek-r1:free", serde_json::json!({}));
        assert!(openrouter_entry_is_free(&e));
    }

    #[test]
    fn paid_when_prompt_nonzero() {
        let e = entry(
            "openai/gpt-4.1",
            serde_json::json!({"pricing": {"prompt": "0.000003", "completion": "0"}}),
        );
        assert!(!openrouter_entry_is_free(&e));
    }
}
