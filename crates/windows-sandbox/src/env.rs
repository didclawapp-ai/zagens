//! Unelevated network advisory env poison (weak isolation — see design §10.2).

use std::collections::HashMap;
use std::env;

/// Copy PATH/PATHEXT from the parent when missing.
///
/// `CreateProcessAsUserW` with `CREATE_UNICODE_ENVIRONMENT` uses only the
/// supplied block — it does not inherit the parent environment. Codex does the
/// same in `windows-sandbox-rs/src/env.rs` so tools like `Start-Process` and
/// `more.com` resolve under a restricted token.
pub fn inherit_path_env(env: &mut HashMap<String, String>) {
    if !env.contains_key("PATH")
        && let Ok(path) = env::var("PATH")
    {
        env.insert("PATH".to_string(), path);
    }
    if !env.contains_key("PATHEXT")
        && let Ok(pathext) = env::var("PATHEXT")
    {
        env.insert("PATHEXT".to_string(), pathext);
    }
}

/// Minimal Windows process-locator keys for a custom env block.
pub fn inherit_windows_process_locator_env(env: &mut HashMap<String, String>) {
    for key in [
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
    ] {
        if !env.contains_key(key)
            && let Ok(value) = env::var(key)
        {
            env.insert(key.to_string(), value);
        }
    }
}

pub fn apply_unelevated_network_poison(env: &mut HashMap<String, String>, network_allowed: bool) {
    if network_allowed {
        return;
    }
    env.insert("HTTPS_PROXY".to_string(), "http://127.0.0.1:9".to_string());
    env.insert("HTTP_PROXY".to_string(), "http://127.0.0.1:9".to_string());
    env.insert("ALL_PROXY".to_string(), "http://127.0.0.1:9".to_string());
    env.insert(
        "GIT_HTTPS_PROXY".to_string(),
        "http://127.0.0.1:9".to_string(),
    );
    env.insert("GIT_SSH_COMMAND".to_string(), "cmd /c exit 1".to_string());
    env.insert(
        "NO_PROXY".to_string(),
        "localhost,127.0.0.1,::1".to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherit_path_env_copies_parent_path() {
        let mut env = HashMap::new();
        inherit_path_env(&mut env);
        if let Ok(path) = env::var("PATH") {
            assert_eq!(env.get("PATH").map(String::as_str), Some(path.as_str()));
        }
    }

    #[test]
    fn poison_skipped_when_network_allowed() {
        let mut env = HashMap::new();
        apply_unelevated_network_poison(&mut env, true);
        assert!(env.is_empty());
    }

    #[test]
    fn poison_sets_proxy_keys() {
        let mut env = HashMap::new();
        apply_unelevated_network_poison(&mut env, false);
        assert_eq!(
            env.get("HTTPS_PROXY").map(String::as_str),
            Some("http://127.0.0.1:9")
        );
        assert_eq!(
            env.get("GIT_SSH_COMMAND").map(String::as_str),
            Some("cmd /c exit 1")
        );
    }
}
