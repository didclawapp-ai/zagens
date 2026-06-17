//! Full-screen overlays (approval, help, automation).

mod approval;
mod automation;
mod help;
mod onboarding;

use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub use approval::{PendingApproval, draw_approval};
pub use automation::{AutomationUiState, draw_automation};
pub use help::draw_help;
pub use onboarding::{OnboardingUiState, draw_onboarding};

/// Percent-sized popup rect centered in `area`.
pub fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
