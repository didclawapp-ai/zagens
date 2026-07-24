//! Shared model capability catalog (`crates/shared-defs/model-catalog.json`, vendored
//! as `crates/core/model-catalog.json` for `cargo publish`).
//!
//! Embeds the JSON at compile time and evaluates match rules on a lowercased
//! model id. First matching family wins; otherwise defaults apply with
//! `family_id = None`.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const CATALOG_JSON: &str = include_str!("../model-catalog.json");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCaps {
    /// Matched family id, or `None` when only catalog defaults apply.
    pub family_id: Option<&'static str>,
    pub context_window: u32,
    pub max_output: u32,
    pub default_max_output: Option<u32>,
    pub omit_sampling: bool,
    pub always_thinking: bool,
    pub thinking_supported: bool,
    /// When `Some`, the family defines a custom `reasoning_effort` wire map.
    pub has_effort_map: bool,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    defaults: CapsFields,
    families: Vec<FamilyDef>,
}

#[derive(Debug, Deserialize)]
struct CapsFields {
    context_window: u32,
    max_output: u32,
    #[serde(default)]
    default_max_output: Option<u32>,
    #[serde(default)]
    omit_sampling: bool,
    #[serde(default)]
    always_thinking: bool,
    #[serde(default)]
    thinking_supported: bool,
}

#[derive(Debug, Deserialize)]
struct FamilyDef {
    id: String,
    #[serde(rename = "match")]
    match_rule: MatchNode,
    context_window: u32,
    max_output: u32,
    #[serde(default)]
    default_max_output: Option<u32>,
    #[serde(default)]
    omit_sampling: bool,
    #[serde(default)]
    always_thinking: bool,
    #[serde(default)]
    thinking_supported: bool,
    /// Maps UI/config effort aliases → wire `reasoning_effort` values.
    /// Presence means: set `reasoning_effort` only (no `thinking: disabled` object).
    #[serde(default)]
    effort_map: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum MatchNode {
    All { all: Vec<MatchNode> },
    Any { any: Vec<MatchNode> },
    Leaf(MatchLeaf),
}

#[derive(Debug, Deserialize)]
struct MatchLeaf {
    #[serde(default)]
    contains: Option<String>,
    #[serde(default)]
    starts_with: Option<String>,
    #[serde(default)]
    equals: Option<String>,
    #[serde(default)]
    not_contains: Option<String>,
}

struct Catalog {
    defaults: CapsFields,
    families: Vec<FamilyDef>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let file: CatalogFile =
            serde_json::from_str(CATALOG_JSON).expect("shared-defs/model-catalog.json must parse");
        Catalog {
            defaults: file.defaults,
            families: file.families,
        }
    })
}

fn eval_match(node: &MatchNode, model_lower: &str) -> bool {
    match node {
        MatchNode::All { all } => all.iter().all(|n| eval_match(n, model_lower)),
        MatchNode::Any { any } => any.iter().any(|n| eval_match(n, model_lower)),
        MatchNode::Leaf(leaf) => {
            let mut ok = true;
            let mut any_pred = false;
            if let Some(v) = leaf.contains.as_deref() {
                any_pred = true;
                ok &= model_lower.contains(&v.to_ascii_lowercase());
            }
            if let Some(v) = leaf.starts_with.as_deref() {
                any_pred = true;
                ok &= model_lower.starts_with(&v.to_ascii_lowercase());
            }
            if let Some(v) = leaf.equals.as_deref() {
                any_pred = true;
                ok &= model_lower == v.to_ascii_lowercase();
            }
            if let Some(v) = leaf.not_contains.as_deref() {
                any_pred = true;
                ok &= !model_lower.contains(&v.to_ascii_lowercase());
            }
            any_pred && ok
        }
    }
}

fn intern_str(id: &str) -> &'static str {
    static MAP: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let map = MAP.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = map.lock().expect("catalog string intern lock");
    if let Some(existing) = guard.get(id) {
        return existing;
    }
    let leaked: &'static str = Box::leak(id.to_owned().into_boxed_str());
    guard.insert(id.to_owned(), leaked);
    leaked
}

fn caps_from_family(family: &FamilyDef) -> ModelCaps {
    ModelCaps {
        family_id: Some(intern_str(&family.id)),
        context_window: family.context_window,
        max_output: family.max_output,
        default_max_output: family.default_max_output,
        omit_sampling: family.omit_sampling,
        always_thinking: family.always_thinking,
        thinking_supported: family.thinking_supported,
        has_effort_map: family.effort_map.as_ref().is_some_and(|m| !m.is_empty()),
    }
}

/// Resolve capability flags for a model id from the shared catalog.
#[must_use]
pub fn resolve_model_caps(model: &str) -> ModelCaps {
    let lower = model.to_ascii_lowercase();
    let cat = catalog();
    for family in &cat.families {
        if eval_match(&family.match_rule, &lower) {
            return caps_from_family(family);
        }
    }
    ModelCaps {
        family_id: None,
        context_window: cat.defaults.context_window,
        max_output: cat.defaults.max_output,
        default_max_output: cat.defaults.default_max_output,
        omit_sampling: cat.defaults.omit_sampling,
        always_thinking: cat.defaults.always_thinking,
        thinking_supported: cat.defaults.thinking_supported,
        has_effort_map: false,
    }
}

/// Map a UI/config reasoning-effort alias through the family's `effort_map`.
///
/// Returns `Some(wire_value)` when the matched family defines `effort_map`
/// (caller should set `reasoning_effort` only). Returns `None` when the family
/// has no map — caller keeps the provider-dialect path in `apply_reasoning_effort`.
#[must_use]
pub fn map_model_reasoning_effort(model: &str, effort: &str) -> Option<&'static str> {
    let lower = model.to_ascii_lowercase();
    let cat = catalog();
    for family in &cat.families {
        if !eval_match(&family.match_rule, &lower) {
            continue;
        }
        let map = family.effort_map.as_ref()?;
        if map.is_empty() {
            return None;
        }
        let key = {
            let t = effort.trim().to_ascii_lowercase();
            if t.is_empty() {
                "default".to_string()
            } else {
                t
            }
        };
        let mapped = map
            .get(&key)
            .or_else(|| map.get("default"))
            .map(String::as_str)
            .unwrap_or("max");
        return Some(intern_str(mapped));
    }
    None
}

/// Whether the model matched the `deepseek_v4` family.
#[must_use]
pub fn is_deepseek_v4_model(model: &str) -> bool {
    resolve_model_caps(model).family_id == Some("deepseek_v4")
}

/// Whether the model matched the `kimi_k3` family.
#[must_use]
pub fn is_kimi_k3_model(model: &str) -> bool {
    resolve_model_caps(model).family_id == Some("kimi_k3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_parses_and_matches_known_families() {
        let v4 = resolve_model_caps("deepseek-v4-pro");
        assert_eq!(v4.family_id, Some("deepseek_v4"));
        assert_eq!(v4.context_window, 1_000_000);
        assert_eq!(v4.max_output, 393_216);
        assert!(v4.thinking_supported);
        assert!(!v4.has_effort_map);

        let k3 = resolve_model_caps("kimi-k3");
        assert_eq!(k3.family_id, Some("kimi_k3"));
        assert!(k3.omit_sampling);
        assert!(k3.always_thinking);
        assert_eq!(k3.max_output, 1_048_576);
        assert!(k3.has_effort_map);

        let agnes = resolve_model_caps("agnes-2.0-flash");
        assert_eq!(agnes.family_id, Some("agnes_chat"));
        assert_eq!(agnes.context_window, 256_000);

        assert_ne!(
            resolve_model_caps("agnes-image-2.0-flash").family_id,
            Some("agnes_chat")
        );
    }

    #[test]
    fn kimi_k3_effort_map_aliases() {
        assert_eq!(map_model_reasoning_effort("kimi-k3", "low"), Some("low"));
        assert_eq!(map_model_reasoning_effort("kimi-k3", "high"), Some("high"));
        assert_eq!(map_model_reasoning_effort("kimi-k3", "max"), Some("max"));
        assert_eq!(map_model_reasoning_effort("kimi-k3", "off"), Some("max"));
        assert_eq!(map_model_reasoning_effort("kimi-k3", ""), Some("high"));
        assert_eq!(
            map_model_reasoning_effort("kimi-k3", "medium"),
            Some("high")
        );
        // Families without effort_map leave provider dialect in charge.
        assert_eq!(map_model_reasoning_effort("deepseek-v4-pro", "off"), None);
    }

    #[test]
    fn unknown_model_uses_defaults_without_family() {
        let caps = resolve_model_caps("totally-unknown-xyz");
        assert_eq!(caps.family_id, None);
        assert_eq!(caps.context_window, 128_000);
        assert_eq!(caps.max_output, 65_536);
    }

    #[test]
    fn kimi_public_consts_match_catalog() {
        let k3 = resolve_model_caps("kimi-k3");
        assert_eq!(
            k3.context_window,
            crate::chat::KIMI_K3_CONTEXT_WINDOW_TOKENS
        );
        assert_eq!(k3.max_output, crate::chat::KIMI_K3_MAX_OUTPUT_TOKENS);
        assert_eq!(
            k3.default_max_output,
            Some(crate::chat::KIMI_K3_DEFAULT_MAX_OUTPUT_TOKENS)
        );
    }
}
