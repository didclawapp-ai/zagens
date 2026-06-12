//! Composer LHT tri-state labels (settings.toml `lht_composer_mode`).

use zagens_config::LhtComposerMode;

#[must_use]
pub fn format_lht_mode_label(mode: LhtComposerMode) -> String {
    match mode {
        LhtComposerMode::Auto => "LHT Auto".to_string(),
        LhtComposerMode::Strict => "LHT Strict".to_string(),
        LhtComposerMode::Off => "LHT Off".to_string(),
    }
}

#[must_use]
pub fn load_lht_composer_mode() -> LhtComposerMode {
    zagens_config::read_lht_composer_mode_setting().unwrap_or(LhtComposerMode::Auto)
}

pub fn persist_lht_composer_mode(mode: LhtComposerMode) -> anyhow::Result<()> {
    zagens_config::write_lht_composer_mode_setting(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_match_tri_state() {
        assert_eq!(format_lht_mode_label(LhtComposerMode::Auto), "LHT Auto");
        assert_eq!(format_lht_mode_label(LhtComposerMode::Strict), "LHT Strict");
        assert_eq!(format_lht_mode_label(LhtComposerMode::Off), "LHT Off");
    }
}
