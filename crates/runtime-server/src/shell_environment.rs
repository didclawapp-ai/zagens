//! Shell child environment for `exec_shell` / task shell (aligned with Codex `shell_environment.rs`).
//!
//! Windows sandbox spawns use a custom env block (`CreateProcessAsUserW`); without inheriting
//! the parent process environment, MSVC/SDK variables (`LIB`, `INCLUDE`, …) never reach the
//! linker even when installed on the host.

use std::collections::HashMap;
use std::env;

/// How much of the parent process environment is copied into sandboxed shell children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellEnvironmentInherit {
    /// Copy nearly all parent vars (minus default secret excludes). Codex default.
    All,
    /// Copy a platform-specific core set (PATH, TEMP/TMP, USERPROFILE, …).
    Core,
    /// Start from hook/`spec.env` overrides only; sandbox prep still adds PATH when missing.
    None,
}

impl ShellEnvironmentInherit {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Core => "core",
            Self::None => "none",
        }
    }
}

/// Policy for constructing shell child environments.
#[derive(Debug, Clone)]
pub struct ShellEnvironmentPolicy {
    pub inherit: ShellEnvironmentInherit,
    /// When true, strip vars matching `*KEY*`, `*SECRET*`, `*TOKEN*` (case-insensitive).
    pub apply_default_secret_excludes: bool,
}

impl Default for ShellEnvironmentPolicy {
    fn default() -> Self {
        Self {
            inherit: ShellEnvironmentInherit::All,
            apply_default_secret_excludes: true,
        }
    }
}

/// Default inherit mode used for agent `exec_shell` children.
#[must_use]
pub fn default_exec_shell_inherit() -> ShellEnvironmentInherit {
    ShellEnvironmentPolicy::default().inherit
}

/// Build the env map for a sandboxed shell spawn from the current process environment.
#[must_use]
pub fn create_exec_shell_env() -> HashMap<String, String> {
    create_exec_shell_env_from_vars(env::vars(), ShellEnvironmentPolicy::default())
}

/// Merge hook/`shell_env` overrides on top of the inherited exec-shell environment.
#[must_use]
pub fn merge_exec_shell_env(hook_env: HashMap<String, String>) -> HashMap<String, String> {
    let mut env = create_exec_shell_env();
    env.extend(hook_env);
    env
}

#[must_use]
pub fn create_exec_shell_env_from_vars<I>(
    vars: I,
    policy: ShellEnvironmentPolicy,
) -> HashMap<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut env_map = match policy.inherit {
        ShellEnvironmentInherit::All => vars.into_iter().collect(),
        ShellEnvironmentInherit::None => HashMap::new(),
        ShellEnvironmentInherit::Core => vars
            .into_iter()
            .filter(|(k, _)| core_env_var_allowed(k))
            .collect(),
    };

    if policy.apply_default_secret_excludes {
        env_map.retain(|k, _| !matches_default_secret_exclude(k));
    }

    #[cfg(windows)]
    if !env_map.keys().any(|k| k.eq_ignore_ascii_case("PATHEXT")) {
        env_map.insert("PATHEXT".to_string(), ".COM;.EXE;.BAT;.CMD".to_string());
    }

    env_map
}

/// Whether the current process appears to carry MSVC / Windows SDK linker env.
#[must_use]
pub fn parent_has_toolchain_env() -> bool {
    env_has_toolchain_marker(env::vars())
}

#[must_use]
pub fn env_has_toolchain_marker<I>(vars: I) -> bool
where
    I: IntoIterator<Item = (String, String)>,
{
    vars.into_iter().any(|(key, value)| {
        if value.is_empty() {
            return false;
        }
        matches!(
            key.to_ascii_uppercase().as_str(),
            "LIB"
                | "INCLUDE"
                | "LIBPATH"
                | "VCINSTALLDIR"
                | "VCTOOLSINSTALLDIR"
                | "WINDOWSSDKDIR"
                | "WINDOWSSDKVERSION"
                | "VSCMD_VER"
        )
    })
}

fn core_env_var_allowed(name: &str) -> bool {
    #[cfg(not(windows))]
    const CORE: &[&str] = &[
        "PATH", "SHELL", "TMPDIR", "TEMP", "TMP", "HOME", "LANG", "LC_ALL", "LC_CTYPE", "LOGNAME",
        "USER",
    ];
    #[cfg(windows)]
    const CORE: &[&str] = &[
        "PATH",
        "PATHEXT",
        "SHELL",
        "COMSPEC",
        "SYSTEMROOT",
        "SYSTEMDRIVE",
        "USERNAME",
        "USERDOMAIN",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "PROGRAMFILES",
        "PROGRAMFILES(X86)",
        "PROGRAMW6432",
        "PROGRAMDATA",
        "LOCALAPPDATA",
        "APPDATA",
        "TEMP",
        "TMP",
        "TMPDIR",
        "POWERSHELL",
        "PWSH",
        "LIB",
        "INCLUDE",
        "LIBPATH",
        "VCINSTALLDIR",
        "VCTOOLSINSTALLDIR",
        "WINDOWSSDKDIR",
        "WINDOWSSDKVERSION",
        "VSCMD_VER",
    ];

    CORE.iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(name))
}

fn matches_default_secret_exclude(name: &str) -> bool {
    const DEFAULT_EXCLUDES: &[&str] = &["KEY", "SECRET", "TOKEN"];
    let upper = name.to_ascii_uppercase();
    DEFAULT_EXCLUDES.iter().any(|needle| upper.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_inherit_keeps_lib_when_not_secret_filtered() {
        let vars = vec![
            ("LIB".to_string(), "C:\\SDK\\Lib".to_string()),
            ("PATH".to_string(), "C:\\Windows".to_string()),
        ];
        let env = create_exec_shell_env_from_vars(
            vars,
            ShellEnvironmentPolicy {
                inherit: ShellEnvironmentInherit::All,
                apply_default_secret_excludes: true,
            },
        );
        assert_eq!(env.get("LIB").map(String::as_str), Some("C:\\SDK\\Lib"));
    }

    #[test]
    fn default_secret_exclude_strips_api_key() {
        let vars = vec![("OPENAI_API_KEY".to_string(), "sk-test".to_string())];
        let env = create_exec_shell_env_from_vars(vars, ShellEnvironmentPolicy::default());
        assert!(!env.contains_key("OPENAI_API_KEY"));
    }

    #[test]
    fn core_inherit_keeps_toolchain_vars_case_insensitively() {
        let vars = vec![
            ("include".to_string(), "C:\\SDK\\Include".to_string()),
            ("Path".to_string(), "C:\\Windows".to_string()),
        ];
        let env = create_exec_shell_env_from_vars(
            vars,
            ShellEnvironmentPolicy {
                inherit: ShellEnvironmentInherit::Core,
                apply_default_secret_excludes: true,
            },
        );
        assert_eq!(
            env.get("include").map(String::as_str),
            Some("C:\\SDK\\Include")
        );
        assert_eq!(env.get("Path").map(String::as_str), Some("C:\\Windows"));
    }

    #[test]
    fn merge_exec_shell_env_applies_hook_overrides() {
        let hook = HashMap::from([("HOOK_ONLY".to_string(), "from-hook".to_string())]);
        let merged = merge_exec_shell_env(hook);
        assert_eq!(
            merged.get("HOOK_ONLY").map(String::as_str),
            Some("from-hook")
        );
    }

    #[test]
    fn env_has_toolchain_marker_detects_lib() {
        assert!(env_has_toolchain_marker([(
            "LIB".to_string(),
            "x".to_string()
        )]));
        assert!(!env_has_toolchain_marker([(
            "PATH".to_string(),
            "x".to_string()
        )]));
    }

    #[cfg(windows)]
    #[test]
    fn create_exec_shell_env_inserts_pathext_when_missing() {
        let env = create_exec_shell_env_from_vars(
            Vec::<(String, String)>::new(),
            ShellEnvironmentPolicy {
                inherit: ShellEnvironmentInherit::None,
                apply_default_secret_excludes: false,
            },
        );
        assert_eq!(
            env.get("PATHEXT").map(String::as_str),
            Some(".COM;.EXE;.BAT;.CMD")
        );
    }
}
