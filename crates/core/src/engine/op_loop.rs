//! Core engine event loop (`Op` dispatch).

use crate::engine::op::Op;
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
        let Some(mut ext) = engine.ext.take() else {
            return;
        };
        ext.on_shutdown().await;
        engine.ext = Some(ext);
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
                    .send(crate::engine::approval::ApprovalDecision::Approved {
                        id,
                        cache_key: None,
                        remember_for_session: false,
                    })
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
                let Some(mut ext) = engine.ext.take() else {
                    return true;
                };
                ext.dispatch_op(engine, other).await;
                engine.ext = Some(ext);
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
