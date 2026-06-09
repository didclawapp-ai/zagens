//! Gate G0: verify unelevated restricted token + cap SID deny-read ACE blocks `.ssh` reads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::cap::load_or_create_cap_sids;
use crate::deny_read::{apply_deny_read_acls, revoke_deny_read_acls};
use crate::paths::{poc_result_file, sandbox_dir, zagens_home_from_env};
use crate::process::run_as_user;
use crate::token::{LocalSid, create_restricted_token_with_capabilities};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnelevatedDenyReadPocResult {
    pub result: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win32_last_error: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<u32>,
}

pub fn zagens_home() -> PathBuf {
    zagens_home_from_env()
}

pub fn write_poc_result(result: &UnelevatedDenyReadPocResult) -> Result<PathBuf> {
    let home = zagens_home();
    std::fs::create_dir_all(sandbox_dir(&home))?;
    let path = poc_result_file(&home);
    let json = serde_json::to_string_pretty(result)?;
    std::fs::write(&path, json).with_context(|| format!("write {}", path.display()))?;
    Ok(path)
}

pub fn run_unelevated_deny_read_poc() -> Result<UnelevatedDenyReadPocResult> {
    let home = zagens_home();
    let caps = load_or_create_cap_sids(&home)?;
    let cap_sid = LocalSid::from_string(&caps.workspace)?;

    let user_profile = std::env::var("USERPROFILE")
        .map(PathBuf::from)
        .context("USERPROFILE not set")?;
    let ssh_dir = user_profile.join(".ssh");

    let probe = pick_probe_file(&ssh_dir);
    let probe_display = probe.display().to_string();

    let _deny_added = apply_deny_read_acls(&[ssh_dir.clone()], &cap_sid)?;

    let token = create_restricted_token_with_capabilities(&[&caps.workspace])?;

    let cwd = std::env::current_dir().unwrap_or_else(|_| user_profile.clone());
    let argv = build_probe_argv(&probe);
    let env = HashMap::new();

    let output = run_as_user(token.handle(), &argv, &cwd, &env);

    revoke_deny_read_acls(&[ssh_dir], &cap_sid);

    let timestamp = Utc::now().to_rfc3339();

    match output {
        Ok(captured) => {
            let combined = format!("{}{}", captured.stdout, captured.stderr);
            let denied = captured.exit_code != 0
                || combined.to_ascii_lowercase().contains("access is denied")
                || combined.to_ascii_lowercase().contains("permission denied")
                || combined.contains("EACCES");
            let leaked = !denied
                && probe.exists()
                && probe
                    .file_name()
                    .is_some_and(|n| n != "id_rsa" && n != "id_ed25519")
                && captured.stdout.lines().any(|l| {
                    l.contains("Host ") || l.contains("IdentityFile") || l.starts_with("-----BEGIN")
                });

            if denied && !leaked {
                Ok(UnelevatedDenyReadPocResult {
                    result: "pass".to_string(),
                    timestamp,
                    win32_last_error: None,
                    notes: Some(
                        "Restricted token + cap SID deny-read blocked probe read".to_string(),
                    ),
                    probe: Some(probe_display),
                    exit_code: Some(captured.exit_code),
                })
            } else {
                Ok(UnelevatedDenyReadPocResult {
                    result: "fail".to_string(),
                    timestamp,
                    win32_last_error: if captured.exit_code == 5 {
                        Some(5)
                    } else {
                        None
                    },
                    notes: Some(format!(
                        "Probe may have read sensitive content (exit={}, leaked={leaked})",
                        captured.exit_code
                    )),
                    probe: Some(probe_display),
                    exit_code: Some(captured.exit_code),
                })
            }
        }
        Err(err) => Ok(UnelevatedDenyReadPocResult {
            result: "fail".to_string(),
            timestamp,
            win32_last_error: None,
            notes: Some(format!("Spawn failed: {err}")),
            probe: Some(probe_display),
            exit_code: None,
        }),
    }
}

fn pick_probe_file(ssh_dir: &Path) -> PathBuf {
    for name in ["config", "known_hosts"] {
        let p = ssh_dir.join(name);
        if p.is_file() {
            return p;
        }
    }
    ssh_dir.join("config")
}

fn build_probe_argv(probe: &Path) -> Vec<String> {
    if probe.is_file() {
        vec![
            "cmd".to_string(),
            "/c".to_string(),
            format!("type \"{}\"", probe.display()),
        ]
    } else {
        vec![
            "cmd".to_string(),
            "/c".to_string(),
            format!("dir \"{}\"", probe.parent().unwrap_or(probe).display()),
        ]
    }
}
