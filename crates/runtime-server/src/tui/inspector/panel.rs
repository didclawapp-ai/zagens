//! Right-rail scroll and tab-local interaction state.

#[derive(Debug, Clone, Default)]
pub struct InspectorInteraction {
    pub scroll: usize,
    pub file_cursor: usize,
    pub diff_staged: bool,
    pub diff_cursor: usize,
    /// Relative path when viewing a file preview (Files tab).
    pub file_preview_rel: Option<String>,
    /// Relative path when viewing a diff patch (Diff tab).
    pub diff_detail_path: Option<String>,
    /// Cached preview/diff body lines (populated on Enter into detail).
    pub file_preview_body: Option<Vec<String>>,
    pub diff_detail_body: Option<Vec<String>>,
    pub agents_cursor: usize,
    /// Index of the agent row currently expanded to show full status detail.
    pub agents_expanded: Option<usize>,
    pub mcp_cursor: usize,
    /// Expanded MCP server name.
    pub mcp_expanded: Option<String>,
}

impl InspectorInteraction {
    pub fn reset_for_tab(&mut self) {
        self.scroll = 0;
        self.file_cursor = 0;
        self.diff_cursor = 0;
        self.agents_cursor = 0;
        self.agents_expanded = None;
        self.mcp_cursor = 0;
        self.file_preview_rel = None;
        self.diff_detail_path = None;
        self.file_preview_body = None;
        self.diff_detail_body = None;
        self.mcp_expanded = None;
    }

    pub fn in_detail_view(&self) -> bool {
        self.file_preview_rel.is_some() || self.diff_detail_path.is_some()
    }

    pub fn clear_detail_views(&mut self) {
        self.file_preview_rel = None;
        self.diff_detail_path = None;
        self.file_preview_body = None;
        self.diff_detail_body = None;
        self.scroll = 0;
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, max_scroll: usize, n: usize) {
        self.scroll = (self.scroll + n).min(max_scroll);
    }

    pub fn file_move_up(&mut self) {
        self.file_cursor = self.file_cursor.saturating_sub(1);
        Self::ensure_file_cursor_visible(self);
    }

    pub fn file_move_down(&mut self, line_count: usize, visible: usize) {
        if line_count == 0 {
            self.file_cursor = 0;
            return;
        }
        self.file_cursor = (self.file_cursor + 1).min(line_count - 1);
        Self::ensure_file_cursor_visible_with(self, visible);
    }

    pub fn diff_move_up(&mut self) {
        self.diff_cursor = self.diff_cursor.saturating_sub(1);
        if self.diff_cursor < self.scroll {
            self.scroll = self.diff_cursor;
        }
    }

    pub fn diff_move_down(&mut self, line_count: usize, visible: usize) {
        Self::list_move_down_on(&mut self.diff_cursor, &mut self.scroll, line_count, visible);
    }

    pub fn agents_move_up(&mut self) {
        self.agents_cursor = self.agents_cursor.saturating_sub(1);
        if self.agents_cursor < self.scroll {
            self.scroll = self.agents_cursor;
        }
    }

    pub fn agents_move_down(&mut self, line_count: usize, visible: usize) {
        Self::list_move_down_on(
            &mut self.agents_cursor,
            &mut self.scroll,
            line_count,
            visible,
        );
    }

    pub fn mcp_move_up(&mut self) {
        self.mcp_cursor = self.mcp_cursor.saturating_sub(1);
        if self.mcp_cursor < self.scroll {
            self.scroll = self.mcp_cursor;
        }
    }

    pub fn mcp_move_down(&mut self, line_count: usize, visible: usize) {
        Self::list_move_down_on(&mut self.mcp_cursor, &mut self.scroll, line_count, visible);
    }

    fn list_move_down_on(
        cursor: &mut usize,
        scroll: &mut usize,
        line_count: usize,
        visible: usize,
    ) {
        if line_count == 0 {
            *cursor = 0;
            *scroll = 0;
            return;
        }
        *cursor = (*cursor + 1).min(line_count - 1);
        if visible == 0 {
            return;
        }
        if *cursor < *scroll {
            *scroll = *cursor;
        } else if *cursor >= *scroll + visible {
            *scroll = *cursor + 1 - visible;
        }
    }

    pub fn clamp_diff_cursor(&mut self, line_count: usize) {
        Self::clamp_cursor(&mut self.diff_cursor, &mut self.scroll, line_count);
    }

    pub fn clamp_agents_cursor(&mut self, line_count: usize) {
        Self::clamp_cursor(&mut self.agents_cursor, &mut self.scroll, line_count);
    }

    pub fn clamp_mcp_cursor(&mut self, line_count: usize) {
        Self::clamp_cursor(&mut self.mcp_cursor, &mut self.scroll, line_count);
    }

    fn clamp_cursor(cursor: &mut usize, scroll: &mut usize, line_count: usize) {
        if line_count == 0 {
            *cursor = 0;
            *scroll = 0;
            return;
        }
        *cursor = (*cursor).min(line_count - 1);
        *scroll = (*scroll).min(line_count.saturating_sub(1));
    }

    fn ensure_file_cursor_visible(state: &mut Self) {
        if state.file_cursor < state.scroll {
            state.scroll = state.file_cursor;
        }
    }

    fn ensure_file_cursor_visible_with(state: &mut Self, visible: usize) {
        if visible == 0 {
            return;
        }
        if state.file_cursor < state.scroll {
            state.scroll = state.file_cursor;
        } else if state.file_cursor >= state.scroll + visible {
            state.scroll = state.file_cursor + 1 - visible;
        }
    }

    pub fn clamp_file_cursor(&mut self, line_count: usize) {
        Self::clamp_cursor(&mut self.file_cursor, &mut self.scroll, line_count);
    }
}

#[derive(Debug, Clone, Default)]
pub struct LhtPaneUi {
    pub scroll: usize,
}

impl LhtPaneUi {
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, max_scroll: usize, n: usize) {
        self.scroll = (self.scroll + n).min(max_scroll);
    }

    pub fn reset(&mut self) {
        self.scroll = 0;
    }
}
