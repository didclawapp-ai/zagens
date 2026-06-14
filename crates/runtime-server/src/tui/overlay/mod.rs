//! Full-screen overlays (approval, help, automation).

mod approval;
mod automation;
mod help;

pub use approval::{PendingApproval, draw_approval};
pub use automation::{AutomationUiState, draw_automation};
pub use help::draw_help;
