//! Tool-plan metadata resolution via PolicyEngine (kernel-v2 M3 G-PR).
//!
//! `legacy_tool_plan_approval_meta` is retained **only** as the kill-switch
//! path (`ToolsPolicyMode::Legacy`).  The `Shadow` variant now maps to
//! `Engine` behaviour — the M3 bake period is complete (2026-06-14).
//!
//! # Description source
//! `approval_description` is always derived from `build_approval_description`,
//! which reads from the tool registry or falls back to hard-coded strings for
//! special tools.  `PolicyEngine` controls approval/parallelism/sandbox; it
//! does not generate human-readable descriptions.

use serde_json::Value;
use zagens_core::engine::dispatch::{
    is_mcp_tool_name, mcp_tool_approval_description, mcp_tool_is_parallel_safe,
    mcp_tool_is_read_only,
};
use zagens_core::engine::tool_catalog::{CODE_EXECUTION_TOOL_NAME, is_tool_search_tool};
use zagens_core::engine::turn_loop::{ToolPlanApprovalMeta, build_edit_file_approval_desc};
use zagens_core::turn::TurnLoopMode;
use zagens_tools::{
    ApprovalRequirement, FootprintProvenance, PolicyEngine, PolicyInput, PolicyPlanMeta,
    PolicySessionMode, ToolCapability, ToolManifest,
};

use crate::config::ToolsPolicyMode;
use crate::tools::ToolRegistry;
use crate::tools::spec::ApprovalRequirement as SpecApprovalRequirement;

fn is_first_party_mcp_surface_tool(name: &str) -> bool {
    matches!(
        name,
        "list_mcp_resources"
            | "list_mcp_resource_templates"
            | "read_mcp_resource"
            | "mcp_read_resource"
            | "mcp_get_prompt"
    )
}

fn session_mode_from_turn(mode: TurnLoopMode) -> PolicySessionMode {
    match mode {
        TurnLoopMode::Agent => PolicySessionMode::Agent,
        TurnLoopMode::Plan => PolicySessionMode::Plan,
        TurnLoopMode::Yolo => PolicySessionMode::Yolo,
    }
}

// ── Description helper ────────────────────────────────────────────────────────

/// Human-readable approval description for the UI (independent of policy decisions).
///
/// Used by both the engine path and the legacy kill-switch.  The description is
/// always sourced from the registry or hard-coded strings — `PolicyEngine` does
/// not generate text.
fn build_approval_description(
    tool_name: &str,
    tool_input: &Value,
    registry: Option<&ToolRegistry>,
) -> String {
    if is_mcp_tool_name(tool_name) {
        return mcp_tool_approval_description(tool_name);
    }
    if matches!(
        tool_name,
        "browser_click"
            | "browser_type"
            | "browser_scroll"
            | "browser_start_preview"
            | "browser_navigate"
    ) {
        return browser_write_approval_description(tool_name, tool_input);
    }
    if tool_name == "exec_shell" {
        return exec_shell_approval_description(tool_input);
    }
    if let Some(registry) = registry
        && let Some(spec) = registry.get(tool_name)
    {
        return if tool_name == "edit_file" {
            build_edit_file_approval_desc(tool_input)
        } else {
            spec.description().to_string()
        };
    }
    if tool_name == CODE_EXECUTION_TOOL_NAME {
        return "Run model-provided Python code in local execution sandbox".to_string();
    }
    if is_tool_search_tool(tool_name) {
        return "Search tool catalog".to_string();
    }
    String::new()
}

fn exec_shell_approval_description(tool_input: &Value) -> String {
    let command = tool_input
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if command.is_empty() {
        return "Execute a shell command in the workspace".into();
    }
    let mut out = String::new();
    if let Some(banner) = crate::command_safety::git_push_approval_banner(command) {
        out.push_str(banner);
        out.push_str("\n\n");
    }
    out.push_str(command);
    out
}

/// Parse stable snapshot ref `role:slug:nth` into human-readable role / name bits.
fn describe_stable_browser_ref(r: &str) -> String {
    let mut parts = r.splitn(3, ':');
    let (Some(role), Some(slug), Some(nth)) = (parts.next(), parts.next(), parts.next()) else {
        return format!("ref={r}");
    };
    if role.is_empty() || slug.is_empty() || nth.parse::<u32>().is_err() {
        return format!("ref={r}");
    }
    let name = if slug == "anon" {
        "(unnamed)".to_string()
    } else {
        format!("\"{slug}\"")
    };
    format!("{role} {name} (#{nth}, ref={r})")
}

fn browser_write_approval_description(tool_name: &str, tool_input: &Value) -> String {
    let r = tool_input
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or("?");
    let target = describe_stable_browser_ref(r);
    match tool_name {
        "browser_click" => format!("Browser: click {target}"),
        "browser_type" => {
            let n = tool_input
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().count())
                .unwrap_or(0);
            format!("Browser: type into {target} ({n} chars)")
        }
        "browser_scroll" => {
            let dir = tool_input
                .get("direction")
                .and_then(|v| v.as_str())
                .unwrap_or("down");
            format!("Browser: scroll {dir} ({target})")
        }
        "browser_start_preview" => {
            "Browser: start `.zagens/preview.json` server and open URL".into()
        }
        "browser_navigate" => {
            let url = tool_input
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            if let Some(host) = external_https_host(url) {
                format!("Browser: open external site {host} (session allowlist)")
            } else {
                format!("Browser: navigate to {url}")
            }
        }
        _ => format!("Browser: {tool_name}"),
    }
}

#[derive(Clone, Default)]
struct HotBrowserPrefs {
    yolo: bool,
    allowlist: Vec<String>,
}

fn browser_prefs_path() -> Option<std::path::PathBuf> {
    zagens_config::user_data_path("browser/prefs.json").ok()
}

fn legacy_browser_prefs_path() -> Option<std::path::PathBuf> {
    dirs::data_dir().map(|d| d.join("zagens").join("browser-profile").join("prefs.json"))
}

fn load_hot_browser_prefs_from(path: &std::path::Path) -> Option<HotBrowserPrefs> {
    let raw = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&raw).ok()?;
    Some(HotBrowserPrefs {
        yolo: v.get("yolo").and_then(|x| x.as_bool()).unwrap_or(false),
        allowlist: v
            .get("allowlist")
            .and_then(|x| x.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|h| h.as_str().map(|s| s.to_ascii_lowercase()))
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn load_hot_browser_prefs() -> HotBrowserPrefs {
    if let Some(path) = browser_prefs_path()
        && let Some(prefs) = load_hot_browser_prefs_from(&path)
    {
        return prefs;
    }
    // Read-only fallback while desktop migrates AppData → ~/.zagens/browser/.
    if let Some(legacy) = legacy_browser_prefs_path()
        && let Some(prefs) = load_hot_browser_prefs_from(&legacy)
    {
        return prefs;
    }
    HotBrowserPrefs::default()
}

fn hot_browser_prefs_cached() -> HotBrowserPrefs {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CACHE: Mutex<Option<(Instant, HotBrowserPrefs)>> = Mutex::new(None);
    let now = Instant::now();
    if let Ok(guard) = CACHE.lock()
        && let Some((ts, prefs)) = guard.as_ref()
        && now.duration_since(*ts) < Duration::from_millis(750)
    {
        return prefs.clone();
    }
    let prefs = load_hot_browser_prefs();
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some((now, prefs.clone()));
    }
    prefs
}

fn browser_yolo_enabled() -> bool {
    if matches!(
        std::env::var("ZAGENS_BROWSER_YOLO")
            .ok()
            .as_deref()
            .map(str::trim),
        Some("1") | Some("true") | Some("TRUE") | Some("yes")
    ) {
        return true;
    }
    // Hot path: desktop `browser_set_prefs` writes prefs.json (no sidecar restart).
    hot_browser_prefs_cached().yolo
}

/// Rough external-https host extractor (mirrors desktop url_policy agent ask).
fn external_https_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("https://") {
        return None;
    }
    let rest = trimmed.get(8..)?;
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = hostport
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(':')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.is_empty()
        || matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1")
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || host.starts_with("172.")
    {
        return None;
    }
    Some(host)
}

fn browser_navigate_needs_external_ask(tool_input: &Value) -> Option<String> {
    let url = tool_input.get("url").and_then(|v| v.as_str())?;
    let host = external_https_host(url)?;
    let prefs = hot_browser_prefs_cached();
    // browser_yolo auto-allows external nav (same as writes). Already-allowlisted hosts skip ask.
    if prefs.yolo {
        return None;
    }
    if prefs
        .allowlist
        .iter()
        .any(|h| h.eq_ignore_ascii_case(&host))
    {
        return None;
    }
    Some(host)
}

fn is_browser_write_tool(name: &str) -> bool {
    matches!(
        name,
        "browser_click" | "browser_type" | "browser_scroll" | "browser_start_preview"
    )
}

// ── Legacy kill-switch path ───────────────────────────────────────────────────

/// Full legacy heuristic resolution (kill-switch: `[tools] policy = "legacy"`).
///
/// Retained so that users with `policy = "legacy"` in `config.toml` can
/// downgrade to pre-M3 approval/parallelism behaviour without reinstalling.
/// Not called in normal operation — `PolicyEngine` is the default.
fn legacy_tool_plan_approval_meta(
    tool_name: &str,
    tool_input: &Value,
    registry: Option<&ToolRegistry>,
) -> ToolPlanApprovalMeta {
    if is_mcp_tool_name(tool_name) {
        return ToolPlanApprovalMeta {
            read_only: mcp_tool_is_read_only(tool_name),
            supports_parallel: mcp_tool_is_parallel_safe(tool_name),
            approval_required: !mcp_tool_is_read_only(tool_name),
            approval_description: mcp_tool_approval_description(tool_name),
        };
    }
    if let Some(registry) = registry
        && let Some(spec) = registry.get(tool_name)
    {
        let mut meta = ToolPlanApprovalMeta {
            approval_required: spec.approval_requirement() != SpecApprovalRequirement::Auto,
            approval_description: if tool_name == "edit_file" {
                build_edit_file_approval_desc(tool_input)
            } else if is_browser_write_tool(tool_name) {
                browser_write_approval_description(tool_name, tool_input)
            } else {
                spec.description().to_string()
            },
            supports_parallel: spec.supports_parallel(),
            read_only: spec.is_read_only(),
        };
        if is_browser_write_tool(tool_name) && !browser_yolo_enabled() {
            meta.approval_required = true;
        }
        if tool_name == "browser_navigate"
            && let Some(host) = browser_navigate_needs_external_ask(tool_input)
        {
            meta.approval_required = true;
            meta.approval_description =
                format!("Browser: open external site {host} (will allow for this session)");
            meta.read_only = false;
        }
        return meta;
    }
    if tool_name == CODE_EXECUTION_TOOL_NAME {
        return ToolPlanApprovalMeta {
            approval_required: true,
            approval_description: "Run model-provided Python code in local execution sandbox"
                .to_string(),
            supports_parallel: false,
            read_only: false,
        };
    }
    if is_tool_search_tool(tool_name) {
        return ToolPlanApprovalMeta {
            approval_required: false,
            approval_description: "Search tool catalog".to_string(),
            supports_parallel: false,
            read_only: true,
        };
    }
    ToolPlanApprovalMeta {
        approval_required: false,
        approval_description: String::new(),
        supports_parallel: false,
        read_only: false,
    }
}

// ── PolicyEngine path ─────────────────────────────────────────────────────────

fn build_policy_input(
    tool_name: &str,
    registry: Option<&ToolRegistry>,
    session_mode: PolicySessionMode,
    trust_mode: bool,
) -> PolicyInput {
    if is_mcp_tool_name(tool_name) {
        let (manifest, legacy_approval, supports_parallel_hint) =
            if is_first_party_mcp_surface_tool(tool_name) {
                let manifest = ToolManifest::derive_conservative(
                    tool_name,
                    &[ToolCapability::ReadOnly],
                    false,
                    FootprintProvenance::BuiltIn,
                );
                (
                    manifest,
                    ApprovalRequirement::Auto,
                    mcp_tool_is_parallel_safe(tool_name),
                )
            } else {
                let manifest = registry
                    .and_then(|r| r.get(tool_name))
                    .map(|spec| spec.manifest())
                    .unwrap_or_else(|| {
                        ToolManifest::derive_conservative(
                            tool_name,
                            &[ToolCapability::Network, ToolCapability::RequiresApproval],
                            false,
                            FootprintProvenance::McpSelfDeclared,
                        )
                    });
                (manifest, ApprovalRequirement::Required, false)
            };
        return PolicyInput {
            session_mode,
            manifest,
            legacy_approval,
            supports_parallel_hint,
            trust_mode,
        };
    }

    if let Some(registry) = registry
        && let Some(spec) = registry.get(tool_name)
    {
        return PolicyInput {
            session_mode,
            manifest: spec.manifest(),
            legacy_approval: map_spec_approval(spec.approval_requirement()),
            supports_parallel_hint: spec.supports_parallel(),
            trust_mode,
        };
    }

    if tool_name == CODE_EXECUTION_TOOL_NAME {
        return PolicyInput {
            session_mode,
            manifest: ToolManifest::derive_conservative(
                tool_name,
                &[ToolCapability::ExecutesCode],
                true,
                FootprintProvenance::BuiltIn,
            ),
            legacy_approval: ApprovalRequirement::Required,
            supports_parallel_hint: false,
            trust_mode,
        };
    }

    if is_tool_search_tool(tool_name) {
        return PolicyInput {
            session_mode,
            manifest: ToolManifest::derive_conservative(
                tool_name,
                &[ToolCapability::ReadOnly],
                false,
                FootprintProvenance::BuiltIn,
            ),
            legacy_approval: ApprovalRequirement::Auto,
            supports_parallel_hint: false,
            trust_mode,
        };
    }

    PolicyInput {
        session_mode,
        manifest: ToolManifest::derive_conservative(
            tool_name,
            &[],
            false,
            FootprintProvenance::BuiltIn,
        ),
        legacy_approval: ApprovalRequirement::Auto,
        supports_parallel_hint: false,
        trust_mode,
    }
}

fn map_spec_approval(level: SpecApprovalRequirement) -> ApprovalRequirement {
    match level {
        SpecApprovalRequirement::Auto => ApprovalRequirement::Auto,
        SpecApprovalRequirement::Suggest => ApprovalRequirement::Suggest,
        SpecApprovalRequirement::Required => ApprovalRequirement::Required,
    }
}

fn engine_plan_meta(
    tool_name: &str,
    registry: Option<&ToolRegistry>,
    session_mode: PolicySessionMode,
    trust_mode: bool,
) -> PolicyPlanMeta {
    let input = build_policy_input(tool_name, registry, session_mode, trust_mode);
    PolicyEngine::decide(&input).plan_meta()
}

fn apply_engine_meta(description: String, engine: PolicyPlanMeta) -> ToolPlanApprovalMeta {
    ToolPlanApprovalMeta {
        approval_required: engine.approval_required,
        read_only: engine.read_only,
        supports_parallel: engine.supports_parallel,
        approval_description: description,
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Resolve tool-plan metadata for one planned tool call.
///
/// - `Legacy`  → full pre-M3 heuristic path (kill-switch only).
/// - `Shadow`  → `Engine` (bake complete; shadow comparison removed).
/// - `Engine`  → `PolicyEngine` controls approval/parallelism/sandbox;
///   description sourced from `build_approval_description`.
#[must_use]
pub fn resolve_tool_plan_approval_meta(
    policy_mode: ToolsPolicyMode,
    turn_mode: TurnLoopMode,
    trust_mode: bool,
    tool_name: &str,
    tool_input: &Value,
    registry: Option<&ToolRegistry>,
) -> ToolPlanApprovalMeta {
    // Kill-switch: restore legacy heuristics.
    if policy_mode == ToolsPolicyMode::Legacy {
        return legacy_tool_plan_approval_meta(tool_name, tool_input, registry);
    }

    // Engine (and Shadow, which is now an alias for Engine post-bake).
    let session_mode = session_mode_from_turn(turn_mode);
    let engine = engine_plan_meta(tool_name, registry, session_mode, trust_mode);
    let description = build_approval_description(tool_name, tool_input, registry);
    let mut meta = apply_engine_meta(description, engine);
    // Global YOLO / trust_mode must NOT auto-approve browser writes unless browser_yolo.
    if is_browser_write_tool(tool_name) && !browser_yolo_enabled() {
        meta.approval_required = true;
    }
    // C1: external https navigate asks once, then session allowlist (unless already allowed / yolo).
    if tool_name == "browser_navigate"
        && let Some(host) = browser_navigate_needs_external_ask(tool_input)
    {
        meta.approval_required = true;
        meta.approval_description =
            format!("Browser: open external site {host} (will allow for this session)");
        meta.read_only = false;
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_mode_denies_mcp_self_declared_parallel() {
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "mcp_server_evil",
            &serde_json::json!({}),
            None,
        );
        assert!(meta.approval_required);
        assert!(!meta.supports_parallel);
        assert!(!meta.read_only);
    }

    #[test]
    fn first_party_mcp_discovery_is_auto_in_engine_mode() {
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "list_mcp_resources",
            &serde_json::json!({}),
            None,
        );
        assert!(!meta.approval_required);
        assert!(meta.read_only);
    }

    #[test]
    fn browser_navigate_external_requires_ask() {
        // Use a host that will not appear in the developer's persisted browser allowlist.
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "browser_navigate",
            &serde_json::json!({ "url": "https://ask-once.example.invalid/docs" }),
            None,
        );
        assert!(meta.approval_required);
        assert!(
            meta.approval_description
                .contains("ask-once.example.invalid")
        );
    }

    #[test]
    fn browser_navigate_loopback_stays_auto() {
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "browser_navigate",
            &serde_json::json!({ "url": "http://127.0.0.1:5173/" }),
            None,
        );
        // Engine may mark network tools variously; we only assert we did NOT force external ask.
        assert!(!meta.approval_description.contains("session allowlist"));
    }

    #[test]
    fn shadow_mode_is_alias_for_engine_post_bake() {
        // Shadow mode no longer compares legacy; it returns engine result.
        let shadow = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Shadow,
            TurnLoopMode::Agent,
            false,
            "list_mcp_resources",
            &serde_json::json!({}),
            None,
        );
        let engine = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Engine,
            TurnLoopMode::Agent,
            false,
            "list_mcp_resources",
            &serde_json::json!({}),
            None,
        );
        assert_eq!(shadow.approval_required, engine.approval_required);
        assert_eq!(shadow.read_only, engine.read_only);
        assert_eq!(shadow.supports_parallel, engine.supports_parallel);
    }

    #[test]
    fn legacy_kill_switch_returns_heuristic_result() {
        // Unknown tool via legacy path: heuristic defaults (conservative = not read_only,
        // no approval for unknown).  This is the kill-switch contract.
        let meta = resolve_tool_plan_approval_meta(
            ToolsPolicyMode::Legacy,
            TurnLoopMode::Agent,
            false,
            "unknown_tool_xyz",
            &serde_json::json!({}),
            None,
        );
        assert!(!meta.approval_required);
        assert!(!meta.read_only);
        assert!(!meta.supports_parallel);
    }

    #[test]
    fn build_approval_description_uses_mcp_helper() {
        // Smoke test: description helper does not panic for MCP names.
        let desc = build_approval_description("mcp_some_server", &serde_json::json!({}), None);
        let _ = desc; // content is mcp_tool_approval_description(name)
    }

    #[test]
    fn exec_shell_force_push_banner_in_description() {
        let desc = exec_shell_approval_description(&serde_json::json!({
            "command": "git push --force origin main"
        }));
        assert!(desc.contains("FORCE PUSH"), "{desc}");
        assert!(desc.contains("git push --force origin main"), "{desc}");
        let safe = exec_shell_approval_description(&serde_json::json!({
            "command": "git push --follow-tags origin main"
        }));
        assert!(!safe.contains("FORCE PUSH"), "{safe}");
    }

    #[test]
    fn browser_click_approval_describes_role_and_name() {
        let desc = browser_write_approval_description(
            "browser_click",
            &serde_json::json!({ "ref": "button:submit:0" }),
        );
        assert!(desc.contains("button"));
        assert!(desc.contains("\"submit\""));
        assert!(desc.contains("#0"));
        assert!(desc.contains("ref=button:submit:0"));
    }

    #[test]
    fn describe_stable_browser_ref_handles_anon_and_legacy() {
        assert!(describe_stable_browser_ref("link:anon:2").contains("(unnamed)"));
        assert_eq!(describe_stable_browser_ref("e12"), "ref=e12");
    }
}
