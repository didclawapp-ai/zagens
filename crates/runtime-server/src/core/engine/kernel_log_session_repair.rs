//! v3 resume — repair engine session transcript from kernel log previews (Phase 3b 5c).

use zagens_core::engine::turn_loop::message_body_rebuild_policy::{
    rebuild_preview_messages_from_thread_events, should_repair_session_from_kernel_log,
    verify_session_transcript_preview_bodies,
};
use zagens_core::engine::turn_machine::KernelResumeHints;

use crate::session_manager::{SessionManager, update_session};

use super::Engine;
use super::kernel_log_session_repair_shadow::{
    record_log_session_repair_persist_failed, record_log_session_repair_persist_ok,
    record_log_session_repair_persist_skipped_no_session, record_log_session_repair_run,
    record_log_session_repair_skipped_aligned,
};

impl Engine {
    fn load_thread_turn_events_for_hints(
        &self,
        hints: &KernelResumeHints,
    ) -> Option<Vec<(String, Vec<zagens_core::engine::kernel_event::KernelEvent>)>> {
        let writer = self.runtime_ext().kernel_event_writer.as_ref()?;
        let turn_ids: Vec<String> = if !hints.thread_turn_ids_with_events.is_empty() {
            hints.thread_turn_ids_with_events.clone()
        } else {
            hints
                .latest_turn_id
                .clone()
                .map(|id| vec![id])
                .unwrap_or_default()
        };
        if turn_ids.is_empty() {
            return None;
        }
        let mut turn_events = Vec::with_capacity(turn_ids.len());
        for turn_id in turn_ids {
            let events = writer.load_turn_events_sync(&turn_id).unwrap_or_default();
            if !events.is_empty() {
                turn_events.push((turn_id, events));
            }
        }
        if turn_events.is_empty() {
            None
        } else {
            Some(turn_events)
        }
    }

    async fn maybe_persist_repaired_session_to_disk(
        &self,
        hints: &KernelResumeHints,
        messages: &[zagens_core::chat::Message],
    ) {
        let ext = self.runtime_ext();
        if !ext.kernel_log_transcript_repair_persist {
            return;
        }
        let Some(session_manager) = ext.session_manager.clone() else {
            return;
        };
        let Some(thread_id) = hints.runtime_thread_id.clone() else {
            return;
        };
        let system_prompt = self.session.system_prompt.clone();
        let messages = messages.to_vec();
        let message_count = messages.len();
        let thread_id_for_log = thread_id.clone();
        let result = tokio::task::spawn_blocking(move || {
            persist_repaired_session_sync(
                session_manager.as_ref(),
                &thread_id,
                &messages,
                system_prompt.as_ref(),
            )
        })
        .await;
        match result {
            Ok(Ok(session_id)) => {
                record_log_session_repair_persist_ok();
                tracing::info!(
                    target: "kernel_resume",
                    session_id = %session_id,
                    thread_id = %thread_id_for_log,
                    message_count,
                    "persisted log-repaired session transcript to session store"
                );
            }
            Ok(Err(reason)) => match reason {
                PersistRepairedSessionError::NoLinkedSession => {
                    record_log_session_repair_persist_skipped_no_session();
                    tracing::debug!(
                        target: "kernel_resume",
                        thread_id = %thread_id_for_log,
                        "log transcript repair persist skipped: no linked session row"
                    );
                }
                PersistRepairedSessionError::Io(err) => {
                    record_log_session_repair_persist_failed();
                    tracing::warn!(
                        target: "kernel_resume",
                        thread_id = %thread_id_for_log,
                        error = %err,
                        "log transcript repair persist failed"
                    );
                }
            },
            Err(err) => {
                record_log_session_repair_persist_failed();
                tracing::warn!(
                    target: "kernel_resume",
                    thread_id = %thread_id_for_log,
                    error = %err,
                    "log transcript repair persist task failed"
                );
            }
        }
    }

    /// Replace divergent session rows with log-rebuilt preview transcript when enabled.
    pub(in crate::core::engine) async fn maybe_repair_session_from_kernel_log(
        &mut self,
        hints: &KernelResumeHints,
    ) {
        let ext = self.runtime_ext();
        if !ext.kernel_log_transcript_repair {
            return;
        }
        if !ext.kernel_machine_mode.uses_v3_turn_loop() {
            return;
        }
        if hints.kernel_transcript_preview_body_count == 0 {
            return;
        }
        let Some(turn_events) = self.load_thread_turn_events_for_hints(hints) else {
            tracing::warn!(
                target: "kernel_resume",
                "log transcript repair skipped: no turn events loaded"
            );
            return;
        };
        if !should_repair_session_from_kernel_log(&self.session.messages, &turn_events) {
            record_log_session_repair_skipped_aligned();
            return;
        }
        let repaired = rebuild_preview_messages_from_thread_events(&turn_events);
        if repaired.is_empty() {
            return;
        }
        let before = self.session.messages.len();
        self.session.messages = repaired;
        self.rehydrate_latest_canonical_state();
        self.emit_session_updated().await;
        record_log_session_repair_run(self.session.messages.len() as u64);
        tracing::info!(
            target: "kernel_resume",
            before_rows = before,
            after_rows = self.session.messages.len(),
            preview_body_events = hints.kernel_transcript_preview_body_count,
            "repaired engine session transcript from kernel log previews"
        );
        if verify_session_transcript_preview_bodies(&self.session.messages, &turn_events).is_some()
        {
            tracing::warn!(
                target: "kernel_resume",
                "log transcript repair still diverges after apply"
            );
        }
        self.maybe_persist_repaired_session_to_disk(hints, &self.session.messages)
            .await;
    }
}

enum PersistRepairedSessionError {
    NoLinkedSession,
    Io(std::io::Error),
}

fn persist_repaired_session_sync(
    session_manager: &SessionManager,
    runtime_thread_id: &str,
    messages: &[zagens_core::chat::Message],
    system_prompt: Option<&zagens_core::chat::SystemPrompt>,
) -> Result<String, PersistRepairedSessionError> {
    let Some(session_id) = session_manager
        .find_session_id_by_runtime_thread_id(runtime_thread_id)
        .map_err(PersistRepairedSessionError::Io)?
    else {
        return Err(PersistRepairedSessionError::NoLinkedSession);
    };
    let existing = session_manager
        .load_session(&session_id)
        .map_err(PersistRepairedSessionError::Io)?;
    let total_tokens = existing.metadata.total_tokens;
    let mut updated = update_session(existing, messages, total_tokens, system_prompt);
    updated.metadata.runtime_thread_id = Some(runtime_thread_id.to_string());
    session_manager
        .save_session(&updated)
        .map_err(PersistRepairedSessionError::Io)?;
    Ok(session_id)
}
