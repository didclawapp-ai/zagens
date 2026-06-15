//! Kernel log-driven engine resume (Phase 3b 6e).

use anyhow::Result;

use zagens_core::engine::KernelResumeHints;
use zagens_core::engine::Op;
use zagens_core::engine::turn_machine::kernel_resume_hints_from_projection;

use crate::core::engine::EngineHandle;
use crate::runtime_api::kernel_replay::collect_thread_kernel_replay;

use super::RuntimeThreadManager;

pub(crate) fn load_kernel_resume_hints(
    manager: &RuntimeThreadManager,
    thread_id: &str,
) -> Option<KernelResumeHints> {
    let projection = collect_thread_kernel_replay(manager, thread_id).ok()?;
    if projection.report.turns_with_events == 0 {
        return None;
    }
    Some(kernel_resume_hints_from_projection(
        &projection.latest_projection,
    ))
}

pub(crate) async fn push_kernel_resume_to_engine(
    manager: &RuntimeThreadManager,
    thread_id: &str,
    handle: &EngineHandle,
) -> Result<()> {
    let Some(hints) = load_kernel_resume_hints(manager, thread_id) else {
        return Ok(());
    };
    handle
        .send(Op::ApplyKernelResume { hints })
        .await
        .map_err(|e| anyhow::anyhow!("ApplyKernelResume send failed: {e}"))
}
