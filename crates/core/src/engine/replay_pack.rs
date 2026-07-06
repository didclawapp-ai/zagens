//! Replay pack v0 (Phase 3.4) — single-file trace + session metadata for import/export.
//!
//! Wraps [`TraceBundle`] with optional session transcript JSON and export metadata.
//! Compatible with golden fixtures under `fixtures/harness/kernel-v3-replay/`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::trace_bundle::{TraceBundle, build_trace_bundle_from_fixture, trace_bundle_to_json};

pub const REPLAY_PACK_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayPackMetadata {
    pub exported_at_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture_path: Option<String>,
    pub includes_session: bool,
    pub golden_replay_compatible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayPack {
    pub schema_version: u32,
    pub metadata: ReplayPackMetadata,
    pub trace: TraceBundle,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_messages: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplayPackValidation {
    pub ok: bool,
    pub schema_version: u32,
    pub coherence_ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coherence_error: Option<String>,
    pub event_count: usize,
    pub includes_session: bool,
    pub golden_replay_compatible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Companion session file for a golden fixture: `foo.json` → `foo.session.json`.
#[must_use]
pub fn companion_session_path(fixture: &Path) -> PathBuf {
    fixture.with_extension("session.json")
}

/// Load optional session transcript JSON adjacent to a golden fixture.
pub fn load_companion_session(fixture: &Path) -> Result<Option<Value>> {
    let path = companion_session_path(fixture);
    if !path.is_file() {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("read session companion {}", path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .with_context(|| format!("parse session companion {}", path.display()))?;
    Ok(Some(value))
}

#[must_use]
pub fn build_replay_pack(
    trace: TraceBundle,
    session_messages: Option<Value>,
    metadata: ReplayPackMetadata,
) -> ReplayPack {
    ReplayPack {
        schema_version: REPLAY_PACK_SCHEMA_VERSION,
        metadata,
        trace,
        session_messages,
    }
}

pub fn build_replay_pack_from_fixture(path: &Path) -> Result<ReplayPack> {
    let trace = build_trace_bundle_from_fixture(path)?;
    let session_messages = load_companion_session(path)?;
    let golden = path.to_string_lossy().contains("kernel-v3-replay")
        || super::trace_bundle::GOLDEN_FIXTURE_NAMES
            .iter()
            .any(|name| path.ends_with(name));
    Ok(build_replay_pack(
        trace,
        session_messages.clone(),
        ReplayPackMetadata {
            exported_at_ms: now_ms(),
            workspace_label: None,
            thread_id: None,
            fixture_path: Some(path.to_string_lossy().into_owned()),
            includes_session: session_messages.is_some(),
            golden_replay_compatible: golden,
        },
    ))
}

pub fn parse_replay_pack_json(raw: &str) -> Result<ReplayPack> {
    let pack: ReplayPack = serde_json::from_str(raw).context("parse replay pack JSON")?;
    if pack.schema_version > REPLAY_PACK_SCHEMA_VERSION {
        bail!(
            "unsupported replay pack schema {} (max supported {REPLAY_PACK_SCHEMA_VERSION})",
            pack.schema_version
        );
    }
    Ok(pack)
}

pub fn replay_pack_to_json(pack: &ReplayPack) -> Result<String> {
    serde_json::to_string_pretty(pack).context("serialize replay pack")
}

/// Validate coherence + basic shape (import gate for Replay v0).
#[must_use]
pub fn validate_replay_pack(pack: &ReplayPack) -> ReplayPackValidation {
    let mut warnings = Vec::new();
    if pack.schema_version != REPLAY_PACK_SCHEMA_VERSION {
        warnings.push(format!(
            "schema_version {} != current {REPLAY_PACK_SCHEMA_VERSION}",
            pack.schema_version
        ));
    }
    if pack.metadata.includes_session && pack.session_messages.is_none() {
        warnings.push("metadata.includes_session but session_messages missing".into());
    }
    if !pack.metadata.includes_session && pack.session_messages.is_some() {
        warnings.push("session_messages present but metadata.includes_session is false".into());
    }
    if pack.trace.events.is_empty() {
        warnings.push("trace.events is empty".into());
    }

    let coherence_ok = pack.trace.replay_summary.coherence_ok;
    let coherence_error = pack.trace.replay_summary.coherence_error.clone();
    let ok = coherence_ok
        && !pack.trace.events.is_empty()
        && warnings.iter().all(|w| !w.contains("missing"));

    ReplayPackValidation {
        ok,
        schema_version: pack.schema_version,
        coherence_ok,
        coherence_error,
        event_count: pack.trace.events.len(),
        includes_session: pack.session_messages.is_some(),
        golden_replay_compatible: pack.metadata.golden_replay_compatible,
        warnings,
    }
}

/// Round-trip helper: export trace-only slice for HTML/bundle consumers.
pub fn replay_pack_trace_bundle(pack: &ReplayPack) -> &TraceBundle {
    &pack.trace
}

/// Merge session into a standalone trace bundle JSON string (lossless trace sub-document).
pub fn replay_pack_trace_json(pack: &ReplayPack) -> Result<String> {
    trace_bundle_to_json(&pack.trace)
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
    fn fixture_with_session_companion_round_trips() {
        let path = fixture_path("message_body_rebuild.json");
        let pack = build_replay_pack_from_fixture(&path).expect("build");
        assert!(pack.metadata.includes_session);
        assert!(pack.session_messages.is_some());
        assert!(pack.metadata.golden_replay_compatible);

        let json = replay_pack_to_json(&pack).expect("json");
        let parsed = parse_replay_pack_json(&json).expect("parse");
        let validation = validate_replay_pack(&parsed);
        assert!(validation.coherence_ok, "{:?}", validation.coherence_error);
        assert!(validation.ok);
        assert!(validation.includes_session);
    }

    #[test]
    fn fixture_without_session_still_validates() {
        let path = fixture_path("lht_continue.json");
        let pack = build_replay_pack_from_fixture(&path).expect("build");
        assert!(!pack.metadata.includes_session);
        let validation = validate_replay_pack(&pack);
        assert!(validation.ok);
        assert!(validation.coherence_ok);
    }

    #[test]
    fn companion_session_path_suffix() {
        let p = Path::new("fixtures/pure_read.json");
        assert_eq!(
            companion_session_path(p),
            Path::new("fixtures/pure_read.session.json")
        );
    }
}
