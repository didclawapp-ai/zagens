//! Compact on-disk step journal for sub-agents (debug / anti-black-box).
//!
//! Stores truncated step/tool metadata under
//! `.zagens/state/subagent-journals/{agent_id}.json` — not the full child
//! message transcript (which can exceed millions of tokens).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zagens_config::workspace_meta_file_write;

use super::factory::{epoch_millis_now, write_json_atomic};

pub(crate) const JOURNAL_SCHEMA_VERSION: u32 = 1;
pub(crate) const MAX_JOURNAL_ENTRIES: usize = 500;
pub(crate) const PROMPT_PREVIEW_CHARS: usize = 400;
pub(crate) const DETAIL_PREVIEW_CHARS: usize = 240;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JournalKind {
    Started,
    ModelRequest,
    ToolStart,
    ToolEnd,
    Complete,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct JournalEntry {
    pub ts_ms: u64,
    pub step: u32,
    pub kind: JournalKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SubAgentJournal {
    pub schema_version: u32,
    pub agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_preview: Option<String>,
    pub tools_executed: u32,
    /// Running sum of tool-result UTF-8 bytes (context growth signal).
    pub estimated_context_chars: u64,
    pub entries: Vec<JournalEntry>,
    pub updated_at_ms: u64,
}

impl SubAgentJournal {
    fn new(
        agent_id: String,
        agent_type: Option<String>,
        parent_thread_id: Option<String>,
        prompt_preview: Option<String>,
    ) -> Self {
        Self {
            schema_version: JOURNAL_SCHEMA_VERSION,
            agent_id,
            agent_type,
            parent_thread_id,
            prompt_preview,
            tools_executed: 0,
            estimated_context_chars: 0,
            entries: Vec::new(),
            updated_at_ms: epoch_millis_now(),
        }
    }

    fn push_entry(&mut self, entry: JournalEntry) {
        self.entries.push(entry);
        if self.entries.len() > MAX_JOURNAL_ENTRIES {
            let drop_n = self.entries.len() - MAX_JOURNAL_ENTRIES;
            self.entries.drain(0..drop_n);
        }
        self.updated_at_ms = epoch_millis_now();
    }
}

pub(crate) fn journal_rel_path(agent_id: &str) -> String {
    format!("state/subagent-journals/{agent_id}.json")
}

pub(crate) fn journal_path(workspace: &Path, agent_id: &str) -> PathBuf {
    workspace_meta_file_write(workspace, &journal_rel_path(agent_id))
}

fn truncate_chars(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out = trimmed
        .chars()
        .take(max.saturating_sub(1))
        .collect::<String>();
    out.push('…');
    out
}

/// Summarize tool input for the journal (paths / patterns only — no full blobs).
pub(crate) fn summarize_tool_input(input: &Value) -> Option<String> {
    let obj = input.as_object()?;
    for key in [
        "path",
        "file",
        "file_path",
        "filepath",
        "target",
        "glob",
        "pattern",
        "query",
        "command",
        "cmd",
        "url",
    ] {
        if let Some(v) = obj.get(key)
            && let Some(s) = v.as_str()
        {
            let t = s.trim();
            if !t.is_empty() {
                return Some(format!("{key}={}", truncate_chars(t, DETAIL_PREVIEW_CHARS)));
            }
        }
    }
    // Fallback: compact JSON truncated.
    let raw = serde_json::to_string(input).ok()?;
    if raw == "{}" || raw == "null" {
        return None;
    }
    Some(truncate_chars(&raw, DETAIL_PREVIEW_CHARS))
}

fn load_or_new(
    path: &Path,
    agent_id: &str,
    agent_type: Option<String>,
    parent_thread_id: Option<String>,
    prompt_preview: Option<String>,
) -> SubAgentJournal {
    if path.exists()
        && let Ok(raw) = fs::read_to_string(path)
        && let Ok(mut existing) = serde_json::from_str::<SubAgentJournal>(&raw)
        && existing.schema_version == JOURNAL_SCHEMA_VERSION
        && existing.agent_id == agent_id
    {
        if existing.agent_type.is_none() {
            existing.agent_type = agent_type;
        }
        if existing.parent_thread_id.is_none() {
            existing.parent_thread_id = parent_thread_id;
        }
        if existing.prompt_preview.is_none() {
            existing.prompt_preview = prompt_preview;
        }
        return existing;
    }
    SubAgentJournal::new(
        agent_id.to_string(),
        agent_type,
        parent_thread_id,
        prompt_preview,
    )
}

fn mutate_journal(
    workspace: &Path,
    agent_id: &str,
    agent_type: Option<&str>,
    parent_thread_id: Option<&str>,
    prompt: Option<&str>,
    mutate: impl FnOnce(&mut SubAgentJournal),
) {
    let path = journal_path(workspace, agent_id);
    let mut journal = load_or_new(
        &path,
        agent_id,
        agent_type.map(str::to_string),
        parent_thread_id.map(str::to_string),
        prompt.map(|p| truncate_chars(p, PROMPT_PREVIEW_CHARS)),
    );
    mutate(&mut journal);
    if let Err(err) = write_json_atomic(&path, &journal) {
        eprintln!("Failed to write sub-agent journal {agent_id}: {err}");
    }
}

pub(crate) fn journal_started(
    workspace: &Path,
    agent_id: &str,
    agent_type: &str,
    prompt: &str,
    parent_thread_id: Option<&str>,
) {
    mutate_journal(
        workspace,
        agent_id,
        Some(agent_type),
        parent_thread_id,
        Some(prompt),
        |j| {
            j.tools_executed = 0;
            j.estimated_context_chars = 0;
            j.entries.clear();
            j.push_entry(JournalEntry {
                ts_ms: epoch_millis_now(),
                step: 0,
                kind: JournalKind::Started,
                tool: None,
                detail: Some(format!("type={agent_type}")),
                result_bytes: None,
                ok: None,
            });
        },
    );
}

pub(crate) fn journal_model_request(workspace: &Path, agent_id: &str, step: u32) {
    mutate_journal(workspace, agent_id, None, None, None, |j| {
        j.push_entry(JournalEntry {
            ts_ms: epoch_millis_now(),
            step,
            kind: JournalKind::ModelRequest,
            tool: None,
            detail: None,
            result_bytes: None,
            ok: None,
        });
    });
}

pub(crate) fn journal_tool_start(
    workspace: &Path,
    agent_id: &str,
    step: u32,
    tool_name: &str,
    detail: Option<String>,
) {
    mutate_journal(workspace, agent_id, None, None, None, |j| {
        j.push_entry(JournalEntry {
            ts_ms: epoch_millis_now(),
            step,
            kind: JournalKind::ToolStart,
            tool: Some(tool_name.to_string()),
            detail,
            result_bytes: None,
            ok: None,
        });
    });
}

pub(crate) fn journal_tool_end(
    workspace: &Path,
    agent_id: &str,
    step: u32,
    tool_name: &str,
    result_bytes: u64,
    ok: bool,
) {
    mutate_journal(workspace, agent_id, None, None, None, |j| {
        j.tools_executed = j.tools_executed.saturating_add(1);
        j.estimated_context_chars = j.estimated_context_chars.saturating_add(result_bytes);
        j.push_entry(JournalEntry {
            ts_ms: epoch_millis_now(),
            step,
            kind: JournalKind::ToolEnd,
            tool: Some(tool_name.to_string()),
            detail: None,
            result_bytes: Some(result_bytes),
            ok: Some(ok),
        });
    });
}

pub(crate) fn journal_terminal(
    workspace: &Path,
    agent_id: &str,
    step: u32,
    kind: JournalKind,
    detail: Option<String>,
) {
    mutate_journal(workspace, agent_id, None, None, None, |j| {
        j.push_entry(JournalEntry {
            ts_ms: epoch_millis_now(),
            step,
            kind,
            tool: None,
            detail,
            result_bytes: None,
            ok: None,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn summarize_prefers_path_keys() {
        let input = json!({"path": "crates/foo/src/lib.rs", "offset": 1});
        assert_eq!(
            summarize_tool_input(&input).as_deref(),
            Some("path=crates/foo/src/lib.rs")
        );
    }

    #[test]
    fn journal_roundtrip_counts_tools_and_caps_entries() {
        let dir = tempdir().expect("tempdir");
        let ws = dir.path();
        journal_started(
            ws,
            "agent_deadbeef",
            "review",
            "review the crate",
            Some("thr_1"),
        );
        for i in 1..=3 {
            journal_model_request(ws, "agent_deadbeef", i);
            journal_tool_start(
                ws,
                "agent_deadbeef",
                i,
                "read_file",
                Some("path=big.json".into()),
            );
            journal_tool_end(ws, "agent_deadbeef", i, "read_file", 10_000, true);
        }
        journal_terminal(
            ws,
            "agent_deadbeef",
            3,
            JournalKind::Failed,
            Some("context overflow".into()),
        );

        let path = journal_path(ws, "agent_deadbeef");
        let raw = fs::read_to_string(path).expect("read journal");
        let parsed: SubAgentJournal = serde_json::from_str(&raw).expect("parse");
        assert_eq!(parsed.tools_executed, 3);
        assert_eq!(parsed.estimated_context_chars, 30_000);
        assert!(parsed.entries.len() >= 8);
        assert_eq!(parsed.parent_thread_id.as_deref(), Some("thr_1"));
        assert!(
            parsed
                .entries
                .iter()
                .any(|e| matches!(e.kind, JournalKind::Failed))
        );
    }

    #[test]
    fn journal_trims_to_max_entries() {
        let dir = tempdir().expect("tempdir");
        let ws = dir.path();
        journal_started(ws, "agent_trim", "explore", "x", None);
        for i in 0..(MAX_JOURNAL_ENTRIES + 20) {
            journal_model_request(ws, "agent_trim", i as u32);
        }
        let path = journal_path(ws, "agent_trim");
        let parsed: SubAgentJournal =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(parsed.entries.len(), MAX_JOURNAL_ENTRIES);
    }
}
