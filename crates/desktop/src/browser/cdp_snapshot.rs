//! CDP accessibility snapshot (Windows WebView2): frame tree + `Accessibility.getFullAXTree`.

use serde::Deserialize;
use serde_json::Value;

use super::cdp::call_devtools_protocol;
use super::scripts::parse_snapshot_json;
use super::{BrowserA11yNode, BrowserError, BrowserMode, BrowserSnapshotDto, eval_js_string};
use tauri::AppHandle;

const MAX_NODES: usize = 120;

/// Interactive AX roles we surface (aligned with `scripts::ZAGENS_SEL` intent).
const INTERACTIVE_ROLES: &[&str] = &[
    "button",
    "link",
    "textbox",
    "textfield",
    "searchbox",
    "checkbox",
    "radio",
    "combobox",
    "menuitem",
    "tab",
    "option",
    "switch",
    "slider",
    "listbox",
    "heading",
];

#[cfg(windows)]
pub fn is_available() -> bool {
    true
}

#[cfg(not(windows))]
pub fn is_available() -> bool {
    false
}

/// Build snapshot via CDP on Windows; returns `cdp_unsupported` elsewhere.
pub async fn snapshot_via_cdp(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<BrowserSnapshotDto, BrowserError> {
    #[cfg(not(windows))]
    {
        let _ = (app, mode, host_label);
        return Err(BrowserError::msg(
            "cdp_unsupported",
            "CDP snapshot 仅 Windows WebView2 可用",
        ));
    }

    #[cfg(windows)]
    {
        let mut snap = page_meta_via_js(app, mode, host_label).await?;
        let frame_ids = fetch_frame_ids(app, mode, host_label).await?;
        let frame_count = frame_ids.len();
        let mut nodes = Vec::new();
        let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

        for (frame_idx, frame_id) in frame_ids.iter().enumerate() {
            if nodes.len() >= MAX_NODES {
                break;
            }
            let params = if frame_idx == 0 {
                "{}".to_string()
            } else {
                format!(r#"{{"frameId":"{frame_id}"}}"#)
            };
            let raw = call_devtools_protocol(
                app,
                mode,
                host_label,
                "Accessibility.getFullAXTree",
                &params,
            )
            .await?;
            let remaining = MAX_NODES.saturating_sub(nodes.len());
            nodes.extend(parse_ax_tree_nodes(&raw, frame_idx, remaining, &mut counts));
        }

        snap.nodes = nodes;
        if frame_count > 1 {
            snap.iframe_note = Some(format!(
                "{frame_count} frame(s) snapshotted via CDP a11y (refs prefixed f0:, f1:, …)"
            ));
        } else if snap.iframe_note.is_some() {
            // JS meta may still mention iframes; CDP covered main frame only.
            snap.iframe_note = None;
        }
        Ok(snap)
    }
}

async fn page_meta_via_js(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<BrowserSnapshotDto, BrowserError> {
    const META_JS: &str = r#"(function(){
  try {
    var text = document.body ? (document.body.innerText || '').slice(0, 50000) : '';
    return JSON.stringify({ url: location.href || '', title: document.title || '', text: text, nodes: [], iframeCount: 0 });
  } catch (e) {
    return JSON.stringify({ url: location.href || '', title: '', text: String(e), nodes: [], iframeCount: 0 });
  }
})()"#;
    let raw = eval_js_string(app, mode, host_label, META_JS).await?;
    Ok(parse_snapshot_json(&raw))
}

#[cfg(windows)]
async fn fetch_frame_ids(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<Vec<String>, BrowserError> {
    let raw = call_devtools_protocol(app, mode, host_label, "Page.getFrameTree", "{}").await?;
    collect_frame_ids(&raw)
        .ok_or_else(|| BrowserError::msg("cdp_frame_tree", "无法解析 Page.getFrameTree 响应"))
}

/// Depth-first frame id list (main frame first).
pub fn collect_frame_ids(raw: &str) -> Option<Vec<String>> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let root = v.get("frameTree")?;
    let mut out = Vec::new();
    walk_frame_tree(root, &mut out);
    if out.is_empty() { None } else { Some(out) }
}

fn walk_frame_tree(node: &Value, out: &mut Vec<String>) {
    if let Some(id) = node
        .get("frame")
        .and_then(|f| f.get("id"))
        .and_then(|v| v.as_str())
    {
        out.push(id.to_string());
    }
    if let Some(children) = node.get("childFrames").and_then(|c| c.as_array()) {
        for child in children {
            walk_frame_tree(child, out);
        }
    }
}

#[derive(Deserialize)]
struct AxTree {
    #[serde(default)]
    nodes: Vec<AxNode>,
}

#[derive(Deserialize)]
struct AxNode {
    #[serde(default)]
    ignored: bool,
    #[serde(default)]
    role: Option<AxValue>,
    #[serde(default)]
    name: Option<AxValue>,
    #[serde(default)]
    properties: Vec<AxProperty>,
}

#[derive(Deserialize)]
struct AxProperty {
    name: String,
    value: AxValue,
}

#[derive(Deserialize)]
struct AxValue {
    #[serde(default)]
    value: Option<Value>,
}

/// Parse `Accessibility.getFullAXTree` JSON into stable refs (`f{frame}:{role}:{slug}:{nth}`).
pub fn parse_ax_tree_nodes(
    raw: &str,
    frame_idx: usize,
    max_nodes: usize,
    counts: &mut std::collections::HashMap<String, u32>,
) -> Vec<BrowserA11yNode> {
    if max_nodes == 0 {
        return Vec::new();
    }
    let tree: AxTree = match serde_json::from_str(raw) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let prefix = if frame_idx == 0 {
        String::new()
    } else {
        format!("f{frame_idx}:")
    };
    let mut out = Vec::new();
    for node in tree.nodes {
        if node.ignored || ax_hidden(&node) {
            continue;
        }
        let role = ax_role(&node);
        if !INTERACTIVE_ROLES.contains(&role.as_str()) {
            continue;
        }
        let name = ax_name(&node);
        if name.is_empty() && !matches!(role.as_str(), "textbox" | "textfield" | "searchbox") {
            continue;
        }
        let slug = slug(&name);
        let key = format!("{prefix}{role}:{slug}");
        let nth = *counts.entry(key.clone()).or_insert(0);
        *counts.get_mut(&key).unwrap() = nth + 1;
        let r#ref = format!("{prefix}{role}:{slug}:{nth}");
        out.push(BrowserA11yNode { r#ref, role, name });
        if out.len() >= max_nodes {
            break;
        }
    }
    out
}

fn ax_role(node: &AxNode) -> String {
    let raw = node
        .role
        .as_ref()
        .and_then(|r| r.value.as_ref())
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let normalized = raw.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "-");
    match normalized.as_str() {
        "textfield" => "textbox".into(),
        other => {
            let s = other.trim_matches('-');
            if s.is_empty() {
                "unknown".into()
            } else {
                s.to_string()
            }
        }
    }
}

fn ax_name(node: &AxNode) -> String {
    node.name
        .as_ref()
        .and_then(|n| n.value.as_ref())
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .chars()
        .take(120)
        .collect()
}

fn ax_hidden(node: &AxNode) -> bool {
    for prop in &node.properties {
        if (prop.name == "hidden" || prop.name == "aria-hidden")
            && let Some(Value::Bool(b)) = prop.value.value
        {
            return b;
        }
    }
    false
}

/// Same slug rules as `scripts.rs` / injected JS (`zagensSlug`).
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let out = out.trim_matches('-');
    let out = if out.is_empty() { "anon" } else { out };
    out.chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_frame_ids_dfs_main_first() {
        let raw = r#"{
          "frameTree": {
            "frame": {"id": "main", "url": "https://a/"},
            "childFrames": [
              {"frame": {"id": "child1", "url": "https://b/"}, "childFrames": []},
              {"frame": {"id": "child2", "url": "https://c/"}, "childFrames": []}
            ]
          }
        }"#;
        assert_eq!(
            collect_frame_ids(raw).unwrap(),
            vec![
                String::from("main"),
                String::from("child1"),
                String::from("child2")
            ]
        );
    }

    #[test]
    fn parse_ax_tree_builds_frame_prefixed_refs() {
        let raw = r#"{
          "nodes": [
            {"ignored": false, "role": {"value": "button"}, "name": {"value": "Go"}},
            {"ignored": false, "role": {"value": "link"}, "name": {"value": "Home"}}
          ]
        }"#;
        let mut counts = std::collections::HashMap::new();
        let nodes = parse_ax_tree_nodes(raw, 1, 10, &mut counts);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].r#ref, "f1:button:go:0");
        assert_eq!(nodes[1].r#ref, "f1:link:home:0");
    }

    #[test]
    fn parse_ax_tree_skips_hidden_and_non_interactive() {
        let raw = r#"{
          "nodes": [
            {"ignored": true, "role": {"value": "button"}, "name": {"value": "X"}},
            {"ignored": false, "role": {"value": "generic"}, "name": {"value": "Y"}},
            {"ignored": false, "role": {"value": "button"}, "name": {"value": "OK"},
             "properties": [{"name": "hidden", "value": {"value": true}}]}
          ]
        }"#;
        let mut counts = std::collections::HashMap::new();
        assert!(parse_ax_tree_nodes(raw, 0, 10, &mut counts).is_empty());
    }

    #[test]
    fn slug_matches_js_rules() {
        assert_eq!(slug("Go Home!"), "go-home");
        assert_eq!(slug(""), "anon");
    }

    /// Alignment guard: every AX role emitted by the CDP snapshot must be resolvable
    /// by the JS ref scanner (`zagensFindInDoc`) — either through the
    /// `zagensRoleMatches` alias table (native tags) or through a `[role=...]`
    /// entry in `ZAGENS_SEL` (attribute-only roles).
    #[test]
    fn cdp_ax_roles_resolvable_by_js_ref_scanner() {
        let js = super::super::scripts::click_js("probe:anon:0");
        // These roles only exist in the DOM as explicit role attributes.
        let attr_only = ["menuitem", "tab", "switch"];
        for role in INTERACTIVE_ROLES {
            if attr_only.contains(role) {
                assert!(
                    js.contains(&format!("[role=\"{role}\"]")),
                    "role {role} must be reachable via ZAGENS_SEL [role=...] selector"
                );
            } else {
                assert!(
                    js.contains(&format!("case '{role}'")),
                    "role {role} missing from zagensRoleMatches alias table"
                );
            }
        }
    }
}
