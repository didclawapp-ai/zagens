//! Dracula semantic tokens (shared across all layout themes).

use ratatui::style::Color;

// §6.10.1 核心语义
pub const USER_PROMPT: &str = "#8be9fd";
pub const USER_TEXT: &str = "#f8f8f2";
pub const AGENT_REPLY: &str = "#50fa7b";
pub const THINKING: &str = "#f1fa8c";
pub const WARNING: &str = "#ffb86c";
pub const ERROR: &str = "#ff5555";
pub const TOOL_CALL: &str = "#bd93f9";
pub const DIM: &str = "#6272a4";

// §6.10.2 侧边栏 (legacy hex — surfaces override panel bg at runtime)
pub const SIDEBAR_ACTIVE: &str = "#222222";

// §6.10.3 Checklist
pub const CHECKLIST_DONE: &str = "#50fa7b";
pub const CHECKLIST_IN_PROGRESS: &str = "#f1fa8c";
pub const CHECKLIST_PENDING: &str = "#6272a4";
pub const PROGRESS_FILL: &str = "#50fa7b";

// §6.10.4 背景 / 表面
pub const BG: &str = "#000000";
pub const FOREGROUND: &str = "#f8f8f2";
pub const CODE_BG: &str = "#1e1e1e";
pub const TAG_BG: &str = "#141414";
pub const BORDER_IDLE: &str = "#555555";
pub const BORDER_FOCUS: &str = "#777777";

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

pub fn user_prompt() -> Color {
    rgb(0x8b, 0xe9, 0xfd)
}
pub fn user_text() -> Color {
    rgb(0xf8, 0xf8, 0xf2)
}
pub fn agent_reply() -> Color {
    rgb(0x50, 0xfa, 0x7b)
}
pub fn thinking() -> Color {
    rgb(0xf1, 0xfa, 0x8c)
}
pub fn warning() -> Color {
    rgb(0xff, 0xb8, 0x6c)
}
pub fn error() -> Color {
    rgb(0xff, 0x55, 0x55)
}
pub fn tool_call() -> Color {
    rgb(0xbd, 0x93, 0xf9)
}
pub fn dim() -> Color {
    rgb(0x62, 0x72, 0xa4)
}
pub fn item_text() -> Color {
    rgb(0xf8, 0xf8, 0xf2)
}
pub fn bg() -> Color {
    rgb(0x00, 0x00, 0x00)
}
pub fn foreground() -> Color {
    rgb(0xf8, 0xf8, 0xf2)
}
pub fn code_bg() -> Color {
    rgb(0x1e, 0x1e, 0x1e)
}
pub fn tag_bg() -> Color {
    rgb(0x14, 0x14, 0x14)
}
pub fn border_idle() -> Color {
    rgb(0x55, 0x55, 0x55)
}
pub fn border_focus() -> Color {
    rgb(0x77, 0x77, 0x77)
}
