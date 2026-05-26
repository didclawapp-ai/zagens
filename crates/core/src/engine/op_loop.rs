//! Core engine event loop (`Op` dispatch).

use crate::engine::op::Op;
use crate::engine::platform_ext::EnginePlatformExt;
use crate::engine::runtime::Engine;
use crate::session;

impl<P, R> Engine<P, R>
where
    P: Send + Sync + 'static,
    R: Send + Sync + 'static,
{
    /// Run the engine op loop until [`Op::Shutdown`] or channel close.
    pub async fn run(mut self) {
        while let Some(op) = self.rx_op.recv().await {
            if matches!(op, Op::Shutdown) {
                break;
            }

            if Self::handle_core_op(&mut self, op).await {
                continue;
            }
        }

        Self::on_shutdown(&mut self).await;
    }

    async fn on_shutdown(engine: &mut Self) {
        let ext_ptr = engine.ext.as_mut() as *mut dyn EnginePlatformExt<P, R>;
        let engine_ptr = engine as *mut Self;
        // SAFETY: `ext` and the rest of `Engine` are disjoint fields.
        unsafe {
            (&mut *ext_ptr).on_shutdown(&mut *engine_ptr).await;
        }
    }

    async fn handle_core_op(engine: &mut Self, op: Op) -> bool {
        match op {
            Op::CancelRequest => {
                engine.cancel_token.cancel();
                engine.reset_cancel_token();
                true
            }
            Op::ApproveToolCall { id } => {
                let _ = engine
                    .tx_approval
                    .send(crate::engine::approval::ApprovalDecision::Approved { id })
                    .await;
                true
            }
            Op::DenyToolCall { id } => {
                let _ = engine
                    .tx_approval
                    .send(crate::engine::approval::ApprovalDecision::Denied { id })
                    .await;
                true
            }
            Op::TruncateBeforeLastUserMessage { reply } => {
                let truncated =
                    session::truncate_before_last_user_message(&mut engine.session.messages);
                let _ = reply.send(truncated);
                true
            }
            other => {
                let ext_ptr = engine.ext.as_mut() as *mut dyn EnginePlatformExt<P, R>;
                let engine_ptr = engine as *mut Self;
                // SAFETY: `ext` and the rest of `Engine` are disjoint fields.
                unsafe {
                    (&mut *ext_ptr).dispatch_op(&mut *engine_ptr, other).await;
                }
                true
            }
        }
    }

    fn reset_cancel_token(&mut self) {
        let token = tokio_util::sync::CancellationToken::new();
        match self.shared_cancel_token.lock() {
            Ok(mut shared) => *shared = token.clone(),
            Err(poisoned) => *poisoned.into_inner() = token.clone(),
        }
        self.cancel_token = token;
    }
}
