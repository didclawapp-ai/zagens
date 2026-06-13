//! Column border ownership — shared vertical edges are painted once, full height.

use ratatui::buffer::Buffer;
use ratatui::style::Style;
use ratatui::widgets::Borders;

use super::super::layout::LayoutRegions;
use super::super::theme::{self, TuiPanel};
use super::pane::clear_rect;

/// Edges drawn by an individual pane (`Block`). Shared column rules are excluded.
#[derive(Debug, Clone, Copy)]
pub struct BorderPlan {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl BorderPlan {
    pub fn none() -> Self {
        Self {
            top: false,
            bottom: false,
            left: false,
            right: false,
        }
    }

    pub fn all() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: true,
        }
    }

    /// Resolve pane chrome for the active theme.
    pub fn for_left_rail() -> Self {
        if super::super::theme::current().borderless() {
            Self::none()
        } else {
            Self::left_rail()
        }
    }

    pub fn for_right_rail() -> Self {
        if super::super::theme::current().borderless() {
            Self::none()
        } else {
            Self::right_rail()
        }
    }

    pub fn for_center_section() -> Self {
        if super::super::theme::current().borderless() {
            Self::none()
        } else {
            Self::center_section()
        }
    }

    /// Left rail: outer-left + top/bottom; right edge is [`paint_column_dividers`].
    pub fn left_rail() -> Self {
        Self {
            top: true,
            bottom: true,
            left: true,
            right: false,
        }
    }

    /// Right-rail pane: outer-right + top/bottom; left edge is [`paint_column_dividers`].
    pub fn right_rail() -> Self {
        Self {
            top: true,
            bottom: true,
            left: false,
            right: true,
        }
    }

    /// Stacked center sections (transcript / activity / composer / status).
    pub fn center_section() -> Self {
        Self {
            top: true,
            bottom: true,
            left: false,
            right: false,
        }
    }

    pub fn into_borders(self) -> Borders {
        let mut borders = Borders::empty();
        if self.top {
            borders |= Borders::TOP;
        }
        if self.bottom {
            borders |= Borders::BOTTOM;
        }
        if self.left {
            borders |= Borders::LEFT;
        }
        if self.right {
            borders |= Borders::RIGHT;
        }
        borders
    }
}

impl From<BorderPlan> for Borders {
    fn from(plan: BorderPlan) -> Self {
        plan.into_borders()
    }
}

pub struct DividerStyles {
    pub left_center: Style,
    pub center_right: Style,
    pub center_outer: Style,
}

/// Paint full-height vertical rules on sidebar-owned columns (not inside center text).
pub fn paint_column_dividers(buf: &mut Buffer, regions: &LayoutRegions, styles: &DividerStyles) {
    paint_left_center_rule(buf, regions, styles);
    paint_center_right_rule(buf, regions, styles);
}

/// Borderless layout: fill each column with its panel surface before sub-panes render.
/// Mirrors CodeWhale `ui.rs` body backstop (#400) — prevents stale diff cells at seams.
pub fn paint_column_backgrounds(buf: &mut Buffer, regions: &LayoutRegions) {
    if !theme::current().borderless() {
        return;
    }
    let gutter_fill = theme::panel(TuiPanel::Transcript).surface(false);
    if regions.left_visible && regions.left.width > 0 && regions.left.height > 0 {
        clear_rect(
            buf,
            regions.left,
            theme::panel(TuiPanel::Left).surface(false),
        );
    }
    if regions.gutter_left.width > 0 && regions.gutter_left.height > 0 {
        clear_rect(buf, regions.gutter_left, gutter_fill);
    }
    if regions.center.width > 0 && regions.center.height > 0 {
        clear_rect(
            buf,
            regions.center,
            theme::panel(TuiPanel::Transcript).surface(false),
        );
    }
    if regions.gutter_right.width > 0 && regions.gutter_right.height > 0 {
        clear_rect(buf, regions.gutter_right, gutter_fill);
    }
    if regions.right_visible && regions.right.width > 0 && regions.right.height > 0 {
        clear_rect(
            buf,
            regions.right,
            theme::panel(TuiPanel::Inspector).surface(false),
        );
    }
}

/// Re-stamp gutter columns after the center stack draws (keeps seam black).
pub fn seal_panel_edge_columns(buf: &mut Buffer, regions: &LayoutRegions) {
    if !theme::current().borderless() {
        return;
    }
    let gutter_fill = theme::panel(TuiPanel::Transcript).surface(false);

    if regions.gutter_left.width > 0 && regions.gutter_left.height > 0 {
        clear_rect(buf, regions.gutter_left, gutter_fill);
    }
    if regions.gutter_right.width > 0 && regions.gutter_right.height > 0 {
        clear_rect(buf, regions.gutter_right, gutter_fill);
    }
}

fn paint_left_center_rule(buf: &mut Buffer, regions: &LayoutRegions, styles: &DividerStyles) {
    if regions.left_visible && regions.left.width > 0 && regions.left.height > 0 {
        let x = regions
            .left
            .x
            .saturating_add(regions.left.width.saturating_sub(1));
        for row in 0..regions.left.height {
            buf[(x, regions.left.y + row)]
                .set_symbol("│")
                .set_style(styles.left_center);
        }
        return;
    }
    if regions.center.width == 0 || regions.center.height == 0 {
        return;
    }
    let x = regions.center.x;
    for row in 0..regions.center.height {
        buf[(x, regions.center.y + row)]
            .set_symbol("│")
            .set_style(styles.center_outer);
    }
}

fn paint_center_right_rule(buf: &mut Buffer, regions: &LayoutRegions, styles: &DividerStyles) {
    if regions.right_visible && regions.right.width > 0 && regions.right.height > 0 {
        let x = regions.right.x;
        for row in 0..regions.right.height {
            buf[(x, regions.right.y + row)]
                .set_symbol("│")
                .set_style(styles.center_right);
        }
        return;
    }
    if regions.center.width < 2 || regions.center.height == 0 {
        return;
    }
    let x = regions
        .center
        .x
        .saturating_add(regions.center.width.saturating_sub(1));
    for row in 0..regions.center.height {
        buf[(x, regions.center.y + row)]
            .set_symbol("│")
            .set_style(styles.center_outer);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use super::*;

    #[test]
    fn left_center_rule_uses_left_rail_right_edge() {
        let regions = LayoutRegions {
            title: Rect::new(0, 0, 120, 1),
            left: Rect::new(0, 1, 28, 20),
            gutter_left: Rect::new(28, 1, 1, 20),
            center: Rect::new(29, 1, 59, 20),
            gutter_right: Rect::new(88, 1, 1, 20),
            right: Rect::new(89, 1, 31, 20),
            left_visible: true,
            right_visible: true,
        };
        let mut buf = Buffer::empty(Rect::new(0, 0, 120, 24));
        let style = Style::default();
        paint_column_dividers(
            &mut buf,
            &regions,
            &DividerStyles {
                left_center: style,
                center_right: style,
                center_outer: style,
            },
        );
        assert_eq!(buf[(27, 1)].symbol(), "│");
        assert_eq!(buf[(89, 1)].symbol(), "│");
    }
}
