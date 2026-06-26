//! Kernel Trace Report bundle builder (KTR P0).
//!
//! Normalizes golden fixture JSON or SQLite kernel events into `trace.bundle.json`
//! and embeds it in the HTML report shell.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::kernel_event::{KernelEvent, KernelEventEnvelope};
use super::turn_machine::{
    build_thread_replay_report, replay_effect_counts, replay_thread_compaction_timeline,
    replay_thread_effect_counts, verify_turn_replay_coherence,
};

pub const TRACE_BUNDLE_SCHEMA_VERSION: u32 = 1;
pub const TRACE_BUNDLE_PLACEHOLDER: &str = "__ZAGENS_TRACE_BUNDLE__";
/// Opening tag for the JSON embed slot (must match `tools/trace-report/index.html`).
pub const TRACE_BUNDLE_SCRIPT_OPEN: &str =
    r#"<script type="application/json" id="zagens-trace-bundle">"#;

/// Golden replay fixtures (keep in sync with `kernel_event_golden.rs`).
pub const GOLDEN_FIXTURE_NAMES: &[&str] = &[
    "pure_read.json",
    "write_batch.json",
    "lht_continue.json",
    "loop_guard.json",
    "scratchpad_compaction.json",
    "cycle_handoff.json",
    "overflow_recovery.json",
    "capacity_checkpoint.json",
    "manual_compaction.json",
    "deferred_activation.json",
    "memory_plane_query.json",
    "resume_thread_parity.json",
    "layered_context_seam.json",
    "message_body_rebuild.json",
    "system_prompt_refresh.json",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceBundleGenerator {
    pub tool: String,
    pub version: String,
    pub generated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TraceBundleSource {
    Fixture {
        fixture_path: String,
    },
    Thread {
        thread_id: String,
        workspace_label: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceTurnSummary {
    pub turn_id: String,
    pub event_count: usize,
    pub coherence_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coherence_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceReplaySummary {
    pub coherence_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coherence_error: Option<String>,
    pub turns: Vec<TraceTurnSummary>,
    pub effect_counts: super::turn_machine::ReplayEffectCounts,
    pub synthetic_timeline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceEventEnvelope {
    pub seq: u64,
    pub ts_ms: u64,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceRedaction {
    pub applied: bool,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCompactionEntry {
    pub turn_id: String,
    pub artifact_id: String,
    pub replaced_from: u32,
    pub replaced_to: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceCapacityEntry {
    pub turn_id: String,
    pub step_idx: u32,
    pub tokens_used: u32,
    pub token_budget: u32,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceAnalysis {
    pub compaction_timeline: Vec<TraceCompactionEntry>,
    pub capacity_checkpoints: Vec<TraceCapacityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraceBundle {
    pub schema_version: u32,
    pub generator: TraceBundleGenerator,
    pub source: TraceBundleSource,
    pub replay_summary: TraceReplaySummary,
    pub events: Vec<TraceEventEnvelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub harness: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analysis: Option<TraceAnalysis>,
    pub redaction: TraceRedaction,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn generator_meta() -> TraceBundleGenerator {
    TraceBundleGenerator {
        tool: "zagens-trace".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        generated_at_ms: now_ms(),
    }
}

fn outcome_label(outcome: &super::kernel_event::TurnOutcome) -> String {
    format!("{outcome:?}")
}

fn turn_id_from_events(events: &[KernelEvent]) -> String {
    events
        .iter()
        .find_map(KernelEvent::turn_id)
        .unwrap_or("unknown")
        .to_string()
}

/// Load a golden fixture JSON array as `KernelEvent` values (same path as CI golden tests).
pub fn load_fixture_kernel_events(path: &Path) -> Result<Vec<KernelEvent>> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read fixture {}", path.display()))?;
    serde_json::from_str(&raw).with_context(|| format!("parse fixture {}", path.display()))
}

/// Assign monotone `seq` and synthetic `ts_ms` for fixture timelines.
#[must_use]
pub fn normalize_fixture_envelopes(events: &[KernelEvent]) -> Vec<TraceEventEnvelope> {
    events
        .iter()
        .enumerate()
        .map(|(idx, event)| {
            let seq = u64::try_from(idx + 1).unwrap_or(u64::MAX);
            TraceEventEnvelope {
                seq,
                ts_ms: seq.saturating_mul(1000),
                payload: serde_json::to_value(event).unwrap_or(Value::Null),
            }
        })
        .collect()
}

/// Map persisted kernel log envelopes into the trace bundle wire format.
#[must_use]
pub fn envelopes_from_kernel_log(envelopes: &[KernelEventEnvelope]) -> Vec<TraceEventEnvelope> {
    envelopes
        .iter()
        .map(|entry| TraceEventEnvelope {
            seq: entry.seq,
            ts_ms: entry.ts_ms,
            payload: serde_json::to_value(&entry.event).unwrap_or(Value::Null),
        })
        .collect()
}

fn map_turn_summaries(
    turns: Vec<super::turn_machine::ThreadTurnReplaySummary>,
) -> Vec<TraceTurnSummary> {
    turns
        .into_iter()
        .map(|t| TraceTurnSummary {
            turn_id: t.turn_id,
            event_count: t.event_count,
            coherence_ok: t.coherence_ok,
            coherence_error: t.coherence_error,
            outcome: t.outcome.as_ref().map(outcome_label),
        })
        .collect()
}

/// Build replay summary for a multi-turn thread.
#[must_use]
pub fn build_replay_summary_from_thread(
    thread_id: &str,
    turn_events: &[(String, Vec<KernelEvent>)],
) -> TraceReplaySummary {
    let report = build_thread_replay_report(thread_id, turn_events);
    let coherence_error = if report.all_coherent {
        None
    } else {
        Some(format!(
            "{}/{} turns coherent ({} turns with events)",
            report.turns_coherent, report.turns_with_events, report.turn_count
        ))
    };
    TraceReplaySummary {
        coherence_ok: report.all_coherent,
        coherence_error,
        turns: map_turn_summaries(report.turns),
        effect_counts: replay_thread_effect_counts(turn_events),
        synthetic_timeline: false,
    }
}

/// Derive Memory / Context panel data from kernel events.
#[must_use]
pub fn build_trace_analysis(turn_events: &[(String, Vec<KernelEvent>)]) -> TraceAnalysis {
    let compaction_timeline = replay_thread_compaction_timeline(turn_events)
        .into_iter()
        .map(|e| TraceCompactionEntry {
            turn_id: e.turn_id,
            artifact_id: e.artifact_id,
            replaced_from: e.replaced_from,
            replaced_to: e.replaced_to,
        })
        .collect();

    let mut capacity_checkpoints = Vec::new();
    for (turn_id, events) in turn_events {
        for event in events {
            if let KernelEvent::CapacityCheckpoint {
                step_idx,
                tokens_used,
                token_budget,
                action,
                ..
            } = event
            {
                capacity_checkpoints.push(TraceCapacityEntry {
                    turn_id: turn_id.clone(),
                    step_idx: *step_idx,
                    tokens_used: *tokens_used,
                    token_budget: *token_budget,
                    action: format!("{action:?}"),
                });
            }
        }
    }

    TraceAnalysis {
        compaction_timeline,
        capacity_checkpoints,
    }
}

/// Build a trace bundle from persisted thread turns + optional harness snapshot.
pub fn build_trace_bundle_from_thread(
    thread_id: &str,
    workspace_label: Option<String>,
    turn_events: &[(String, Vec<KernelEvent>)],
    trace_envelopes: Vec<TraceEventEnvelope>,
    harness: Option<Value>,
) -> Result<TraceBundle> {
    if turn_events.is_empty() {
        bail!("thread {thread_id} has no turns with kernel events");
    }

    Ok(TraceBundle {
        schema_version: TRACE_BUNDLE_SCHEMA_VERSION,
        generator: generator_meta(),
        source: TraceBundleSource::Thread {
            thread_id: thread_id.to_string(),
            workspace_label,
        },
        replay_summary: build_replay_summary_from_thread(thread_id, turn_events),
        events: trace_envelopes,
        harness,
        analysis: Some(build_trace_analysis(turn_events)),
        redaction: TraceRedaction {
            applied: false,
            rules: vec![],
        },
    })
}

/// Build replay summary using the same coherence gate as golden CI (`live = None`).
#[must_use]
pub fn build_replay_summary(
    events: &[KernelEvent],
    synthetic_timeline: bool,
) -> TraceReplaySummary {
    let turn_id = turn_id_from_events(events);
    let coherence_error = verify_turn_replay_coherence(events, None);
    let coherence_ok = coherence_error.is_none();
    let report = build_thread_replay_report(&turn_id, &[(turn_id.clone(), events.to_vec())]);

    let turns = report
        .turns
        .into_iter()
        .map(|t| TraceTurnSummary {
            turn_id: t.turn_id,
            event_count: t.event_count,
            coherence_ok: t.coherence_ok,
            coherence_error: t.coherence_error,
            outcome: t.outcome.as_ref().map(outcome_label),
        })
        .collect();

    TraceReplaySummary {
        coherence_ok,
        coherence_error,
        turns,
        effect_counts: replay_effect_counts(events),
        synthetic_timeline,
    }
}

/// Build a trace bundle from a golden fixture file.
pub fn build_trace_bundle_from_fixture(path: &Path) -> Result<TraceBundle> {
    let events = load_fixture_kernel_events(path)?;
    if events.is_empty() {
        bail!("fixture has no events: {}", path.display());
    }

    Ok(TraceBundle {
        schema_version: TRACE_BUNDLE_SCHEMA_VERSION,
        generator: generator_meta(),
        source: TraceBundleSource::Fixture {
            fixture_path: path.to_string_lossy().into_owned(),
        },
        replay_summary: build_replay_summary(&events, true),
        events: normalize_fixture_envelopes(&events),
        harness: None,
        analysis: Some(build_trace_analysis(&[(
            turn_id_from_events(&events),
            events,
        )])),
        redaction: TraceRedaction {
            applied: false,
            rules: vec![],
        },
    })
}

/// Serialize bundle to pretty JSON.
pub fn trace_bundle_to_json(bundle: &TraceBundle) -> Result<String> {
    serde_json::to_string_pretty(bundle).context("serialize trace bundle")
}

/// Embed JSON only into the `#zagens-trace-bundle` script slot.
///
/// The HTML shell also references `TRACE_BUNDLE_PLACEHOLDER` inside inlined JS as a
/// sentinel string — global `replace` would inject JSON there and break the module.
pub fn embed_json_in_trace_shell(template: &str, json: &str) -> Result<String> {
    if !template.contains(TRACE_BUNDLE_PLACEHOLDER) {
        bail!(
            "HTML template missing placeholder `{TRACE_BUNDLE_PLACEHOLDER}` — run `npm run build` in tools/trace-report/"
        );
    }
    let start = template
        .find(TRACE_BUNDLE_SCRIPT_OPEN)
        .with_context(|| format!("HTML template missing `{TRACE_BUNDLE_SCRIPT_OPEN}`"))?;
    let content_start = start + TRACE_BUNDLE_SCRIPT_OPEN.len();
    let close_rel = template[content_start..]
        .find("</script>")
        .context("HTML template missing closing </script> for trace bundle slot")?;
    let content_end = content_start + close_rel;
    let slot = &template[content_start..content_end];
    if slot != TRACE_BUNDLE_PLACEHOLDER {
        bail!(
            "trace bundle slot has unexpected content (expected exactly `{TRACE_BUNDLE_PLACEHOLDER}`)"
        );
    }
    // Prevent `</script>` inside JSON from terminating the HTML script element early.
    let safe_json = json.replace("</", "<\\/");
    let mut out = String::with_capacity(template.len().saturating_add(safe_json.len()));
    out.push_str(&template[..content_start]);
    out.push_str(&safe_json);
    out.push_str(&template[content_end..]);
    Ok(out)
}

/// Embed bundle JSON into the HTML shell template.
pub fn embed_trace_bundle_in_html(template: &str, bundle: &TraceBundle) -> Result<String> {
    let json = trace_bundle_to_json(bundle)?;
    embed_json_in_trace_shell(template, &json)
}

fn bearer_token_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"(?i)bearer\s+[A-Za-z0-9._\-+/=]{8,}").expect("bearer token regex")
    })
}

fn redact_string_secrets(input: &str) -> (String, bool) {
    let after_bearer = bearer_token_re().replace_all(input, "Bearer [REDACTED]");
    let mut changed = after_bearer.as_ref() != input;
    let mut out = after_bearer.to_string();
    for needle in [
        "api_key=",
        "apikey=",
        "api-key=",
        "token=",
        "secret=",
        "password=",
    ] {
        let lower = out.to_lowercase();
        let Some(idx) = lower.find(needle) else {
            continue;
        };
        let tail_start = idx + needle.len();
        if tail_start >= out.len() {
            continue;
        }
        let end = out[tail_start..]
            .find(|c: char| c.is_whitespace() || c == '&' || c == '"' || c == ',' || c == '}')
            .map_or(out.len(), |off| tail_start + off);
        if tail_start < end {
            out.replace_range(tail_start..end, "[REDACTED]");
            changed = true;
        }
    }
    (out, changed)
}

fn redact_json_value(value: &mut Value, rules: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let (new, changed) = redact_string_secrets(s);
            if changed {
                *s = new;
                rules.push("inline_secret".to_string());
            }
        }
        Value::Array(arr) => {
            for v in arr {
                redact_json_value(v, rules);
            }
        }
        Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                let key_lower = k.to_ascii_lowercase();
                if matches!(
                    key_lower.as_str(),
                    "api_key" | "authorization" | "token" | "secret" | "password" | "access_token"
                ) && let Value::String(s) = v
                    && !s.is_empty()
                {
                    *s = "[REDACTED]".to_string();
                    rules.push(format!("field:{k}"));
                }
                redact_json_value(v, rules);
            }
        }
        _ => {}
    }
}

/// Redact obvious secrets in event payloads and harness snapshots (thread export default).
pub fn apply_trace_redaction(bundle: &mut TraceBundle) {
    let mut rules = Vec::new();
    for event in &mut bundle.events {
        redact_json_value(&mut event.payload, &mut rules);
    }
    if let Some(harness) = bundle.harness.as_mut() {
        redact_json_value(harness, &mut rules);
    }
    rules.sort();
    rules.dedup();
    bundle.redaction = TraceRedaction {
        applied: !rules.is_empty(),
        rules,
    };
}

/// Default HTML shell path relative to the zagens-core crate (repo checkout layout).
#[must_use]
pub fn default_trace_report_template_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/trace-report/dist/report.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/harness/kernel-v3-replay")
            .join(name)
    }

    #[test]
    fn normalize_fixture_envelopes_assigns_seq_and_ts() {
        let events = load_fixture_kernel_events(&fixture_path("lht_continue.json")).unwrap();
        let envelopes = normalize_fixture_envelopes(&events);
        assert_eq!(envelopes.len(), events.len());
        assert_eq!(envelopes[0].seq, 1);
        assert_eq!(envelopes[0].ts_ms, 1000);
        assert_eq!(
            envelopes[0]
                .payload
                .get("event_type")
                .and_then(|v| v.as_str()),
            Some("turn_started")
        );
    }

    #[test]
    fn lht_continue_fixture_is_coherent() {
        let events = load_fixture_kernel_events(&fixture_path("lht_continue.json")).unwrap();
        let summary = build_replay_summary(&events, true);
        assert!(summary.coherence_ok, "{:?}", summary.coherence_error);
        assert!(summary.synthetic_timeline);
        assert_eq!(summary.turns.len(), 1);
        assert_eq!(summary.turns[0].event_count, 6);
    }

    #[test]
    fn all_golden_fixtures_build_bundle_and_coherence() {
        for name in GOLDEN_FIXTURE_NAMES {
            let path = fixture_path(name);
            let bundle =
                build_trace_bundle_from_fixture(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(bundle.schema_version, TRACE_BUNDLE_SCHEMA_VERSION);
            assert!(
                bundle.replay_summary.coherence_ok,
                "{name}: {:?}",
                bundle.replay_summary.coherence_error
            );
            assert!(!bundle.events.is_empty());
        }
    }

    #[test]
    fn embed_json_only_in_script_slot_preserves_js_sentinel() {
        let template = format!(
            "<script type=\"module\">const P=\"{TRACE_BUNDLE_PLACEHOLDER}\";</script>{TRACE_BUNDLE_SCRIPT_OPEN}{TRACE_BUNDLE_PLACEHOLDER}</script>"
        );
        let out = embed_json_in_trace_shell(&template, r#"{"ok":true}"#).unwrap();
        assert!(out.contains(&format!("const P=\"{TRACE_BUNDLE_PLACEHOLDER}\"")));
        assert!(out.contains(&format!("{TRACE_BUNDLE_SCRIPT_OPEN}{{\"ok\":true}}")));
    }

    #[test]
    fn redacts_bearer_in_event_payload() {
        let mut bundle =
            build_trace_bundle_from_fixture(&fixture_path("lht_continue.json")).unwrap();
        bundle.events[0].payload = serde_json::json!({
            "event_type": "turn_started",
            "note": "Authorization: Bearer sk-secret1234567890"
        });
        apply_trace_redaction(&mut bundle);
        assert!(bundle.redaction.applied);
        let note = bundle.events[0]
            .payload
            .get("note")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(!note.contains("sk-secret"));
        assert!(note.contains("[REDACTED]"));
    }
}
