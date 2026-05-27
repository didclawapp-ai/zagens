//! Engine host port — sidecar implements spawn/load/monitor (D16 E1-b phase 2).

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use deepseek_core::engine::{StartTurnParams, handle::EngineHandle};
use tokio_util::sync::CancellationToken;

use super::manager::RuntimeThreadManager;
use super::types::ThreadRecord;
use super::StartTurnRequest;

/// Sidecar-specific engine wiring: spawn, start-turn params, event monitor loop.
#[async_trait]
pub trait RuntimeThreadHost<P, R>: Send + Sync + Clone
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    async fn spawn_engine_for_thread(
        &self,
        thread: &ThreadRecord,
    ) -> Result<EngineHandle<P, R>>;

    async fn prepare_start_turn_params(
        &self,
        thread: &ThreadRecord,
        req: &StartTurnRequest,
        prompt: &str,
    ) -> Result<StartTurnParams>;

    async fn monitor_turn(
        &self,
        thread_id: String,
        turn_id: String,
        engine: EngineHandle<P, R>,
    ) -> Result<()>;
}

/// Spawn a background turn monitor with panic isolation (matches legacy sidecar behavior).
pub fn spawn_turn_monitor<P, R, H>(
    host: Arc<H>,
    thread_id: String,
    turn_id: String,
    engine: EngineHandle<P, R>,
    cancel_token: CancellationToken,
    log_label: &'static str,
) where
    H: RuntimeThreadHost<P, R> + 'static,
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    tokio::spawn(async move {
        if cancel_token.is_cancelled() {
            tracing::debug!("Skipping {log_label}: shutdown requested");
            return;
        }
        use futures_util::FutureExt;
        let result = std::panic::AssertUnwindSafe(host.monitor_turn(
            thread_id,
            turn_id,
            engine,
        ))
        .catch_unwind()
        .await;
        match result {
            Ok(res) => {
                if let Err(err) = res {
                    tracing::error!("Failed to monitor {log_label}: {err}");
                }
            }
            Err(panic_err) => {
                if let Some(msg) = panic_err.downcast_ref::<&str>() {
                    tracing::error!("{log_label} panicked: {msg}");
                } else if let Some(msg) = panic_err.downcast_ref::<String>() {
                    tracing::error!("{log_label} panicked: {msg}");
                } else {
                    tracing::error!("{log_label} panicked with unknown error");
                }
            }
        }
    });
}

impl<P, R> RuntimeThreadManager<P, R>
where
    P: Send + Sync + Clone + 'static,
    R: Send + Sync + Clone + 'static,
{
    pub async fn is_interrupt_requested(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<bool> {
        let active = self.active.lock().await;
        let Some(state) = active.engines.get(thread_id) else {
            return Ok(false);
        };
        let Some(turn) = state.active_turn.as_ref() else {
            return Ok(false);
        };
        Ok(turn.turn_id == turn_id && turn.interrupt_requested)
    }
}
