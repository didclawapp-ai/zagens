//! Large-output routing for tool results (issue #548).
//!
//! Any tool result whose estimated token count exceeds the configured threshold
//! is intercepted here before it reaches the parent context. A lightweight
//! V4-Flash synthesis sub-agent condenses the raw output; only the synthesis
//! is returned to the parent. The raw content is stored in the workshop
//! variable `last_tool_result` so the parent agent can call
//! `promote_to_context` later if it needs the full text.
//!
//! Per-tool thresholds can override the global default. Individual tool calls
//! may pass `raw=true` to bypass routing entirely.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::tools::spec::ToolResult;

// ── Constants ──────────────────────────────────────────────────────────────────

// Re-exported from deepseek-core (P2 PR4g).
pub use deepseek_core::workshop::{WorkshopConfig, DEFAULT_LARGE_OUTPUT_THRESHOLD_TOKENS};

/// Approximate characters-per-token ratio used for the heuristic estimate.
/// We intentionally choose a conservative value (3 chars/token) so we err
/// on the side of routing rather than dumping raw data into the parent.
const CHARS_PER_TOKEN_ESTIMATE: usize = 3;

/// Workshop variable name where the raw tool output is stored.
pub const WORKSHOP_LAST_TOOL_RESULT_VAR: &str = "last_tool_result";

/// Env override for tests: root directory instead of `~/.deepseek/sessions/…`.
const LARGE_OUTPUT_ROOT_ENV: &str = "DEEPSEEK_LARGE_OUTPUT_ROOT";

const LARGE_OUTPUT_PERSIST_SCHEMA_VERSION: u32 = 1;

/// Stable external reference for routed large tool output (A1-MVP.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeOutputExternalRef {
    pub ref_id: String,
    pub tool_name: String,
    pub char_count: usize,
    pub storage_var: String,
}

impl LargeOutputExternalRef {
    #[must_use]
    pub fn new(tool_name: &str, char_count: usize) -> Self {
        Self {
            ref_id: format!("lout_{}", &Uuid::new_v4().to_string()[..8]),
            tool_name: tool_name.to_string(),
            char_count,
            storage_var: WORKSHOP_LAST_TOOL_RESULT_VAR.to_string(),
        }
    }

    /// Single-line JSON for message / JSONL embedding.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// On-disk metadata for a routed large tool output (A1.2 session isomorphism).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LargeOutputPersistRecord {
    pub schema_version: u32,
    pub external_ref: LargeOutputExternalRef,
    pub session_id: String,
    pub raw_bytes: usize,
}

/// Whether `state_namespace` is a durable session id (not the generic workspace scope).
#[must_use]
pub fn should_persist_large_output_for_namespace(namespace: &str) -> bool {
    let ns = namespace.trim();
    !ns.is_empty() && ns != "workspace"
}

/// Directory for large-output blobs for a session (`<sessions>/<session_id>/large_outputs/`).
///
/// Default sessions root: `~/.deepseek/sessions`. Tests may set `DEEPSEEK_LARGE_OUTPUT_ROOT`
/// to a temp directory (used as the sessions root, not the home dir).
#[must_use]
pub fn large_output_dir(session_id: &str) -> PathBuf {
    let sessions_base = std::env::var_os(LARGE_OUTPUT_ROOT_ENV)
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".deepseek").join("sessions")))
        .unwrap_or_else(|| PathBuf::from(".deepseek").join("sessions"));
    sessions_base.join(session_id).join("large_outputs")
}

/// Write raw tool output + JSON metadata; returns the metadata path.
pub fn persist_large_output_blob(
    session_id: &str,
    external_ref: &LargeOutputExternalRef,
    raw: &str,
) -> std::io::Result<PathBuf> {
    let dir = large_output_dir(session_id);
    std::fs::create_dir_all(&dir)?;
    let raw_path = dir.join(format!("{}.txt", external_ref.ref_id));
    std::fs::write(&raw_path, raw)?;
    let record = LargeOutputPersistRecord {
        schema_version: LARGE_OUTPUT_PERSIST_SCHEMA_VERSION,
        external_ref: external_ref.clone(),
        session_id: session_id.to_string(),
        raw_bytes: raw.len(),
    };
    let meta_path = dir.join(format!("{}.json", external_ref.ref_id));
    std::fs::write(&meta_path, serde_json::to_string(&record).unwrap())?;
    Ok(meta_path)
}

/// Load persisted metadata for a workshop ref id.
pub fn load_large_output_persist_record(
    session_id: &str,
    ref_id: &str,
) -> std::io::Result<LargeOutputPersistRecord> {
    let path = large_output_dir(session_id).join(format!("{ref_id}.json"));
    let raw = std::fs::read_to_string(path)?;
    serde_json::from_str(&raw).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Parse `[workshop-ref: {json}]` from a tool-result message body.
#[must_use]
pub fn parse_workshop_ref_from_message(message: &str) -> Option<LargeOutputExternalRef> {
    message.lines().find_map(|line| {
        let line = line.trim();
        let json = line.strip_prefix("[workshop-ref: ")?.trim_end_matches(']');
        serde_json::from_str(json).ok()
    })
}

// ── Token estimation ──────────────────────────────────────────────────────────

/// Estimate the number of tokens in `text` using a character-count heuristic.
///
/// This avoids a real tokeniser dependency; the estimate is deliberately
/// conservative (under-counts tokens) so we route aggressively rather than
/// letting a 5K-token blob slip through.
#[must_use]
pub fn estimate_tokens(text: &str) -> usize {
    let chars = text.chars().count();
    // Round up: partial last token still costs a token.
    chars.div_ceil(CHARS_PER_TOKEN_ESTIMATE)
}

// ── Router ────────────────────────────────────────────────────────────────────

/// Decision returned by [`LargeOutputRouter::route`].
#[derive(Debug, Clone, PartialEq)]
pub enum RouteDecision {
    /// The output is small enough; pass it through unmodified.
    PassThrough,
    /// The output exceeded the threshold and was (or should be) synthesised.
    Synthesise {
        /// Estimated token count of the raw output.
        estimated_tokens: usize,
        /// The threshold that was breached.
        threshold: usize,
    },
}

/// Intercepts tool results and routes large ones through the workshop.
///
/// This type is intentionally `Clone` and `Default` so it can be embedded
/// cheaply in [`ToolContext`](crate::tools::spec::ToolContext) without
/// requiring `Arc` wrappers.
#[derive(Debug, Clone, Default)]
pub struct LargeOutputRouter {
    config: WorkshopConfig,
}

impl LargeOutputRouter {
    /// Construct a router from the resolved workshop config.
    #[must_use]
    pub fn new(config: WorkshopConfig) -> Self {
        Self { config }
    }

    /// Decide whether `result` for `tool_name` should be synthesised.
    ///
    /// Pass `raw_bypass = true` when the tool call included `raw = true`.
    #[must_use]
    pub fn route(&self, tool_name: &str, result: &ToolResult, raw_bypass: bool) -> RouteDecision {
        if raw_bypass || !result.success {
            return RouteDecision::PassThrough;
        }
        let threshold = self.config.threshold_for(tool_name);
        let estimated_tokens = estimate_tokens(&result.content);
        if estimated_tokens > threshold {
            RouteDecision::Synthesise {
                estimated_tokens,
                threshold,
            }
        } else {
            RouteDecision::PassThrough
        }
    }

    /// Build the synthesis prompt sent to the V4-Flash workshop sub-agent.
    ///
    /// The prompt is intentionally terse — Flash is a fast model and we just
    /// want a faithful summary, not deep reasoning.
    ///
    /// This is the building block for the live LLM synthesis call wired in
    /// the follow-up (once the async Flash client is safe to call from the
    /// registry layer). The method is public so callers outside this crate
    /// can unit-test the prompt shape.
    #[must_use]
    #[allow(dead_code)] // used by future Flash synthesis call; keep for API stability
    pub fn synthesis_prompt(tool_name: &str, raw_output: &str, estimated_tokens: usize) -> String {
        format!(
            "You are a synthesis assistant. The tool `{tool_name}` produced {estimated_tokens} tokens \
             of output that is too large to include directly in the parent context.\n\n\
             Summarise the output below into a concise, faithful synthesis of ≤ 800 words. \
             Preserve key facts, numbers, file paths, error messages, and any actionable \
             information. Do NOT add commentary or interpretation beyond what is in the source.\n\n\
             <raw_tool_output>\n{raw_output}\n</raw_tool_output>"
        )
    }

    /// Wrap a synthesis result with a workshop provenance header and a hint
    /// about the stored raw output.
    #[must_use]
    pub fn wrap_synthesis(
        tool_name: &str,
        synthesis: &str,
        estimated_tokens: usize,
        threshold: usize,
        external_ref: Option<&LargeOutputExternalRef>,
    ) -> String {
        let ref_line = external_ref
            .map(|r| format!("[workshop-ref: {}]\n", r.to_json_line()))
            .unwrap_or_default();
        format!(
            "{ref_line}[workshop-synthesis: tool={tool_name}, raw_tokens≈{estimated_tokens}, \
             threshold={threshold}, raw_stored_in={WORKSHOP_LAST_TOOL_RESULT_VAR}]\n\n{synthesis}"
        )
    }
}

// ── Workshop variable store ───────────────────────────────────────────────────

/// In-process store for workshop variables that persist across tool calls
/// within a session. The only variable exposed today is `last_tool_result`
/// which holds the most recent raw large-tool output for `promote_to_context`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkshopVariables {
    /// Raw content of the most recent large tool output that was routed
    /// through the workshop. Empty string when no routing has occurred.
    #[serde(default)]
    pub last_tool_result: String,

    /// Name of the tool that produced `last_tool_result`.
    #[serde(default)]
    pub last_tool_name: String,

    /// Structured ref for the most recent large output (session/JSONL embedding).
    #[serde(default)]
    pub last_output_ref: Option<LargeOutputExternalRef>,
}

impl WorkshopVariables {
    /// Store the raw output from a large-tool routing event.
    pub fn store_raw(&mut self, tool_name: &str, raw: &str) -> LargeOutputExternalRef {
        let external_ref = LargeOutputExternalRef::new(tool_name, raw.chars().count());
        self.last_tool_result = raw.to_string();
        self.last_tool_name = tool_name.to_string();
        self.last_output_ref = Some(external_ref.clone());
        external_ref
    }

    /// Retrieve and clear the stored raw output (consume semantics so the
    /// variable is not accidentally promoted twice).
    ///
    /// Called by the `promote_to_context` tool (not yet wired in this PR).
    #[must_use]
    #[allow(dead_code)] // consumed by promote_to_context tool in follow-up
    pub fn take_raw(&mut self) -> Option<(String, String)> {
        if self.last_tool_result.is_empty() {
            return None;
        }
        let content = std::mem::take(&mut self.last_tool_result);
        let name = std::mem::take(&mut self.last_tool_name);
        self.last_output_ref = None;
        Some((name, content))
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(content: &str) -> ToolResult {
        ToolResult::success(content.to_string())
    }

    #[test]
    fn pass_through_below_threshold() {
        let router = LargeOutputRouter::default();
        let small = "x".repeat(100);
        let result = make_result(&small);
        assert_eq!(
            router.route("read_file", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn synthesise_above_threshold() {
        let router = LargeOutputRouter::default();
        // DEFAULT threshold = 4096 tokens; 3 chars/token → 4096*3 = 12288 chars
        let big = "a".repeat(13_000);
        let result = make_result(&big);
        assert!(matches!(
            router.route("read_file", &result, false),
            RouteDecision::Synthesise { .. }
        ));
    }

    #[test]
    fn raw_bypass_skips_routing() {
        let router = LargeOutputRouter::default();
        let big = "a".repeat(13_000);
        let result = make_result(&big);
        // raw=true → always pass through regardless of size
        assert_eq!(
            router.route("exec_shell", &result, true),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn error_results_always_pass_through() {
        let router = LargeOutputRouter::default();
        let big = "error: ".repeat(2_000);
        let result = ToolResult::error(big);
        assert_eq!(
            router.route("exec_shell", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn per_tool_threshold_override() {
        let mut per_tool = HashMap::new();
        per_tool.insert("grep_files".to_string(), 100); // very low
        let config = WorkshopConfig {
            large_output_threshold_tokens: Some(4096),
            per_tool_thresholds: Some(per_tool),
        };
        let router = LargeOutputRouter::new(config);
        // 100 tokens * 3 = 300 chars → trigger with 400 chars
        let medium = "b".repeat(400);
        let result = make_result(&medium);
        assert!(matches!(
            router.route("grep_files", &result, false),
            RouteDecision::Synthesise { .. }
        ));
        // Other tools still use the global threshold
        assert_eq!(
            router.route("read_file", &result, false),
            RouteDecision::PassThrough
        );
    }

    #[test]
    fn synthesise_at_one_megabyte_boundary() {
        let router = LargeOutputRouter::default();
        // R-015: exercise >=1 MB tool output path (~1.1 MB of ASCII).
        let one_mb_plus = "z".repeat(1_100_000);
        let result = make_result(&one_mb_plus);
        match router.route("read_file", &result, false) {
            RouteDecision::Synthesise {
                estimated_tokens,
                threshold,
            } => {
                assert!(estimated_tokens > threshold);
                assert!(estimated_tokens >= 350_000, "1.1MB should estimate well above threshold");
            }
            RouteDecision::PassThrough => panic!("1.1 MB output must route to synthesis"),
        }
    }

    #[test]
    fn estimate_tokens_conservative() {
        // 9 chars → ceil(9/3) = 3 tokens
        assert_eq!(estimate_tokens("123456789"), 3);
        // 10 chars → ceil(10/3) = 4 tokens
        assert_eq!(estimate_tokens("1234567890"), 4);
        // Empty string
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn workshop_variables_store_and_take() {
        let mut vars = WorkshopVariables::default();
        assert!(vars.take_raw().is_none());

        vars.store_raw("read_file", "raw content here");
        let taken = vars.take_raw().expect("should have content");
        assert_eq!(taken.0, "read_file");
        assert_eq!(taken.1, "raw content here");
        assert!(vars.last_output_ref.is_none());

        // Second take is empty — consume semantics
        assert!(vars.take_raw().is_none());
    }

    #[test]
    fn store_raw_records_external_ref() {
        let mut vars = WorkshopVariables::default();
        let big = "y".repeat(10_000);
        let external_ref = vars.store_raw("grep_files", &big);
        assert!(external_ref.ref_id.starts_with("lout_"));
        assert_eq!(external_ref.tool_name, "grep_files");
        assert_eq!(external_ref.char_count, 10_000);
        assert_eq!(
            vars.last_output_ref.as_ref().map(|r| r.ref_id.as_str()),
            Some(external_ref.ref_id.as_str())
        );
    }

    #[test]
    fn wrap_synthesis_includes_provenance_header() {
        let external_ref = LargeOutputExternalRef::new("web_search", 5000);
        let wrapped = LargeOutputRouter::wrap_synthesis(
            "web_search",
            "key facts here",
            5000,
            4096,
            Some(&external_ref),
        );
        assert!(wrapped.contains("workshop-synthesis"));
        assert!(wrapped.contains("workshop-ref:"));
        assert!(wrapped.contains("web_search"));
        assert!(wrapped.contains("5000"));
        assert!(wrapped.contains("key facts here"));
        assert!(wrapped.contains(&external_ref.ref_id));
    }

    #[test]
    fn should_persist_skips_workspace_namespace() {
        assert!(!should_persist_large_output_for_namespace("workspace"));
        assert!(!should_persist_large_output_for_namespace(""));
        assert!(should_persist_large_output_for_namespace("sess_abc"));
    }

    #[test]
    fn large_output_persist_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // SAFETY: single-threaded test; env cleared before return.
        unsafe { std::env::set_var(LARGE_OUTPUT_ROOT_ENV, tmp.path()) };

        let session_id = "sess_test_roundtrip";
        let raw = "payload-".repeat(800);
        let external_ref = LargeOutputExternalRef::new("read_file", raw.chars().count());
        persist_large_output_blob(session_id, &external_ref, &raw).expect("persist");

        let wrapped = LargeOutputRouter::wrap_synthesis(
            "read_file",
            "summary",
            5000,
            4096,
            Some(&external_ref),
        );
        let parsed =
            parse_workshop_ref_from_message(&wrapped).expect("workshop-ref line in synthesis");
        assert_eq!(parsed.ref_id, external_ref.ref_id);

        let record =
            load_large_output_persist_record(session_id, &parsed.ref_id).expect("load meta");
        assert_eq!(record.schema_version, LARGE_OUTPUT_PERSIST_SCHEMA_VERSION);
        assert_eq!(record.session_id, session_id);
        assert_eq!(record.raw_bytes, raw.len());

        let raw_path = large_output_dir(session_id).join(format!("{}.txt", parsed.ref_id));
        assert_eq!(std::fs::read_to_string(raw_path).expect("raw blob"), raw);

        unsafe { std::env::remove_var(LARGE_OUTPUT_ROOT_ENV) };
    }
}
