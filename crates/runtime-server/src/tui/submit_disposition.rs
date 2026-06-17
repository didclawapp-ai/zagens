//! How Enter/Ctrl+Enter routes composer text while a turn is live.

use super::transcript::TranscriptState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitDisposition {
    /// Engine idle — start a new turn.
    Immediate,
    /// Park until the current turn finishes (active assistant streaming).
    Queue,
    /// Inject into the in-flight turn (tool wait, thinking gap, or Ctrl+Enter).
    Steer,
}

/// Mirrors CodeWhale `decide_submit_disposition` (v0.8.44+).
///
/// - Idle → immediate send
/// - Busy + assistant content streaming → queue (avoid interrupting reasoning)
/// - Busy + waiting (tools / gaps) → steer; Ctrl+Enter forces steer while streaming
pub fn decide(transcript: &TranscriptState, force_steer: bool) -> SubmitDisposition {
    if !transcript.is_live_activity() {
        return SubmitDisposition::Immediate;
    }
    if force_steer {
        return SubmitDisposition::Steer;
    }
    if transcript.is_assistant_content_streaming() {
        return SubmitDisposition::Queue;
    }
    SubmitDisposition::Steer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::transcript::{TranscriptState, apply_event};
    use zagens_core::events::Event;

    #[test]
    fn idle_is_immediate() {
        let t = TranscriptState::default();
        assert_eq!(decide(&t, false), SubmitDisposition::Immediate);
    }

    #[test]
    fn streaming_content_queues_plain_enter() {
        let mut t = TranscriptState::default();
        t.begin_turn("hi".into());
        apply_event(&mut t, Event::MessageStarted { index: 0 });
        assert!(t.is_assistant_content_streaming());
        assert_eq!(decide(&t, false), SubmitDisposition::Queue);
    }

    #[test]
    fn streaming_content_steer_on_ctrl_enter() {
        let mut t = TranscriptState::default();
        t.begin_turn("hi".into());
        apply_event(&mut t, Event::MessageStarted { index: 0 });
        assert_eq!(decide(&t, true), SubmitDisposition::Steer);
    }

    #[test]
    fn tool_wait_steer_on_enter() {
        let mut t = TranscriptState::default();
        t.begin_turn("hi".into());
        apply_event(
            &mut t,
            Event::ToolCallStarted {
                id: "t1".into(),
                name: "read_file".into(),
                input: serde_json::json!({}),
            },
        );
        assert_eq!(decide(&t, false), SubmitDisposition::Steer);
    }
}
