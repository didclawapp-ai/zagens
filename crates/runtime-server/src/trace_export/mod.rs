//! Shared Kernel Trace Report export (CLI + runtime HTTP).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use zagens_core::engine::KernelEvent;
use zagens_core::engine::trace_bundle::{
    TRACE_BUNDLE_PLACEHOLDER, TraceBundle, apply_trace_redaction, build_trace_bundle_from_thread,
    default_trace_report_template_path, embed_trace_bundle_in_html, envelopes_from_kernel_log,
    trace_bundle_to_json,
};
use zagens_runtime_adapters::persist::KernelEventWriter;
use zagens_runtime_orchestrator::runtime_threads::{
    RuntimeThreadManagerConfig, RuntimeThreadStore,
};

use crate::cli::trace_harness::build_offline_harness_snapshot;
use crate::config::Config;
use crate::task_manager::default_tasks_dir;

const EMBEDDED_TRACE_REPORT_TEMPLATE: &str = include_str!("../../assets/trace-report/report.html");

/// Resolve the HTML shell used for trace export.
pub fn load_trace_report_template(custom: Option<&Path>) -> Result<String> {
    if let Some(path) = custom {
        return fs::read_to_string(path)
            .with_context(|| format!("read HTML template {}", path.display()));
    }

    let default = default_trace_report_template_path();
    if default.is_file() {
        return fs::read_to_string(&default).with_context(|| {
            format!(
                "read HTML template {} (run `npm run build` in tools/trace-report/)",
                default.display()
            )
        });
    }

    let runtime_asset =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/trace-report/report.html");
    if runtime_asset.is_file() {
        return fs::read_to_string(&runtime_asset)
            .with_context(|| format!("read bundled HTML template {}", runtime_asset.display()));
    }

    if EMBEDDED_TRACE_REPORT_TEMPLATE.contains(TRACE_BUNDLE_PLACEHOLDER) {
        return Ok(EMBEDDED_TRACE_REPORT_TEMPLATE.to_string());
    }

    bail!(
        "trace report HTML shell not found — build tools/trace-report (`npm run build`) or set --template"
    )
}

pub fn render_trace_html(bundle: &TraceBundle, template: &str) -> Result<String> {
    embed_trace_bundle_in_html(template, bundle)
}

pub fn render_trace_bundle_json(bundle: &TraceBundle) -> Result<String> {
    trace_bundle_to_json(bundle)
}

/// Load kernel events + optional harness snapshot for a persisted thread.
pub fn build_trace_bundle_for_thread(
    thread_id: &str,
    config: &Config,
    workspace: &Path,
    include_harness: bool,
    redact: bool,
) -> Result<TraceBundle> {
    // Resolve the runtime task data dir the SAME way `task_manager` / `runtime_serve`
    // do, so `trace_export` opens the same `runtime.db` the live runtime wrote to.
    // Using `user_data_root()` directly would point at `~/.zagens/runtime/runtime.db`
    // (an empty shadow copy) instead of `~/.zagens/tasks/runtime/runtime.db`.
    let task_data_dir = default_tasks_dir();
    let manager_cfg = RuntimeThreadManagerConfig::from_task_data_dir(task_data_dir);
    let data_dir = manager_cfg.data_dir.clone();
    let store = RuntimeThreadStore::open(data_dir)
        .with_context(|| format!("open runtime store at {}", manager_cfg.data_dir.display()))?;

    let thread = store
        .load_thread(thread_id)
        .with_context(|| format!("load thread {thread_id}"))?;

    let turns = store
        .list_turns_for_thread(thread_id)
        .with_context(|| format!("list turns for thread {thread_id}"))?;

    let writer = KernelEventWriter::try_open_default().ok_or_else(|| {
        anyhow::anyhow!("kernel event log unavailable (~/.zagens/sessions/sessions.db)")
    })?;

    let mut turn_events: Vec<(String, Vec<KernelEvent>)> = Vec::new();
    let mut all_envelopes = Vec::new();
    let mut skipped_empty = 0usize;
    let mut first_turn_id: Option<String> = None;
    let mut aliased: Vec<(String, String)> = Vec::new(); // (runtime_id, sessions_id)
    let mut time_windowed: Vec<String> = Vec::new();

    for turn in turns {
        if first_turn_id.is_none() {
            first_turn_id = Some(turn.id.clone());
        }
        let mut envelopes = writer
            .load_turn_envelopes_sync(&turn.id)
            .with_context(|| format!("load kernel events for turn {}", turn.id))?;
        // Historical turns: the orchestrator `turn_…` id may differ from the
        // engine-internal UUID stored in sessions.db. Try (1) prefix/alias
        // resolve, then (2) time-window fallback using the turn's
        // started_at..ended_at range (see CHANGELOG [Unreleased] trace export fix).
        if envelopes.is_empty() {
            if let Ok(Some((alias_id, _count))) = writer.resolve_turn_id_alias(&turn.id) {
                envelopes = writer
                    .load_turn_envelopes_sync(&alias_id)
                    .with_context(|| {
                        format!("load kernel events for turn {} (alias {alias_id})", turn.id)
                    })?;
                if !envelopes.is_empty() {
                    aliased.push((turn.id.clone(), alias_id));
                }
            }
        }
        if envelopes.is_empty() {
            // Time-window fallback: pre-fix turns wrote kernel events under an
            // engine UUID not persisted in runtime.db. Use the turn's wall-clock
            // range to scoop events back up. Only attempt when the turn actually
            // ran (has started_at) — pending/errored turns have no window.
            if let (Some(started), Some(ended)) = (turn.started_at, turn.ended_at) {
                let from_ms = started.timestamp_millis().max(0) as u64;
                // Pad the window by 5s on each side to absorb ts_ms skew between
                // engine emit time and orchestrator ended_at recording.
                let to_ms = ended.timestamp_millis().saturating_add(5_000).max(0) as u64;
                let from_padded = from_ms.saturating_sub(5_000);
                let windowed = writer
                    .load_events_by_time_window(from_padded, to_ms)
                    .with_context(|| {
                        format!("load kernel events by time window for turn {}", turn.id)
                    })?;
                if !windowed.is_empty() {
                    time_windowed.push(turn.id.clone());
                    envelopes = windowed;
                }
            }
        }
        if envelopes.is_empty() {
            skipped_empty += 1;
            continue;
        }
        let events: Vec<KernelEvent> = envelopes.iter().map(|e| e.event.clone()).collect();
        all_envelopes.extend(envelopes_from_kernel_log(&envelopes));
        turn_events.push((turn.id, events));
    }

    if turn_events.is_empty() {
        let total = skipped_empty;
        if total == 0 {
            bail!(
                "thread {thread_id} has no turn records in runtime.db \
                 (thread exists but list_turns_for_thread returned empty)"
            );
        }
        // Diagnose sessions.db so the user can tell apart the three failure modes:
        // empty db / turn_id mismatch / wrong db path.
        let turn_prefix_count = writer.count_turn_ids_like("turn_%").unwrap_or(0);
        let diag = writer
            .diagnose_turn_ids(8)
            .map(|(total_rows, earliest, latest)| {
                format!(
                    "sessions.db kernel_events: {total_rows} row(s) with turn_id; \
                     {turn_prefix_count} distinct turn_id(s) with `turn_` prefix; \
                     earliest turn_ids: [{earliest}]; latest turn_ids: [{latest}]",
                    earliest = earliest.join(", "),
                    latest = latest.join(", ")
                )
            })
            .unwrap_or_else(|e| format!("sessions.db diagnose failed: {e}"));
        bail!(
            "thread {thread_id} has {total} turn record(s) in runtime.db but \
             none have kernel events in sessions.db (first turn id: {first}). \
             {diag}. Possible causes: turn predates kernel-event logging, \
             turn_id mismatch between runtime.db and sessions.db, \
             or sessions.db at a different path.",
            first = first_turn_id.as_deref().unwrap_or("?")
        );
    }

    if !aliased.is_empty() {
        eprintln!(
            "[trace] recovered {} turn(s) via alias resolve: {}",
            aliased.len(),
            aliased
                .iter()
                .map(|(r, s)| format!("{r} → {s}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if !time_windowed.is_empty() {
        eprintln!(
            "[trace] recovered {} turn(s) via time-window fallback (pre-fix engine UUID): {}",
            time_windowed.len(),
            time_windowed.join(", ")
        );
    }

    let workspace_label = thread
        .workspace
        .to_str()
        .map(str::to_string)
        .or_else(|| workspace.to_str().map(str::to_string));

    let lht = config.long_horizon_config();
    let harness = if include_harness {
        Some(build_offline_harness_snapshot(
            &thread,
            &lht,
            Some(&store),
            thread_id,
        ))
    } else {
        None
    };

    let mut bundle = build_trace_bundle_from_thread(
        thread_id,
        workspace_label,
        &turn_events,
        all_envelopes,
        harness,
    )?;

    if redact {
        apply_trace_redaction(&mut bundle);
    }

    Ok(bundle)
}

/// Build a compare document from two persisted threads.
pub fn build_trace_compare_for_threads(
    left_id: &str,
    right_id: &str,
    config: &Config,
    workspace: &Path,
    include_harness: bool,
    redact: bool,
) -> Result<zagens_core::engine::TraceCompareDocument> {
    let left = build_trace_bundle_for_thread(left_id, config, workspace, include_harness, redact)?;
    let right =
        build_trace_bundle_for_thread(right_id, config, workspace, include_harness, redact)?;
    Ok(zagens_core::engine::build_trace_compare_document(
        left_id.to_string(),
        left,
        right_id.to_string(),
        right,
    ))
}

/// Inject a polling script that reloads when `/api/bundle.json` changes (thread watch mode).
pub fn inject_trace_watch_script(html: &str, interval_secs: u64) -> String {
    let script = format!(
        r#"<script>
(function() {{
  let revision = null;
  setInterval(async () => {{
    try {{
      const r = await fetch('/api/bundle.json');
      if (!r.ok) return;
      const rev = r.headers.get('X-Trace-Revision');
      if (rev && revision !== null && rev !== revision) location.reload();
      revision = rev;
    }} catch (_) {{}}
  }}, {interval_secs} * 1000);
}})();
</script>"#
    );
    if let Some(idx) = html.rfind("</body>") {
        let mut out = html.to_string();
        out.insert_str(idx, &script);
        out
    } else {
        format!("{html}{script}")
    }
}

/// Revision token for watch polling (event count + last seq).
#[must_use]
pub fn trace_bundle_revision(bundle: &TraceBundle) -> String {
    let last_seq = bundle.events.last().map(|e| e.seq).unwrap_or(0);
    format!("{}:{last_seq}", bundle.events.len())
}
