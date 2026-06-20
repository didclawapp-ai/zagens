//! Load persisted thread kernel events for trace export.

use anyhow::Result;
use zagens_core::engine::trace_bundle::TraceBundle;

use super::context::CliContext;
use crate::trace_export::build_trace_bundle_for_thread;

pub fn build_trace_bundle_for_thread_cli(
    ctx: &CliContext,
    thread_id: &str,
    include_harness: bool,
    redact: bool,
) -> Result<TraceBundle> {
    build_trace_bundle_for_thread(
        thread_id,
        &ctx.config,
        &ctx.workspace,
        include_harness,
        redact,
    )
}
