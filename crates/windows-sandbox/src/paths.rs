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

pub fn sandbox_dir(zagens_home: &Path) -> PathBuf {
    zagens_home.join(".sandbox")
}

pub fn cap_sid_file(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("cap_sid")
}

pub fn poc_result_file(zagens_home: &Path) -> PathBuf {
    sandbox_dir(zagens_home).join("unelevated_deny_read_poc.json")
}
