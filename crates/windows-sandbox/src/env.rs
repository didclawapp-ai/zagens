//! Unelevated network advisory env poison (weak isolation — see design §10.2).

use std::collections::HashMap;

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
