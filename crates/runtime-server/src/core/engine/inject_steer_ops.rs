//! v3 effect interpreter entry for [`Effect::InjectSteer`].

use super::*;

use zagens_core::engine::context::summarize_text;
use zagens_core::engine::kernel_event::KernelEvent;
use zagens_core::engine::turn_machine::emit_kernel_event;

/// Upper bound for steer text accepted into the session transcript.
const MAX_STEER_TEXT_CHARS: usize = 8_192;

fn steer_text_has_disallowed_controls(text: &str) -> bool {
    text.chars()
        .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}

fn sanitize_steer_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if steer_text_has_disallowed_controls(trimmed) {
        tracing::warn!(
            target: "kernel_v3",
            "InjectSteer rejected: disallowed control characters"
        );
        return None;
    }
    let char_count = trimmed.chars().count();
    let out: String = trimmed.chars().take(MAX_STEER_TEXT_CHARS).collect();
    if char_count > MAX_STEER_TEXT_CHARS {
        tracing::warn!(
            target: "kernel_v3",
            original_chars = char_count,
            max = MAX_STEER_TEXT_CHARS,
            "InjectSteer truncated to max length"
        );
    }
    if out.is_empty() { None } else { Some(out) }
}

impl Engine {
    /// Inject steer text into the session transcript (mirrors op-loop steer drain).
    pub(in crate::core::engine) async fn run_inject_steer_effect(
        &mut self,
        turn_id: &str,
        step_idx: u32,
        text: String,
    ) {
        if self.try_run_pending_inject_steer_kind().await {
            return;
        }
        self.apply_inject_steer_text(turn_id, step_idx, text).await;
    }

    pub(in crate::core::engine) async fn apply_inject_steer_text(
        &mut self,
        turn_id: &str,
        step_idx: u32,
        text: String,
    ) {
        if self.effect_replay_anchor_only() {
            tracing::info!(
                target: "kernel_v3",
                "replay anchor-only: skipping InjectSteer IO"
            );
            return;
        }
        let Some(steer) = sanitize_steer_text(&text) else {
            return;
        };
        let workspace = self.session.workspace.clone();
        self.session
            .working_set
            .observe_user_message(&steer, &workspace);
        self.add_session_message(Message {
            role: "user".to_string(),
            content: vec![ContentBlock::Text {
                text: steer.clone(),
                cache_control: None,
            }],
        })
        .await;
        let _ = self
            .tx_event
            .send(Event::status(format!(
                "Steer input accepted: {}",
                summarize_text(&steer, 120)
            )))
            .await;
        emit_kernel_event(
            self,
            KernelEvent::SteerInjected {
                turn_id: turn_id.to_string(),
                step_idx,
                text: summarize_text(&steer, 512),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_steer_rejects_nul_control_char() {
        assert!(sanitize_steer_text("hello\u{0}world").is_none());
    }

    #[test]
    fn sanitize_steer_allows_newlines() {
        assert_eq!(
            sanitize_steer_text("  line1\nline2  "),
            Some("line1\nline2".to_string())
        );
    }

    #[test]
    fn sanitize_steer_truncates_long_input() {
        let long = "x".repeat(MAX_STEER_TEXT_CHARS + 10);
        let out = sanitize_steer_text(&long).expect("truncated steer");
        assert_eq!(out.chars().count(), MAX_STEER_TEXT_CHARS);
    }
}
