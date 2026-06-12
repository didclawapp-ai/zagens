//! Three-column layout constraints and persisted fold state.

use std::fs;
use std::path::PathBuf;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use serde::{Deserialize, Serialize};

use super::focus::FocusRegion;

const MIN_CENTER_COLS: u16 = 40;
const LEFT_MAX: u16 = 32;
const LEFT_MIN: u16 = 24;
const RIGHT_MAX: u16 = 40;
const RIGHT_MIN: u16 = 20;
/// User-resizable upper bound (still capped by terminal width − center min).
const RIGHT_ABS_MAX: u16 = 72;
/// Columns added/removed per `-` / `=` press while the right rail is focused.
pub const RIGHT_WIDTH_STEP: i16 = 2;
const LEFT_RATIO: f32 = 0.22;
const RIGHT_RATIO: f32 = 0.30;

/// Status chips row inside the Composer block (model, mode, task type, …).
pub const COMPOSER_FOOTER_ROWS: u16 = 1;

/// Faint horizontal rules above and below the composer footer chips.
pub const COMPOSER_FOOTER_DIVIDER_ROWS: u16 = 1;

/// Blank columns inset from center column side borders before text (each side).
pub const CENTER_CONTENT_PAD: u16 = 1;

/// Minimum rows for the upper inspector pane when LHT lower pane is visible.
pub const RIGHT_INSPECTOR_MIN_ROWS: u16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Files,
    Diff,
    Agents,
    Mcp,
}

impl InspectorTab {
    pub const ALL: [Self; 4] = [Self::Files, Self::Diff, Self::Agents, Self::Mcp];

    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Diff => "Diff",
            Self::Agents => "Agents",
            Self::Mcp => "MCP",
        }
    }

    pub fn from_index(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Files),
            2 => Some(Self::Diff),
            3 => Some(Self::Agents),
            4 => Some(Self::Mcp),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiLayoutPrefs {
    #[serde(default)]
    pub left_collapsed: bool,
    #[serde(default)]
    pub right_collapsed: bool,
    #[serde(default = "default_inspector")]
    pub active_inspector: String,
    /// Last focused thread in TUI; restored on launch (unless `--fresh`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_thread_id: Option<String>,
    /// Right rail width in terminal columns (`None` = 30% default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_width: Option<u16>,
}

fn default_inspector() -> String {
    "files".to_string()
}

impl Default for TuiLayoutPrefs {
    fn default() -> Self {
        Self {
            left_collapsed: false,
            right_collapsed: false,
            active_inspector: default_inspector(),
            last_thread_id: None,
            right_width: None,
        }
    }
}

impl TuiLayoutPrefs {
    pub fn path() -> Option<PathBuf> {
        zagens_config::user_data_path("tui-layout.toml").ok()
    }

    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(raw) = fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let body = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        fs::write(path, body)
    }

    pub fn inspector_tab(&self) -> InspectorTab {
        match self.active_inspector.as_str() {
            "diff" => InspectorTab::Diff,
            "agents" => InspectorTab::Agents,
            "mcp" => InspectorTab::Mcp,
            "checklist" => InspectorTab::Files,
            _ => InspectorTab::Files,
        }
    }

    pub fn set_inspector_tab(&mut self, tab: InspectorTab) {
        self.active_inspector = match tab {
            InspectorTab::Files => "files",
            InspectorTab::Diff => "diff",
            InspectorTab::Agents => "agents",
            InspectorTab::Mcp => "mcp",
        }
        .to_string();
    }
}

#[derive(Debug, Clone)]
pub struct RightPaneRegions {
    pub inspector: Rect,
    pub lht: Rect,
    pub lht_visible: bool,
}

#[derive(Debug, Clone)]
pub struct LayoutRegions {
    pub title: Rect,
    pub left: Rect,
    pub center: Rect,
    pub right: Rect,
    pub left_visible: bool,
    pub right_visible: bool,
}

pub struct LayoutEngine {
    pub prefs: TuiLayoutPrefs,
    pub inline_mode: bool,
    pub focus: FocusRegion,
    pub composer_lines: u16,
    /// Updated each frame for width adjustment clamping.
    pub last_terminal_width: u16,
}

impl LayoutEngine {
    pub fn new(inline_mode: bool, prefs: TuiLayoutPrefs) -> Self {
        Self {
            prefs,
            inline_mode,
            focus: FocusRegion::Chat,
            composer_lines: if inline_mode { 6 } else { 9 },
            last_terminal_width: 120,
        }
    }

    /// Narrow/widen the right rail when focused (`-` / `=`). Persists via `prefs.right_width`.
    pub fn adjust_right_width(&mut self, delta: i16) -> bool {
        if !self.right_rail_available() {
            return false;
        }
        let delta = if delta < 0 {
            -RIGHT_WIDTH_STEP
        } else {
            RIGHT_WIDTH_STEP
        };
        let total = self
            .last_terminal_width
            .max(MIN_CENTER_COLS + RIGHT_MIN + 4);
        let left_w = self.effective_left_width(total);
        let current = self.effective_right_width(total, left_w);
        let bounds = right_width_bounds(total, left_w, self.left_rail_available());
        let next = (i32::from(current) + i32::from(delta))
            .clamp(i32::from(RIGHT_MIN), i32::from(bounds)) as u16;
        if next == current {
            return false;
        }
        self.prefs.right_width = Some(next);
        true
    }

    fn effective_left_width(&self, total: u16) -> u16 {
        if !self.left_rail_available() {
            return 0;
        }
        column_width(total, LEFT_RATIO, LEFT_MIN, LEFT_MAX)
    }

    fn effective_right_width(&self, total: u16, left_w: u16) -> u16 {
        if !self.right_rail_available() {
            return 0;
        }
        let bounds = right_width_bounds(total, left_w, self.left_rail_available());
        if let Some(stored) = self.prefs.right_width {
            return stored.clamp(RIGHT_MIN, bounds);
        }
        column_width(total, RIGHT_RATIO, RIGHT_MIN, RIGHT_MAX.min(bounds))
    }

    pub fn toggle_left(&mut self) {
        self.prefs.left_collapsed = !self.prefs.left_collapsed;
    }

    pub fn toggle_right(&mut self) {
        self.prefs.right_collapsed = !self.prefs.right_collapsed;
    }

    pub fn left_rail_available(&self) -> bool {
        !self.inline_mode && !self.prefs.left_collapsed
    }

    pub fn right_rail_available(&self) -> bool {
        !self.inline_mode && !self.prefs.right_collapsed
    }

    pub fn focus_next_visible(&self) -> FocusRegion {
        self.step_focus_visible(true)
    }

    pub fn focus_prev_visible(&self) -> FocusRegion {
        self.step_focus_visible(false)
    }

    fn step_focus_visible(&self, forward: bool) -> FocusRegion {
        let mut region = self.focus;
        for _ in 0..3 {
            region = if forward {
                region.next()
            } else {
                region.prev()
            };
            if self.is_focus_region_visible(region) {
                return region;
            }
        }
        FocusRegion::Chat
    }

    fn is_focus_region_visible(&self, region: FocusRegion) -> bool {
        match region {
            FocusRegion::Left => self.left_rail_available(),
            FocusRegion::Chat => true,
            FocusRegion::Right => self.right_rail_available(),
        }
    }

    pub fn apply_auto_collapse(&mut self, width: u16) {
        if self.inline_mode {
            self.prefs.left_collapsed = true;
            self.prefs.right_collapsed = true;
            return;
        }
        if width < 100 {
            self.prefs.left_collapsed = true;
            self.prefs.right_collapsed = true;
        } else if width < 120 {
            self.prefs.right_collapsed = true;
        }
    }

    pub fn regions(&self, area: Rect) -> LayoutRegions {
        let title_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);
        let body = title_rows[1];

        if self.inline_mode {
            return LayoutRegions {
                title: title_rows[0],
                left: Rect::default(),
                center: body,
                right: Rect::default(),
                left_visible: false,
                right_visible: false,
            };
        }

        let left_visible = !self.prefs.left_collapsed;
        let right_visible = !self.prefs.right_collapsed;

        let left_w = if left_visible {
            self.effective_left_width(area.width)
        } else {
            0
        };
        let right_w = if right_visible {
            self.effective_right_width(area.width, left_w)
        } else {
            0
        };
        let center_w = area
            .width
            .saturating_sub(left_w)
            .saturating_sub(right_w)
            .saturating_sub(4);

        let (left_w, right_w, left_visible, right_visible) = if center_w < MIN_CENTER_COLS {
            if left_visible && right_visible && area.width >= 100 {
                (left_w, 0, true, false)
            } else {
                (0, 0, false, false)
            }
        } else {
            (left_w, right_w, left_visible, right_visible)
        };

        let mut h_constraints = Vec::new();
        if left_visible {
            h_constraints.push(Constraint::Length(left_w));
        }
        h_constraints.push(Constraint::Min(MIN_CENTER_COLS));
        if right_visible {
            h_constraints.push(Constraint::Length(right_w));
        }

        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(h_constraints)
            .split(body);

        let mut idx = 0usize;
        let left = if left_visible {
            let r = cols[idx];
            idx += 1;
            r
        } else {
            Rect::default()
        };
        let center = {
            let r = cols[idx];
            idx += 1;
            r
        };
        let right = if right_visible {
            cols.get(idx).copied().unwrap_or_default()
        } else {
            Rect::default()
        };

        LayoutRegions {
            title: title_rows[0],
            left,
            center,
            right,
            left_visible,
            right_visible,
        }
    }

    /// Split the right rail into upper inspector + optional lower LHT pane.
    pub fn split_right_pane(&self, right: Rect, lht_visible: bool) -> RightPaneRegions {
        if !lht_visible || right.height < RIGHT_INSPECTOR_MIN_ROWS + 6 {
            return RightPaneRegions {
                inspector: right,
                lht: Rect::default(),
                lht_visible: false,
            };
        }
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(RIGHT_INSPECTOR_MIN_ROWS),
                Constraint::Min(6),
            ])
            .split(right);
        RightPaneRegions {
            inspector: rows[0],
            lht: rows[1],
            lht_visible: true,
        }
    }

    pub fn center_panes(&self, center: Rect) -> (Rect, Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(8), Constraint::Length(self.composer_lines)])
            .split(center);
        (rows[0], rows[1])
    }
}

fn column_width(total: u16, ratio: f32, min: u16, max: u16) -> u16 {
    let w = ((total as f32) * ratio).round() as u16;
    w.clamp(min, max)
        .min(total.saturating_sub(MIN_CENTER_COLS + 4))
}

fn right_width_bounds(total: u16, left_w: u16, left_visible: bool) -> u16 {
    let left = if left_visible { left_w } else { 0 };
    total
        .saturating_sub(MIN_CENTER_COLS)
        .saturating_sub(left)
        .saturating_sub(2)
        .clamp(RIGHT_MIN, RIGHT_ABS_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_width_hides_sidebars() {
        let engine = LayoutEngine::new(false, TuiLayoutPrefs::default());
        let regions = engine.regions(Rect::new(0, 0, 80, 24));
        assert!(!regions.left_visible);
        assert!(!regions.right_visible);
        assert!(regions.center.width >= MIN_CENTER_COLS);
    }

    #[test]
    fn focus_next_skips_collapsed_sidebars() {
        let mut engine = LayoutEngine::new(false, TuiLayoutPrefs::default());
        engine.prefs.left_collapsed = true;
        engine.prefs.right_collapsed = true;
        engine.focus = FocusRegion::Chat;
        assert_eq!(engine.focus_next_visible(), FocusRegion::Chat);
        engine.prefs.right_collapsed = false;
        assert_eq!(engine.focus_next_visible(), FocusRegion::Right);
    }

    #[test]
    fn wide_width_shows_three_columns() {
        let engine = LayoutEngine::new(false, TuiLayoutPrefs::default());
        let regions = engine.regions(Rect::new(0, 0, 140, 40));
        assert!(regions.left_visible);
        assert!(regions.right_visible);
        assert!(regions.center.width >= MIN_CENTER_COLS);
    }

    #[test]
    fn split_right_pane_reserves_lht_section() {
        let engine = LayoutEngine::new(false, TuiLayoutPrefs::default());
        let right = Rect::new(0, 0, 32, 30);
        let split = engine.split_right_pane(right, true);
        assert!(split.lht_visible);
        assert!(split.inspector.height + split.lht.height <= right.height);
    }

    #[test]
    fn adjust_right_width_persists_and_clamps() {
        let mut engine = LayoutEngine::new(false, TuiLayoutPrefs::default());
        engine.last_terminal_width = 160;
        assert!(engine.adjust_right_width(1));
        assert_eq!(engine.prefs.right_width, Some(42));
        let bounds = right_width_bounds(160, engine.effective_left_width(160), true);
        engine.prefs.right_width = Some(bounds.saturating_sub(2));
        assert!(engine.adjust_right_width(1));
        assert_eq!(engine.prefs.right_width, Some(bounds));
        assert!(!engine.adjust_right_width(1));
    }

    #[test]
    fn stored_right_width_applied_in_regions() {
        let mut prefs = TuiLayoutPrefs::default();
        prefs.right_width = Some(50);
        let mut engine = LayoutEngine::new(false, prefs);
        engine.last_terminal_width = 160;
        let regions = engine.regions(Rect::new(0, 0, 160, 40));
        assert!(regions.right_visible);
        assert_eq!(regions.right.width, 50);
    }
}
