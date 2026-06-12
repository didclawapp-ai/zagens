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
const RIGHT_MIN: u16 = 28;

/// Status chips row inside the Composer block (model, mode, task type, …).
pub const COMPOSER_FOOTER_ROWS: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Files,
    Diff,
    Checklist,
    Agents,
    Mcp,
}

impl InspectorTab {
    pub const ALL: [Self; 5] = [
        Self::Files,
        Self::Diff,
        Self::Checklist,
        Self::Agents,
        Self::Mcp,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Diff => "Diff",
            Self::Checklist => "Checklist",
            Self::Agents => "Agents",
            Self::Mcp => "MCP",
        }
    }

    pub fn from_index(n: u8) -> Option<Self> {
        match n {
            1 => Some(Self::Files),
            2 => Some(Self::Diff),
            3 => Some(Self::Checklist),
            4 => Some(Self::Agents),
            5 => Some(Self::Mcp),
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
            "checklist" => InspectorTab::Checklist,
            "agents" => InspectorTab::Agents,
            "mcp" => InspectorTab::Mcp,
            _ => InspectorTab::Files,
        }
    }

    pub fn set_inspector_tab(&mut self, tab: InspectorTab) {
        self.active_inspector = match tab {
            InspectorTab::Files => "files",
            InspectorTab::Diff => "diff",
            InspectorTab::Checklist => "checklist",
            InspectorTab::Agents => "agents",
            InspectorTab::Mcp => "mcp",
        }
        .to_string();
    }
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
}

impl LayoutEngine {
    pub fn new(inline_mode: bool, prefs: TuiLayoutPrefs) -> Self {
        Self {
            prefs,
            inline_mode,
            focus: FocusRegion::Chat,
            composer_lines: if inline_mode { 6 } else { 9 },
        }
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
            column_width(area.width, 0.22, LEFT_MIN, LEFT_MAX)
        } else {
            0
        };
        let right_w = if right_visible {
            column_width(area.width, 0.30, RIGHT_MIN, RIGHT_MAX)
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
}
