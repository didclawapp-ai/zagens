//! Panel background presets and theme registry (extensible style switching).

use ratatui::style::Color;

/// Persisted / selectable TUI color layout. Semantic Dracula tokens stay shared in [`super::palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuiThemeId {
    /// Legacy bordered shell — all panels `#000000`.
    Classic,
    /// Scheme B — cool blue-gray sidebars, pure-black transcript (default).
    CoolBlue,
    /// Scheme A — minimal gray scale, low contrast.
    GrayScale,
    /// Scheme C — layered charcoal.
    Charcoal,
    /// Scheme D — faint purple-gray sidebars.
    DraculaTint,
    /// Scheme E — higher contrast sidebars.
    HighContrast,
}

impl TuiThemeId {
    pub const DEFAULT: Self = Self::CoolBlue;

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::CoolBlue => "cool-blue",
            Self::GrayScale => "gray-scale",
            Self::Charcoal => "charcoal",
            Self::DraculaTint => "dracula-tint",
            Self::HighContrast => "high-contrast",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic (bordered)",
            Self::CoolBlue => "Cool blue-gray",
            Self::GrayScale => "Gray scale",
            Self::Charcoal => "Charcoal",
            Self::DraculaTint => "Dracula tint",
            Self::HighContrast => "High contrast",
        }
    }

    /// All ids in UI / settings order.
    pub const ALL: [Self; 6] = [
        Self::CoolBlue,
        Self::GrayScale,
        Self::Charcoal,
        Self::DraculaTint,
        Self::HighContrast,
        Self::Classic,
    ];

    #[must_use]
    pub fn from_storage(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            None | Some("") | Some("cool-blue") | Some("cool_blue") | Some("b") => Self::CoolBlue,
            Some("classic") | Some("bordered") => Self::Classic,
            Some("gray-scale") | Some("gray_scale") | Some("a") => Self::GrayScale,
            Some("charcoal") | Some("c") => Self::Charcoal,
            Some("dracula-tint") | Some("dracula_tint") | Some("d") => Self::DraculaTint,
            Some("high-contrast") | Some("high_contrast") | Some("e") => Self::HighContrast,
            _ => Self::DEFAULT,
        }
    }

    #[must_use]
    pub fn cycle(self) -> Self {
        let all = Self::ALL;
        let idx = all.iter().position(|id| *id == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }
}

/// Whether panes draw ratatui border lines or rely on background color alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeLayout {
    Borderless,
    Bordered,
}

/// Per-panel background colors (24-bit RGB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelSurfaces {
    pub title: Color,
    pub left: Color,
    pub left_active: Color,
    pub transcript: Color,
    pub activity: Color,
    pub composer: Color,
    pub status: Color,
    pub inspector: Color,
    pub inspector_active: Color,
    pub lht: Color,
    pub lht_active: Color,
}

impl PanelSurfaces {
    #[must_use]
    pub fn surface_for(self, panel: super::TuiPanel) -> Color {
        match panel {
            super::TuiPanel::Title => self.title,
            super::TuiPanel::Left => self.left,
            super::TuiPanel::Transcript => self.transcript,
            super::TuiPanel::Activity => self.activity,
            super::TuiPanel::Composer => self.composer,
            super::TuiPanel::Status => self.status,
            super::TuiPanel::Inspector => self.inspector,
            super::TuiPanel::Lht => self.lht,
        }
    }

    #[must_use]
    pub fn active_for(self, panel: super::TuiPanel) -> Color {
        match panel {
            super::TuiPanel::Left => self.left_active,
            super::TuiPanel::Inspector => self.inspector_active,
            super::TuiPanel::Lht => self.lht_active,
            other => tint_color(self.surface_for(other), 0x0a),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiTheme {
    pub id: TuiThemeId,
    pub layout: ThemeLayout,
    pub surfaces: PanelSurfaces,
}

impl TuiTheme {
    #[must_use]
    pub fn default_theme() -> Self {
        Self::resolve(TuiThemeId::DEFAULT)
    }

    #[must_use]
    pub fn resolve(id: TuiThemeId) -> Self {
        let (layout, surfaces) = match id {
            TuiThemeId::Classic => (ThemeLayout::Bordered, classic_surfaces()),
            TuiThemeId::CoolBlue => (ThemeLayout::Borderless, cool_blue_surfaces()),
            TuiThemeId::GrayScale => (ThemeLayout::Borderless, gray_scale_surfaces()),
            TuiThemeId::Charcoal => (ThemeLayout::Borderless, charcoal_surfaces()),
            TuiThemeId::DraculaTint => (ThemeLayout::Borderless, dracula_tint_surfaces()),
            TuiThemeId::HighContrast => (ThemeLayout::Borderless, high_contrast_surfaces()),
        };
        Self {
            id,
            layout,
            surfaces,
        }
    }

    #[must_use]
    pub fn pane_chrome_rows(self) -> u16 {
        match self.layout {
            ThemeLayout::Borderless => 1,
            ThemeLayout::Bordered => 2,
        }
    }

    #[must_use]
    pub fn borderless(self) -> bool {
        self.layout == ThemeLayout::Borderless
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Rgb(r, g, b)
}

#[must_use]
pub fn tint_color(base: Color, amount: u8) -> Color {
    match base {
        Color::Rgb(r, g, b) => Color::Rgb(
            r.saturating_add(amount),
            g.saturating_add(amount),
            b.saturating_add(amount),
        ),
        other => other,
    }
}

fn classic_surfaces() -> PanelSurfaces {
    let black = rgb(0, 0, 0);
    let active = rgb(0x22, 0x22, 0x22);
    PanelSurfaces {
        title: black,
        left: black,
        left_active: active,
        transcript: black,
        activity: black,
        composer: black,
        status: black,
        inspector: black,
        inspector_active: active,
        lht: black,
        lht_active: active,
    }
}

fn cool_blue_surfaces() -> PanelSurfaces {
    PanelSurfaces {
        title: rgb(0, 0, 0),
        left: rgb(0x12, 0x18, 0x20),
        left_active: rgb(0x1a, 0x22, 0x30),
        transcript: rgb(0, 0, 0),
        activity: rgb(0, 0, 0),
        composer: rgb(0x08, 0x0b, 0x0f),
        status: rgb(0x05, 0x06, 0x08),
        inspector: rgb(0x14, 0x18, 0x20),
        inspector_active: rgb(0x1c, 0x24, 0x30),
        lht: rgb(0x0c, 0x10, 0x18),
        lht_active: rgb(0x14, 0x1c, 0x28),
    }
}

fn gray_scale_surfaces() -> PanelSurfaces {
    PanelSurfaces {
        title: rgb(0, 0, 0),
        left: rgb(0x11, 0x11, 0x11),
        left_active: rgb(0x1a, 0x1a, 0x1a),
        transcript: rgb(0, 0, 0),
        activity: rgb(0, 0, 0),
        composer: rgb(0x0a, 0x0a, 0x0a),
        status: rgb(0x05, 0x05, 0x05),
        inspector: rgb(0x12, 0x12, 0x12),
        inspector_active: rgb(0x1a, 0x1a, 0x1a),
        lht: rgb(0x0a, 0x0a, 0x0a),
        lht_active: rgb(0x12, 0x12, 0x12),
    }
}

fn charcoal_surfaces() -> PanelSurfaces {
    PanelSurfaces {
        title: rgb(0, 0, 0),
        left: rgb(0x16, 0x16, 0x16),
        left_active: rgb(0x20, 0x20, 0x20),
        transcript: rgb(0, 0, 0),
        activity: rgb(0, 0, 0),
        composer: rgb(0x12, 0x12, 0x12),
        status: rgb(0x08, 0x08, 0x08),
        inspector: rgb(0x18, 0x18, 0x18),
        inspector_active: rgb(0x22, 0x22, 0x22),
        lht: rgb(0x10, 0x10, 0x10),
        lht_active: rgb(0x18, 0x18, 0x18),
    }
}

fn dracula_tint_surfaces() -> PanelSurfaces {
    PanelSurfaces {
        title: rgb(0, 0, 0),
        left: rgb(0x15, 0x13, 0x1a),
        left_active: rgb(0x1e, 0x1b, 0x26),
        transcript: rgb(0, 0, 0),
        activity: rgb(0, 0, 0),
        composer: rgb(0x0e, 0x0c, 0x12),
        status: rgb(0x08, 0x07, 0x0a),
        inspector: rgb(0x17, 0x14, 0x1f),
        inspector_active: rgb(0x21, 0x1d, 0x2a),
        lht: rgb(0x0f, 0x0d, 0x14),
        lht_active: rgb(0x17, 0x15, 0x1e),
    }
}

fn high_contrast_surfaces() -> PanelSurfaces {
    PanelSurfaces {
        title: rgb(0, 0, 0),
        left: rgb(0x1e, 0x1e, 0x1e),
        left_active: rgb(0x28, 0x28, 0x28),
        transcript: rgb(0, 0, 0),
        activity: rgb(0, 0, 0),
        composer: rgb(0x14, 0x14, 0x14),
        status: rgb(0x0a, 0x0a, 0x0a),
        inspector: rgb(0x22, 0x22, 0x22),
        inspector_active: rgb(0x2c, 0x2c, 0x2c),
        lht: rgb(0x16, 0x16, 0x16),
        lht_active: rgb(0x20, 0x20, 0x20),
    }
}

static THEME: std::sync::OnceLock<std::sync::RwLock<TuiTheme>> = std::sync::OnceLock::new();

/// Install the active theme for the process (call at TUI startup and on theme switch).
pub fn install(theme: TuiTheme) {
    match THEME.get() {
        Some(lock) => *lock.write().expect("theme lock") = theme,
        None => {
            let _ = THEME.set(std::sync::RwLock::new(theme));
        }
    }
}

#[must_use]
pub fn current() -> TuiTheme {
    THEME
        .get()
        .map(|lock| *lock.read().expect("theme lock"))
        .unwrap_or_else(TuiTheme::default_theme)
}

#[must_use]
pub fn current_id() -> TuiThemeId {
    current().id
}

#[must_use]
pub fn pane_chrome_rows() -> u16 {
    current().pane_chrome_rows()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cool_blue_matches_fixture_preview() {
        let s = cool_blue_surfaces();
        assert_eq!(s.left, rgb(0x12, 0x18, 0x20));
        assert_eq!(s.transcript, rgb(0, 0, 0));
        assert_eq!(s.composer, rgb(0x08, 0x0b, 0x0f));
        assert_eq!(s.lht, rgb(0x0c, 0x10, 0x18));
    }

    #[test]
    fn theme_id_round_trip() {
        assert_eq!(
            TuiThemeId::from_storage(Some("cool-blue")),
            TuiThemeId::CoolBlue
        );
        assert_eq!(TuiThemeId::CoolBlue.cycle(), TuiThemeId::GrayScale);
        assert!(TuiTheme::resolve(TuiThemeId::CoolBlue).borderless());
        assert!(TuiTheme::resolve(TuiThemeId::Classic).layout == ThemeLayout::Bordered);
    }
}
