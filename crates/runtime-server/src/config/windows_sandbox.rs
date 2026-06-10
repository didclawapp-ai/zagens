//! Windows native sandbox config resolution (mirrors Codex `windows_sandbox.rs`).

use zagens_config::WindowsSandboxModeToml;

use super::types::Config;

/// Effective Windows sandbox mode from config.
///
/// Resolution order (design §11): explicit `[windows] sandbox` → when unset,
/// **elevated** if elevated setup completed (PR-2.12 / G2) → otherwise **unelevated**.
#[must_use]
pub fn resolve_windows_sandbox_mode(config: &Config) -> WindowsSandboxModeToml {
    if let Some(mode) = config.windows.as_ref().and_then(|windows| windows.sandbox) {
        return mode;
    }
    default_windows_sandbox_mode_when_unconfigured()
}

fn default_windows_sandbox_mode_when_unconfigured() -> WindowsSandboxModeToml {
    #[cfg(windows)]
    {
        let home = zagens_windows_sandbox::zagens_home();
        if zagens_windows_sandbox::sandbox_setup_is_complete(&home) {
            return WindowsSandboxModeToml::Elevated;
        }
    }
    WindowsSandboxModeToml::Unelevated
}

/// Whether the private-desktop spawn path is enabled (Phase 3; default true).
#[must_use]
pub fn resolve_windows_sandbox_private_desktop(config: &Config) -> bool {
    config
        .windows
        .as_ref()
        .and_then(|windows| windows.sandbox_private_desktop)
        .unwrap_or(true)
}

/// Parse and validate a `[windows] sandbox` override string.
pub fn parse_windows_sandbox_mode(value: &str) -> anyhow::Result<WindowsSandboxModeToml> {
    WindowsSandboxModeToml::parse(value).ok_or_else(|| {
        anyhow::anyhow!("Invalid windows.sandbox '{value}': expected elevated or unelevated.")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zagens_config::WindowsConfigToml;

    #[test]
    fn defaults_to_unelevated_when_absent_and_setup_incomplete() {
        let config = Config::default();
        assert_eq!(
            resolve_windows_sandbox_mode(&config),
            WindowsSandboxModeToml::Unelevated
        );
    }

    #[test]
    fn reads_elevated_from_config() {
        let config = Config {
            windows: Some(WindowsConfigToml {
                sandbox: Some(WindowsSandboxModeToml::Elevated),
                sandbox_private_desktop: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_windows_sandbox_mode(&config),
            WindowsSandboxModeToml::Elevated
        );
    }

    #[test]
    fn explicit_unelevated_overrides_setup_complete_default() {
        let config = Config {
            windows: Some(WindowsConfigToml {
                sandbox: Some(WindowsSandboxModeToml::Unelevated),
                sandbox_private_desktop: None,
            }),
            ..Default::default()
        };
        assert_eq!(
            resolve_windows_sandbox_mode(&config),
            WindowsSandboxModeToml::Unelevated
        );
    }
}
