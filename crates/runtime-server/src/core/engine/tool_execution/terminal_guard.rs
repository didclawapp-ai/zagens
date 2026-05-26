//! RAII pause/resume of TUI terminal ownership during interactive tools.

use super::super::*;

/// RAII guard that pauses the TUI's terminal-state ownership for the duration
/// of an interactive tool, then restores it on drop.
pub(super) struct InteractiveTerminalGuard {
    tx: Option<mpsc::Sender<Event>>,
}

impl InteractiveTerminalGuard {
    pub(super) async fn engage(tx: mpsc::Sender<Event>, interactive: bool) -> Self {
        if !interactive {
            return Self { tx: None };
        }
        let _ = tx.send(Event::PauseEvents).await;
        Self { tx: Some(tx) }
    }
}

impl Drop for InteractiveTerminalGuard {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            if let Err(err) = tx.try_send(Event::ResumeEvents) {
                tracing::warn!(
                    target: "engine.tool_execution",
                    ?err,
                    "InteractiveTerminalGuard: try_send(ResumeEvents) failed; \
                     terminal may stay in paused state until the next \
                     pause/resume cycle"
                );
            }
        }
    }
}
