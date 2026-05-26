//! Tool progress streaming to `Event::ToolCallProgress`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::tools::spec::ToolProgressEmit;

use super::super::*;

pub(super) async fn emit_tool_progress(tx: &mpsc::Sender<Event>, tool_call_id: &str, message: &str) {
    let text = message.trim_end_matches('\n');
    if text.is_empty() {
        return;
    }
    let line = format!("{text}\n");
    let _ = tx
        .send(Event::ToolCallProgress {
            id: tool_call_id.to_string(),
            output: line,
        })
        .await;
}

pub(super) struct ChannelToolProgress {
    tx: mpsc::Sender<Event>,
    tool_call_id: String,
    stderr_banner_sent: Arc<AtomicBool>,
}

impl ChannelToolProgress {
    pub(super) fn new_arc(tx: mpsc::Sender<Event>, tool_call_id: String) -> Arc<Self> {
        Arc::new(Self {
            tx,
            tool_call_id,
            stderr_banner_sent: Arc::new(AtomicBool::new(false)),
        })
    }

    fn emit_raw(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        let _ = self.tx.try_send(Event::ToolCallProgress {
            id: self.tool_call_id.clone(),
            output: text.to_string(),
        });
    }
}

impl ToolProgressEmit for ChannelToolProgress {
    fn emit_stdout(&self, chunk: &str) {
        self.emit_raw(chunk);
    }

    fn emit_stderr(&self, chunk: &str) {
        if chunk.is_empty() {
            return;
        }
        let mut buf = String::new();
        if !self.stderr_banner_sent.swap(true, Ordering::SeqCst) {
            buf.push_str("\n--- stderr ---\n");
        }
        buf.push_str(chunk);
        self.emit_raw(&buf);
    }
}
