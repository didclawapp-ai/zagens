//! Windows native sandbox config resolution (mirrors Codex `windows_sandbox.rs`).

use zagens_config::WindowsSandboxModeToml;

use super::types::Config;

/// Effective Windows sandbox mode from config. Defaults to **unelevated** when
/// unset (Phase 1 MVP; elevated becomes default after G2 setup — PR-2.12).
#[must_use]
pub fn resolve_windows_sandbox_mode(config: &Config) -> WindowsSandboxModeToml {
    config
        .windows
        .as_ref()
        .and_then(|windows| windows.sandbox)
        .unwrap_or(WindowsSandboxModeToml::Unelevated)
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

    #[test]
    fn defaults_to_unelevated_when_absent() {
        let config = Config::default();
        assert_eq!(
            resolve_windows_sandbox_mode(&config),
            WindowsSandboxModeToml::Unelevated
        );
    }

    #[test]
    fn reads_elevated_from_config() {
        use zagens_config::WindowsConfigToml;
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
}
