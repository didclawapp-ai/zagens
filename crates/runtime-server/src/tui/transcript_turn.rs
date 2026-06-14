//! One desktop-style conversation turn (user prompt + assistant bundle).

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TurnThinking {
    pub text: String,
    pub char_count: usize,
    pub streaming: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TurnTool {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub detail: String,
    pub expanded: bool,
    pub done: bool,
    pub success: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptTurn {
    pub user: String,
    pub thinking: TurnThinking,
    pub tools: Vec<TurnTool>,
    pub content: String,
    pub content_streaming: bool,
    pub harness: Vec<String>,
    pub open: bool,
    /// Collapsed tool-chain summary for this turn (desktop MessageMetaBar default).
    pub tools_collapsed: bool,
    /// Collapsed harness log (subagent / CRAFT / gate events) — detail in Activity tab.
    pub harness_collapsed: bool,
    /// Show full THK body instead of preview when not streaming.
    pub thinking_expanded: bool,
}

impl TranscriptTurn {
    pub fn new(user: String) -> Self {
        Self {
            user,
            thinking: TurnThinking::default(),
            tools: Vec::new(),
            content: String::new(),
            content_streaming: false,
            harness: Vec::new(),
            open: true,
            tools_collapsed: true,
            harness_collapsed: true,
            thinking_expanded: false,
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.content_streaming = false;
        self.thinking.streaming = false;
    }

    pub fn has_pending_tools(&self) -> bool {
        self.tools.iter().any(|t| !t.done)
    }
}
