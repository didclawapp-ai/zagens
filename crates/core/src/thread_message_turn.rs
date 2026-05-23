//! Thread `Message` turn port (P2 PR5).
//!
//! `Runtime::handle_thread(ThreadRequest::Message)` delegates here when a port
//! is installed; otherwise the legacy `"queued"` placeholder is returned.

use std::path::PathBuf;

use async_trait::async_trait;
use deepseek_protocol::EventFrame;

/// Inputs for a single user message turn on an existing thread.
#[derive(Debug, Clone)]
pub struct ThreadMessageTurnRequest {
    pub thread_id: String,
    pub input: String,
    pub cwd: PathBuf,
    pub model: String,
}

/// Outcome of a delegated thread message turn.
#[derive(Debug, Clone)]
pub struct ThreadMessageTurnResult {
    pub status: String,
    pub assistant_text: String,
}

/// Executes a real turn for `ThreadRequest::Message` (app-server / shared core path).
#[async_trait]
pub trait ThreadMessageTurnPort: Send + Sync {
    async fn run_turn(
        &self,
        req: ThreadMessageTurnRequest,
    ) -> anyhow::Result<ThreadMessageTurnResult>;
}

/// Build protocol events for a completed assistant reply.
pub fn thread_message_turn_events(response_id: &str, assistant_text: &str) -> Vec<EventFrame> {
    vec![
        EventFrame::ResponseStart {
            response_id: response_id.to_string(),
        },
        EventFrame::ResponseDelta {
            response_id: response_id.to_string(),
            delta: assistant_text.to_string(),
        },
        EventFrame::ResponseEnd {
            response_id: response_id.to_string(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_events_include_assistant_delta() {
        let events = thread_message_turn_events("resp-1", "hello");
        assert_eq!(events.len(), 3);
        assert!(matches!(
            &events[1],
            EventFrame::ResponseDelta { delta, .. } if delta == "hello"
        ));
    }
}
