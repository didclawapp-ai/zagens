//! Keyboard focus regions for the three-column layout.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusRegion {
    #[default]
    Chat,
    Left,
    Right,
}

impl FocusRegion {
    pub fn next(self) -> Self {
        match self {
            Self::Left => Self::Chat,
            Self::Chat => Self::Right,
            Self::Right => Self::Left,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Chat => Self::Left,
            Self::Right => Self::Chat,
        }
    }
}
