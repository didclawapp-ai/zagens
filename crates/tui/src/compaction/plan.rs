use regex::Regex;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::logging;
use crate::models::{ContentBlock, Message};

use super::{MAX_WORKING_SET_PATHS, RECENT_WORKING_SET_WINDOW};

#[derive(Debug, Clone, Default)]
pub struct CompactionPlan {
    pub pinned_indices: BTreeSet<usize>,
    pub summarize_indices: Vec<usize>,
}

impl CompactionPlan {
    /// Hot-window / cold-zone view for this plan (A1.1).
    #[must_use]
    pub fn context_partition(
        &self,
        messages: &[Message],
        keep_recent: usize,
    ) -> deepseek_core::context_partition::SessionContextPartition {
        deepseek_core::context_partition::classify_session_messages(
            messages,
            keep_recent,
            &self.pinned_indices,
        )
    }
}

pub(crate) fn path_regex() -> &'static Regex {
    static PATH_RE: OnceLock<Regex> = OnceLock::new();
    PATH_RE.get_or_init(|| {
        Regex::new(
            r"(?x)
            (?:
                (?P<root>
                    Cargo\.toml|
                    Cargo\.lock|
                    README\.md|
                    CHANGELOG\.md|
                    AGENTS\.md|
                    config\.example\.toml
                )
            )
            |
            (?P<path>
                (?:[A-Za-z0-9._-]+/)+
                [A-Za-z0-9._-]+
                \.(?:rs|toml|md|json|ya?ml|txt|lock)
            )
        ",
        )
        .expect("path regex is valid")
    })
}

pub(crate) fn normalize_path_candidate(candidate: &str, workspace: Option<&Path>) -> Option<String> {
    if candidate.is_empty() {
        return None;
    }

    let cleaned = candidate.replace('\\', "/");
    let mut path = PathBuf::from(cleaned);

    if path.is_absolute() {
        let ws = workspace?;
        if let Ok(stripped) = path.strip_prefix(ws) {
            path = stripped.to_path_buf();
        } else {
            return None;
        }
    }

    let rel = path.to_string_lossy().trim_start_matches("./").to_string();
    if rel.is_empty() || rel.contains("..") {
        return None;
    }

    if let Some(ws) = workspace {
        let repo_path = ws.join(&rel);
        if repo_path.exists() || looks_repo_relative(&rel) {
            return Some(rel);
        }
        return None;
    }

    if looks_repo_relative(&rel) {
        return Some(rel);
    }

    None
}

pub(crate) fn looks_repo_relative(path: &str) -> bool {
    matches!(
        path,
        "Cargo.toml"
            | "Cargo.lock"
            | "README.md"
            | "CHANGELOG.md"
            | "AGENTS.md"
            | "config.example.toml"
    ) || path.starts_with("src/")
        || path.starts_with("tests/")
        || path.starts_with("docs/")
        || path.starts_with("examples/")
        || path.starts_with("benches/")
        || path.starts_with("crates/")
        || path.starts_with(".github/")
        || (path.contains('/') && path.rsplit('.').next().is_some())
}

pub(crate) fn extract_paths_from_text(text: &str, workspace: Option<&Path>) -> Vec<String> {
    path_regex()
        .captures_iter(text)
        .filter_map(|caps| {
            let candidate = caps
                .name("path")
                .or_else(|| caps.name("root"))
                .map(|m| m.as_str())?;
            normalize_path_candidate(candidate, workspace)
        })
        .collect()
}

pub(crate) fn extract_paths_from_tool_input(
    input: &serde_json::Value,
    workspace: Option<&Path>,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(obj) = input.as_object() else {
        return out;
    };

    for key in ["path", "file", "target", "cwd"] {
        if let Some(val) = obj.get(key).and_then(serde_json::Value::as_str)
            && let Some(path) = normalize_path_candidate(val, workspace)
        {
            out.push(path);
        }
    }

    for key in ["paths", "files", "targets"] {
        if let Some(vals) = obj.get(key).and_then(serde_json::Value::as_array) {
            for val in vals {
                if let Some(s) = val.as_str()
                    && let Some(path) = normalize_path_candidate(s, workspace)
                {
                    out.push(path);
                }
            }
        }
    }

    out
}

pub(crate) fn message_text(msg: &Message) -> String {
    let mut text = String::new();
    for block in &msg.content {
        match block {
            ContentBlock::Text { text: t, .. } => {
                let _ = writeln!(text, "{t}");
            }
            ContentBlock::Thinking { .. } => {}
            ContentBlock::ToolUse { name, input, .. } => {
                let _ = writeln!(text, "[tool_use:{name}] {input}");
            }
            ContentBlock::ToolResult { content, .. } => {
                let _ = writeln!(text, "{content}");
            }
            ContentBlock::ServerToolUse { .. }
            | ContentBlock::ToolSearchToolResult { .. }
            | ContentBlock::CodeExecutionToolResult { .. } => {}
        }
    }
    text
}

pub(crate) fn extract_paths_from_message(message: &Message, workspace: Option<&Path>) -> Vec<String> {
    let mut paths = Vec::new();
    for block in &message.content {
        let candidates = match block {
            ContentBlock::Text { text, .. } => extract_paths_from_text(text, workspace),
            ContentBlock::ToolResult { content, .. } => extract_paths_from_text(content, workspace),
            ContentBlock::ToolUse { input, .. } => extract_paths_from_tool_input(input, workspace),
            ContentBlock::Thinking { .. } => Vec::new(),
            ContentBlock::ServerToolUse { .. }
            | ContentBlock::ToolSearchToolResult { .. }
            | ContentBlock::CodeExecutionToolResult { .. } => Vec::new(),
        };
        paths.extend(candidates);
    }
    paths
}

pub(crate) fn derive_working_set_paths(
    messages: &[Message],
    workspace: Option<&Path>,
    seed_indices: &[usize],
) -> HashSet<String> {
    let mut paths: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let mut seeds: Vec<usize> = seed_indices
        .iter()
        .copied()
        .filter(|idx| *idx < messages.len())
        .collect();
    seeds.sort_unstable_by(|a, b| b.cmp(a));

    for idx in seeds {
        for candidate in extract_paths_from_message(&messages[idx], workspace) {
            if seen.insert(candidate.clone()) {
                paths.push(candidate);
                if paths.len() >= MAX_WORKING_SET_PATHS {
                    return paths.into_iter().collect();
                }
            }
        }
    }

    for msg in messages.iter().rev().take(RECENT_WORKING_SET_WINDOW) {
        for candidate in extract_paths_from_message(msg, workspace) {
            if seen.insert(candidate.clone()) {
                paths.push(candidate);
                if paths.len() >= MAX_WORKING_SET_PATHS {
                    return paths.into_iter().collect();
                }
            }
        }
    }

    paths.into_iter().collect()
}

pub(crate) fn should_pin_message(text: &str, working_set_paths: &HashSet<String>) -> bool {
    let lower = text.to_lowercase();

    let mentions_working_set = working_set_paths.iter().any(|p| text.contains(p));
    if mentions_working_set {
        return true;
    }

    let error_markers = [
        "error:",
        "error ",
        "failed",
        "panic",
        "traceback",
        "stack trace",
        "assertion failed",
        "test failed",
    ];
    if error_markers.iter().any(|m| lower.contains(m)) {
        return true;
    }

    let patch_markers = [
        "diff --git",
        "+++ b/",
        "--- a/",
        "*** begin patch",
        "*** update file:",
        "*** add file:",
        "*** delete file:",
        "```diff",
        "apply_patch",
    ];
    patch_markers.iter().any(|m| lower.contains(m))
}

pub fn plan_compaction(
    messages: &[Message],
    workspace: Option<&Path>,
    keep_recent: usize,
    external_pins: Option<&[usize]>,
    external_working_set_paths: Option<&[String]>,
) -> CompactionPlan {
    let mut pinned_indices: BTreeSet<usize> = BTreeSet::new();
    let len = messages.len();
    if len == 0 {
        return CompactionPlan::default();
    }

    // Always pin the tail of the conversation to preserve immediate context.
    let recent_start = len.saturating_sub(keep_recent);
    pinned_indices.extend(recent_start..len);

    // Derive a repo-aware working set from recent messages/tool calls and
    // merge it with any externally provided working-set paths.
    let seed_indices = external_pins.unwrap_or(&[]);
    let mut working_set_paths = derive_working_set_paths(messages, workspace, seed_indices);
    if let Some(paths) = external_working_set_paths {
        for path in paths {
            if let Some(normalized) = normalize_path_candidate(path, workspace) {
                let _ = working_set_paths.insert(normalized);
            }
        }
    }

    for (idx, msg) in messages.iter().enumerate() {
        if pinned_indices.contains(&idx) {
            continue;
        }
        let text = message_text(msg);
        if should_pin_message(&text, &working_set_paths) {
            pinned_indices.insert(idx);
        }
    }

    // External pins are authoritative and should be preserved even if they
    // were not detected by the heuristics above.
    if let Some(pins) = external_pins {
        pinned_indices.extend(pins.iter().copied().filter(|idx| *idx < len));
    }

    // Ensure tool result messages are not kept without their corresponding tool call.
    enforce_tool_call_pairs(messages, &mut pinned_indices);

    let summarize_indices = (0..len)
        .filter(|idx| !pinned_indices.contains(idx))
        .collect();

    // `working_set_paths` was used only for pinning decisions above.
    drop(working_set_paths);

    CompactionPlan {
        pinned_indices,
        summarize_indices,
    }
}

pub(crate) fn enforce_tool_call_pairs(messages: &[Message], pinned_indices: &mut BTreeSet<usize>) {
    if pinned_indices.is_empty() {
        return;
    }

    // Build maps: tool_id → message index across ALL messages (not just pinned).
    let mut call_id_to_idx: HashMap<String, usize> = HashMap::new();
    let mut result_id_to_idx: HashMap<String, usize> = HashMap::new();

    for (idx, msg) in messages.iter().enumerate() {
        for block in &msg.content {
            match block {
                ContentBlock::ToolUse { id, .. } => {
                    call_id_to_idx.insert(id.clone(), idx);
                }
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    result_id_to_idx.insert(tool_use_id.clone(), idx);
                }
                _ => {}
            }
        }
    }

    // Fixpoint loop: re-check until stable.
    // Newly pinned messages may introduce new pair requirements;
    // removed messages may orphan their counterparts.
    // Track permanently removed indices so they cannot be re-added
    // by a counterpart in a later iteration (prevents oscillation).
    let mut permanently_removed: HashSet<usize> = HashSet::new();

    let max_iters = messages.len().max(10);
    let mut converged = false;
    for _ in 0..max_iters {
        let mut to_add = Vec::new();
        let mut to_remove = Vec::new();

        let snapshot: Vec<usize> = pinned_indices.iter().copied().collect();

        for idx in snapshot {
            let msg = &messages[idx];
            for block in &msg.content {
                match block {
                    // Pinned result → its call must also be pinned (or remove result)
                    ContentBlock::ToolResult { tool_use_id, .. } => {
                        match call_id_to_idx.get(tool_use_id) {
                            Some(&call_idx) if !permanently_removed.contains(&call_idx) => {
                                to_add.push(call_idx);
                            }
                            _ => {
                                to_remove.push(idx);
                            }
                        }
                    }
                    // Pinned call → its result must also be pinned (or remove call)
                    ContentBlock::ToolUse { id, .. } => match result_id_to_idx.get(id) {
                        Some(&result_idx) if !permanently_removed.contains(&result_idx) => {
                            to_add.push(result_idx);
                        }
                        _ => {
                            to_remove.push(idx);
                        }
                    },
                    _ => {}
                }
            }
        }

        // Removals take priority: if a message is both needed and orphaned,
        // remove it now; the fixpoint loop will cascade the orphaning.
        let remove_set: HashSet<usize> = to_remove.iter().copied().collect();
        let mut changed = false;
        for idx in to_add {
            if !remove_set.contains(&idx) && pinned_indices.insert(idx) {
                changed = true;
            }
        }
        for idx in to_remove {
            if pinned_indices.remove(&idx) {
                permanently_removed.insert(idx);
                changed = true;
            }
        }

        if !changed {
            converged = true;
            break;
        }
    }
    if !converged {
        logging::warn(format!(
            "enforce_tool_call_pairs did not converge after {max_iters} iterations \
             ({} messages, {} pinned)",
            messages.len(),
            pinned_indices.len()
        ));
    }
}

