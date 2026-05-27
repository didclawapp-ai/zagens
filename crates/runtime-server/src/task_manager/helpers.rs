use std::collections::HashMap;

use anyhow::{Result, bail};
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::TaskRecord;

pub(crate) const TIMELINE_SUMMARY_LIMIT: usize = 240;
pub(crate) const ARTIFACT_THRESHOLD: usize = 1200;

pub(super) fn resolve_task_id(tasks: &HashMap<String, TaskRecord>, id_or_prefix: &str) -> Result<String> {
    if tasks.contains_key(id_or_prefix) {
        return Ok(id_or_prefix.to_string());
    }
    let matches = tasks
        .keys()
        .filter(|id| id.starts_with(id_or_prefix))
        .cloned()
        .collect::<Vec<_>>();
    match matches.len() {
        0 => bail!("Task not found: {id_or_prefix}"),
        1 => Ok(matches[0].clone()),
        _ => bail!(
            "Ambiguous task prefix '{}': matches {} tasks",
            id_or_prefix,
            matches.len()
        ),
    }
}

pub(super) fn summarize_json(value: &Value) -> Option<String> {
    let text = serde_json::to_string(value).ok()?;
    Some(summarize_text(&text, TIMELINE_SUMMARY_LIMIT))
}

pub(super) fn summarize_text(text: &str, limit: usize) -> String {
    let take = limit.saturating_sub(3);
    let mut count = 0;
    let mut out = String::new();
    for ch in text.chars() {
        if count >= take {
            out.push_str("...");
            return out;
        }
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        out.push(ch);
        count += 1;
    }
    out
}

pub(super) fn sanitize_filename(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "artifact".to_string()
    } else {
        out
    }
}

pub(super) fn duration_ms(start: DateTime<Utc>, end: DateTime<Utc>) -> u64 {
    let millis = (end - start).num_milliseconds();
    if millis.is_negative() {
        0
    } else {
        u64::try_from(millis).unwrap_or(u64::MAX)
    }
}
