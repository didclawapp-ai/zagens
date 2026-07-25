use serde::{Deserialize, Serialize};

/// `[agent]` table — agent harness knobs (shell spawn, etc.).
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentConfigToml {
    /// Windows shell for `exec_shell` / hooks / gates: `"auto"` | `"pwsh"` | `"powershell"` | `"cmd"`.
    /// Defaults to auto-detect (`pwsh` → `powershell` → `COMSPEC`/`cmd`) when unset.
    #[serde(default)]
    pub shell: Option<String>,
}

#[must_use]
pub fn normalize_agent_shell(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "" | "auto" => None,
        "pwsh" => Some("pwsh"),
        "powershell" => Some("powershell"),
        "cmd" => Some("cmd"),
        _ => None,
    }
}

#[must_use]
pub fn is_valid_agent_shell(value: &str) -> bool {
    normalize_agent_shell(value).is_some()
        || matches!(value.trim().to_ascii_lowercase().as_str(), "" | "auto")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_shell_toml() {
        let cfg: AgentConfigToml = toml::from_str(r#"shell = "pwsh""#).unwrap();
        assert_eq!(cfg.shell.as_deref(), Some("pwsh"));
    }

    #[test]
    fn validates_shell_values() {
        assert!(is_valid_agent_shell("auto"));
        assert!(is_valid_agent_shell("pwsh"));
        assert!(is_valid_agent_shell("powershell"));
        assert!(is_valid_agent_shell("cmd"));
        assert!(!is_valid_agent_shell("bash"));
    }
}
