//! One-row activity indicator between Transcript and Composer while the model is active.
//!
//! Design: braille spinner (1 char, rotates) · phase label · trailing dim ─ fill.
//! Only the spinner character animates; the rest of the row is stable, keeping the
//! eye on the label rather than the entire width scrolling.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::display_format::{display_width, truncate_display_width};
use super::theme::{self, ActivityPhase, TuiPanel};

/// Braille "clock" spinner — 10 frames, each 1 cell wide.
const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// How many milliseconds per spinner frame.
const SPINNER_MS: u64 = 80;
/// Dim separator character used to fill the trailing space.
const TRAIL_CHAR: &str = "─";

pub fn render_activity_strip(
    state: &super::transcript::TranscriptState,
    width: u16,
) -> Line<'static> {
    let label = state.activity_banner_label();
    let since = state.activity_anim_since();
    let tick = since.elapsed().as_millis() as u64 / SPINNER_MS;
    let phase = activity_phase(state);
    build_strip_line(&label, width as usize, tick, phase)
}

fn activity_phase(state: &super::transcript::TranscriptState) -> ActivityPhase {
    if state.is_thinking() {
        ActivityPhase::Thinking
    } else if state.is_tools_active() {
        ActivityPhase::Tools
    } else if state.streaming {
        ActivityPhase::Streaming
    } else {
        ActivityPhase::Other
    }
}

/// Build the activity strip as a multi-span Line:
///   [spinner] [space] [label] [trailing ─ fill]
///
/// Only the spinner frame index changes each tick, so the label is stable.
fn build_strip_line(label: &str, width: usize, tick: u64, phase: ActivityPhase) -> Line<'static> {
    let width = width.max(8);

    let spinner = SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()];
    // " ⠹ " — 1 leading space + 1 spinner char + 1 trailing space = 3 display cols
    let prefix = format!(" {spinner} ");
    let prefix_w = 3usize;

    // Reserve 1 col minimum for trailing fill, so the right edge is always visible.
    let label_max = width.saturating_sub(prefix_w + 2);
    let label_trimmed = truncate_display_width(label, label_max).to_string();
    let label_w = display_width(&label_trimmed);

    // Trailing fill: " " + one or more TRAIL_CHAR up to the right edge.
    let fill_start_w = prefix_w + label_w + 1; // +1 for leading space before fill
    let fill_char_w = width.saturating_sub(fill_start_w);
    let trail = if fill_char_w > 0 {
        format!(" {}", TRAIL_CHAR.repeat(fill_char_w))
    } else {
        String::new()
    };

    let phase_color = theme::activity_phase_color(phase);
    let surface = theme::panel(TuiPanel::Activity).surface(false);
    let bg = surface.bg.unwrap_or(ratatui::style::Color::Reset);
    let dim_color = theme::panel(TuiPanel::Activity)
        .hint()
        .fg
        .unwrap_or(ratatui::style::Color::DarkGray);

    Line::from(vec![
        // Spinner: bold + phase color → catches the eye without noisy motion
        Span::styled(
            prefix,
            Style::default()
                .fg(phase_color)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        ),
        // Label: same phase color, normal weight
        Span::styled(label_trimmed, Style::default().fg(phase_color).bg(bg)),
        // Trailing ─ fill: dim, gives the row visual weight without scrolling
        Span::styled(trail, Style::default().fg(dim_color).bg(bg)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::Event;
    use crate::tui::transcript::{TranscriptState, apply_event};

    fn strip_plain(label: &str, width: usize, tick: u64) -> String {
        let line = build_strip_line(label, width, tick, ActivityPhase::Thinking);
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn strip_fits_width() {
        crate::tui::theme::install(crate::tui::theme::TuiTheme::default_theme());
        let text = strip_plain("推理中 · THK", 40, 0);
        assert!(display_width(&text) <= 40, "width={}", display_width(&text));
    }

    #[test]
    fn strip_fits_narrow_width() {
        crate::tui::theme::install(crate::tui::theme::TuiTheme::default_theme());
        let text = strip_plain("生成回复 · AI", 18, 0);
        assert!(display_width(&text) <= 18, "width={}", display_width(&text));
    }

    #[test]
    fn spinner_rotates_between_ticks() {
        crate::tui::theme::install(crate::tui::theme::TuiTheme::default_theme());
        let a = strip_plain("tools", 30, 0);
        let b = strip_plain("tools", 30, 3);
        // Spinner char changes → leading prefix differs.
        assert_ne!(&a[..4], &b[..4]);
    }

    #[test]
    fn strip_contains_no_marquee_chars() {
        crate::tui::theme::install(crate::tui::theme::TuiTheme::default_theme());
        let text = strip_plain("生成回复 · AI", 48, 0);
        assert!(!text.contains('}'), "old marquee tile should not appear");
        assert!(!text.contains('●'), "old dot style should not appear");
    }

    #[test]
    fn strip_label_reflects_thinking() {
        crate::tui::theme::install(crate::tui::theme::TuiTheme::default_theme());
        let mut state = TranscriptState::default();
        state.begin_turn("test".into());
        apply_event(
            &mut state,
            Event::ThinkingDelta {
                index: 0,
                content: "plan".to_string(),
            },
        );
        assert!(state.activity_banner_label().contains("THK"));
    }

    #[test]
    fn anim_anchor_stable_during_streaming() {
        let mut state = TranscriptState::default();
        state.begin_turn("test".into());
        state.streaming = true;
        let t0 = state.activity_anim_since();
        let t1 = state.activity_anim_since();
        assert_eq!(t0, t1);
    }
}
