//! Load sandbox-user logon credentials from DPAPI-protected setup artifacts.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;

use crate::dpapi::unprotect;
use crate::paths::sandbox_users_path;
use crate::setup::{SandboxUserRecord, SandboxUsersFile, sandbox_setup_is_complete};

#[derive(Debug, Clone)]
pub struct SandboxCreds {
    pub username: String,
    pub password: String,
}

fn load_users(zagens_home: &Path) -> Result<Option<SandboxUsersFile>> {
    let path = sandbox_users_path(zagens_home);
    let file = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("read sandbox_users.json"),
    };
    serde_json::from_str(&file)
        .context("parse sandbox_users.json")
        .map(Some)
}

fn decode_password(record: &SandboxUserRecord) -> Result<String> {
    let blob = BASE64
        .decode(record.password.as_bytes())
        .context("base64 decode sandbox password")?;
    let decrypted = unprotect(&blob)?;
    String::from_utf8(decrypted).context("sandbox password not utf-8")
}

/// Returns offline or online sandbox-user credentials for elevated spawn.
///
/// Callers must ensure setup completed (`deepseek sandbox setup`) before invoking.
pub fn require_sandbox_creds(zagens_home: &Path, network_allowed: bool) -> Result<SandboxCreds> {
    if !sandbox_setup_is_complete(zagens_home) {
        return Err(anyhow!(
            "Windows sandbox setup is incomplete; run `deepseek sandbox setup` first"
        ));
    }
    let users = load_users(zagens_home)?
        .filter(|u| u.version_matches())
        .ok_or_else(|| anyhow!("sandbox users file missing or incompatible"))?;
    let record = if network_allowed {
        &users.online
    } else {
        &users.offline
    };
    let password = decode_password(record)?;
    Ok(SandboxCreds {
        username: record.username.clone(),
        password,
    })
}
