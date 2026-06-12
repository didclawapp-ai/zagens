//! One-row marquee between Transcript and Composer while the model is active.

use ratatui::text::Line;

use super::display_format::{display_width, pad_line_display_width, truncate_display_width};
use super::theme::{self, ActivityPhase};
use super::transcript::TranscriptState;

/// Narrow ASCII rail tile (display width 2): `-}`.
const MARQUEE_TILE: [char; 2] = ['-', '}'];
const MARQUEE_MS: u64 = 80;

pub fn render_activity_strip(state: &TranscriptState, width: u16) -> Line<'static> {
    let label = state.activity_banner_label();
    let since = state.activity_anim_since();
    let tick = since.elapsed().as_millis() as u64 / MARQUEE_MS;
    let text = marquee_text(&label, width as usize, tick);
    let phase = activity_phase(state);
    theme::activity_strip_line(&text, phase)
}

fn activity_phase(state: &TranscriptState) -> ActivityPhase {
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

fn marquee_text(label: &str, width: usize, tick: u64) -> String {
    let width = width.max(8);
    let inner_label = truncate_display_width(label, width.saturating_sub(4));
    let center = format!(" {inner_label} ");
    let center_w = display_width(&center);
    if center_w >= width {
        return fit_line(&center, width);
    }

    let rail_total = width - center_w;
    let left_w = rail_total / 2;
    let right_w = rail_total - left_w;
    let offset = tick as usize;

    let mut out = String::with_capacity(width);
    out.push_str(&rail_segment(left_w, offset));
    out.push_str(&center);
    out.push_str(&rail_segment(right_w, offset + left_w));
    fit_line(&out, width)
}

fn rail_segment(target_width: usize, offset: usize) -> String {
    if target_width == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(target_width);
    let mut used = 0usize;
    let mut idx = offset % MARQUEE_TILE.len();
    while used < target_width {
        let ch = MARQUEE_TILE[idx];
        let cw = char_display_width(ch);
        if used + cw > target_width {
            break;
        }
        out.push(ch);
        used += cw;
        idx = (idx + 1) % MARQUEE_TILE.len();
    }
    out
}

fn char_display_width(ch: char) -> usize {
    unicode_width::UnicodeWidthChar::width(ch)
        .unwrap_or(0)
        .max(1)
}

fn fit_line(line: &str, width: usize) -> String {
    if display_width(line) <= width {
        pad_line_display_width(line, width)
    } else {
        pad_line_display_width(&truncate_display_width(line, width), width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::events::Event;
    use crate::tui::transcript::{TranscriptState, apply_event};

    #[test]
    fn marquee_fits_width() {
        let text = marquee_text("推理中 · THK", 40, 0);
        assert_eq!(display_width(&text), 40);
    }

    #[test]
    fn marquee_fits_narrow_width() {
        let text = marquee_text("生成回复 · AI", 24, 0);
        assert!(display_width(&text) <= 24);
        assert!(text.contains('-') || text.contains('}'));
    }

    #[test]
    fn marquee_animates_with_tick() {
        let a = marquee_text("tools", 30, 0);
        let b = marquee_text("tools", 30, 3);
        assert_ne!(a, b);
    }

    #[test]
    fn marquee_uses_ascii_rail_not_wide_dots() {
        let text = marquee_text("生成回复 · AI", 48, 0);
        assert!(!text.contains('●'));
        assert!(text.contains('-'));
    }

    #[test]
    fn strip_label_reflects_thinking() {
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
