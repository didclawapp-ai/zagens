use std::path::{Path, PathBuf};

pub fn zagens_home_from_env() -> PathBuf {
    if let Ok(v) = std::env::var("ZAGENS_HOME") {
        return PathBuf::from(v);
    }
    if let Ok(v) = std::env::var("DEEPSEEK_HOME") {
        return PathBuf::from(v);
    }
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zagens")
}

pub fn zagens_home() -> PathBuf {
    zagens_home_from_env()
}

pub fn sandbox_bin_dir(zagens_home: &Path) -> PathBuf {
    zagens_home.join(".sandbox-bin")
}

pub fn sandbox_secrets_dir(zagens_home: &Path) -> PathBuf {
    zagens_home.join(".sandbox-secrets")
}

pub fn setup_marker_path(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("setup_marker.json")
}

pub fn sandbox_users_path(zagens_home: &Path) -> PathBuf {
    sandbox_secrets_dir(zagens_home).join("sandbox_users.json")
}

pub fn sandbox_dir(zagens_home: &Path) -> PathBuf {
    zagens_home.join(".sandbox")
}

pub fn cap_sid_file(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("cap_sid")
}

pub fn poc_result_file(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("unelevated_deny_read_poc.json")
}
