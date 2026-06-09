use serde::{Deserialize, Serialize};

/// Windows native sandbox mode (`[windows] sandbox` in config.toml).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WindowsSandboxModeToml {
    #[default]
    Unelevated,
    Elevated,
}

impl WindowsSandboxModeToml {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "elevated" => Some(Self::Elevated),
            "unelevated" => Some(Self::Unelevated),
            _ => None,
        }
    }
}

/// `[windows]` table — native OS sandbox knobs (Windows only at runtime).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsConfigToml {
    pub sandbox: Option<WindowsSandboxModeToml>,
    /// Defaults to `true`. Set to `false` to launch the sandboxed child on
    /// `Winsta0\\Default` instead of a private desktop (Phase 3).
    pub sandbox_private_desktop: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_sandbox_toml() {
        let cfg: WindowsConfigToml = toml::from_str(r#"sandbox = "unelevated""#).unwrap();
        assert_eq!(cfg.sandbox, Some(WindowsSandboxModeToml::Unelevated));
    }
}
