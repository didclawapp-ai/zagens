//! Workspace bootstrap for common Windows + Node friction (TS-14).
//!
//! Runs once per engine session when the user sends a message. Idempotent:
//! only creates files that are missing; never overwrites operator `.npmrc`.

use std::fs;
use std::path::Path;

const NPMRC_BODY: &str = "# Zagens Windows preflight (TS-14): workspace-local npm cache.\n\
                          cache=./.npm-cache\n";

const JEST_CONFIG_BODY: &str = "// Zagens Windows preflight (TS-14): avoid Jest worker spawn EPERM in sandbox.\n\
                               module.exports = {\n\
                                 maxWorkers: 1,\n\
                               };\n";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowsNodePreflightReport {
    pub npmrc_created: bool,
    pub jest_config_created: bool,
    pub skipped_reason: Option<&'static str>,
}

/// Apply Windows Node.js workspace defaults when `package.json` is present.
#[must_use]
pub fn apply_windows_node_preflight(workspace: &Path) -> WindowsNodePreflightReport {
    if !cfg!(windows) {
        return WindowsNodePreflightReport {
            skipped_reason: Some("not_windows"),
            ..Default::default()
        };
    }
    if !workspace.join("package.json").is_file() {
        return WindowsNodePreflightReport {
            skipped_reason: Some("no_package_json"),
            ..Default::default()
        };
    }

    let mut report = WindowsNodePreflightReport::default();

    let npmrc = workspace.join(".npmrc");
    if !npmrc.is_file() && write_new_file(&npmrc, NPMRC_BODY).is_ok() {
        report.npmrc_created = true;
    }

    let has_jest =
        workspace.join("node_modules").join("jest").is_dir() || read_package_has_jest(workspace);
    if has_jest {
        let jest_cfg = workspace.join("jest.config.js");
        let jest_cfg_ts = workspace.join("jest.config.ts");
        let jest_cfg_mjs = workspace.join("jest.config.mjs");
        if !jest_cfg.is_file()
            && !jest_cfg_ts.is_file()
            && !jest_cfg_mjs.is_file()
            && write_new_file(&jest_cfg, JEST_CONFIG_BODY).is_ok()
        {
            report.jest_config_created = true;
        }
    }

    report
}

fn write_new_file(path: &Path, body: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)
}

fn read_package_has_jest(workspace: &Path) -> bool {
    let Ok(raw) = fs::read_to_string(workspace.join("package.json")) else {
        return false;
    };
    raw.contains("\"jest\"")
        || raw.contains("jest.config")
        || raw.contains("\"test\":") && (raw.contains("jest") || raw.contains("npm test"))
}

/// Human-readable status for `Event::status` / sidecar logs.
#[must_use]
pub fn format_preflight_status(report: &WindowsNodePreflightReport) -> Option<String> {
    if report.npmrc_created || report.jest_config_created {
        Some(format!(
            "workspace_preflight: {{\"npmrc\":{},\"jest_config\":{}}}",
            report.npmrc_created, report.jest_config_created
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn preflight_skips_without_package_json() {
        let tmp = TempDir::new().expect("tmp");
        let report = apply_windows_node_preflight(tmp.path());
        assert_eq!(report.skipped_reason, Some("no_package_json"));
    }

    #[test]
    fn preflight_creates_npmrc_when_missing() {
        let tmp = TempDir::new().expect("tmp");
        fs::write(tmp.path().join("package.json"), r#"{"name":"x"}"#).expect("write");
        let report = apply_windows_node_preflight(tmp.path());
        if cfg!(windows) {
            assert!(report.npmrc_created);
            assert!(tmp.path().join(".npmrc").is_file());
        } else {
            assert_eq!(report.skipped_reason, Some("not_windows"));
        }
    }

    #[test]
    fn preflight_does_not_overwrite_existing_npmrc() {
        let tmp = TempDir::new().expect("tmp");
        fs::write(tmp.path().join("package.json"), r#"{"name":"x"}"#).expect("write");
        fs::write(tmp.path().join(".npmrc"), "cache=./custom\n").expect("write");
        let report = apply_windows_node_preflight(tmp.path());
        if cfg!(windows) {
            assert!(!report.npmrc_created);
            let body = fs::read_to_string(tmp.path().join(".npmrc")).expect("read");
            assert!(body.contains("custom"));
        }
    }
}
