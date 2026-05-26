//! Pre-request layered context (Flash seam) checkpoint (#159).

use super::*;
use deepseek_core::engine::hosts::SeamHost;

impl Engine {
    /// Run the pre-request layered-context checkpoint (#159). Checks whether
    /// the active input estimate has crossed a soft-seam threshold and, if so,
    /// produces an `<archived_context>` block via Flash and appends it as an
    /// assistant message. Called from `handle_deepseek_turn` before each API
    /// request so the model always has the latest navigation aids.
    ///
    /// M5: all calls dispatch through the `SeamHost` trait (the
    /// `seam_manager` field type stays `Option<SeamManager>` until M7
    /// swaps it to `Option<Box<dyn SeamHost>>`).
    pub(super) async fn layered_context_checkpoint(&mut self) {
        let Some(ref seam_mgr) = self.seam else {
            return;
        };
        if !SeamHost::config_enabled(seam_mgr.as_ref()) {
            return;
        }

        let highest = SeamHost::highest_level(seam_mgr.as_ref()).await;
        let Some(level) =
            SeamHost::seam_level_for(seam_mgr.as_ref(), self.estimated_input_tokens(), highest)
        else {
            return;
        };

        // Determine the message range to summarize: everything before the
        // verbatim window. The verbatim window (last ~16 turns) stays
        // untouched so the model always has ground-truth recent context.
        let msg_count = self.session.messages.len();
        let verbatim_start = SeamHost::verbatim_window_start(seam_mgr.as_ref(), msg_count);
        if verbatim_start == 0 {
            return; // Not enough messages to summarize.
        }

        let msg_range_end = verbatim_start;
        let pinned = self
            .session
            .working_set
            .pinned_message_indices(&self.session.messages, &self.session.workspace);

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "⏻ producing L{level} context seam ({msg_range_end} messages)…"
            )))
            .await;

        // If we have existing seams, recompact; otherwise produce fresh.
        let existing_seams = SeamHost::collect_seam_texts(seam_mgr.as_ref(), &self.session.messages).await;
        let seam_text = if existing_seams.is_empty() {
            match SeamHost::produce_soft_seam(
                seam_mgr.as_ref(),
                &self.session.messages,
                level,
                0,
                msg_range_end,
                Some(&self.session.workspace),
                &pinned,
            )
            .await
            {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!("L{level} soft seam failed: {err}"));
                    return;
                }
            }
        } else {
            let recent: Vec<&Message> = (0..msg_range_end)
                .filter_map(|i| self.session.messages.get(i))
                .collect();
            match SeamHost::recompact(
                seam_mgr.as_ref(),
                &existing_seams,
                &recent,
                level,
                0,
                msg_range_end,
            )
            .await
            {
                Ok(text) => text,
                Err(err) => {
                    crate::logging::warn(format!("L{level} recompact failed: {err}"));
                    return;
                }
            }
        };

        if seam_text.is_empty() {
            return;
        }

        // Capture seam count before the mutable borrow below.
        let seam_count = SeamHost::seam_count(seam_mgr.as_ref()).await;

        // Append the seam as an assistant message. This is an append-only
        // operation — no messages are deleted. The prefix cache stays hot.
        self.add_session_message(Message {
            role: "assistant".to_string(),
            content: vec![ContentBlock::Text {
                text: seam_text,
                cache_control: None,
            }],
        })
        .await;

        let _ = self
            .tx_event
            .send(Event::status(format!(
                "⏻ L{level} seam complete ({seam_count} total, {msg_range_end} messages covered)"
            )))
            .await;
    }
}

