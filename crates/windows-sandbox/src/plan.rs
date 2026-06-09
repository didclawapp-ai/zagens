//! Map runtime inputs into a Windows execution plan (unelevated MVP).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::deny_read::unelevated_deny_read_enabled;
use crate::env::apply_unelevated_network_poison;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsSandboxMode {
    Unelevated,
}

#[derive(Debug, Clone)]
pub struct PlanInput {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub writable_roots: Vec<PathBuf>,
    pub protected_write_paths: Vec<PathBuf>,
    pub network_allowed: bool,
}

#[derive(Debug, Clone)]
pub struct WindowsExecPlan {
    pub mode: WindowsSandboxMode,
    pub argv: Vec<String>,
    pub cwd: PathBuf,
    pub env: HashMap<String, String>,
    pub writable_roots: Vec<PathBuf>,
    pub protected_write_paths: Vec<PathBuf>,
    pub apply_deny_read: bool,
    pub network_allowed: bool,
}

pub fn plan_exec(input: PlanInput) -> Result<WindowsExecPlan> {
    // G0 PoC validates deny-read with `cmd /c type` + restricted token. PowerShell
    // `-Command` reads files in-process and may not hit the same cap-SID deny ACE path.
    let argv = enforced_shell_argv(&input.program, &input.args);

    let mut env = input.env;
    env.insert(
        "DEEPSEEK_SANDBOX".to_string(),
        "windows:unelevated".to_string(),
    );
    env.insert("DEEPSEEK_SANDBOX_ENFORCED".to_string(), "1".to_string());
    let apply_deny_read = unelevated_deny_read_enabled();
    if apply_deny_read {
        env.insert("DEEPSEEK_SANDBOX_DENY_READ".to_string(), "1".to_string());
    }
    apply_unelevated_network_poison(&mut env, input.network_allowed);

    let writable_roots = canonicalize_paths(&input.writable_roots);
    let protected_write_paths = canonicalize_paths(&input.protected_write_paths);

    Ok(WindowsExecPlan {
        mode: WindowsSandboxMode::Unelevated,
        argv,
        cwd: cmd_spawn_cwd(&input.cwd),
        env,
        writable_roots,
        protected_write_paths,
        apply_deny_read,
        network_allowed: input.network_allowed,
    })
}

/// CWD for `cmd.exe` spawn: strip Windows verbatim `\\?\` prefixes.
///
/// Thread workspaces are stored canonicalized (`\\?\F:\…`) for ACL/path logic, but
/// `cmd.exe` rejects that form as the process working directory and falls back to
/// `C:\Windows`, breaking relative paths and some redirects.
pub fn cmd_spawn_cwd(path: &Path) -> PathBuf {
    let s = path.display().to_string();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path.to_path_buf()
}

fn canonicalize_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !out.iter().any(|existing| existing == &canonical) {
            out.push(canonical);
        }
    }
    out
}

pub fn protected_subdirs_for_root(root: &Path) -> Vec<PathBuf> {
    [".git", ".zagens", ".agents", ".deepseek"]
        .into_iter()
        .map(|name| root.join(name))
        .collect()
}

/// Build `cmd /C <user command>` argv for enforced unelevated spawn (aligned with G0 PoC).
fn enforced_shell_argv(program: &str, args: &[String]) -> Vec<String> {
    let user_command = extract_shell_user_command(program, args).unwrap_or_else(|| {
        if args.is_empty() {
            program.to_string()
        } else {
            format!("{program} {}", args.join(" "))
        }
    });
    let user_command = harden_cmd_user_command(&user_command);
    vec!["cmd".to_string(), "/C".to_string(), user_command]
}

/// Quote bare `C:\…` tokens so `cmd /C type C:\secret` matches G0 PoC (`type "C:\secret"`).
///
/// Skips quoting for: executable paths (`.exe`/`.com`/…), redirect targets (`> file`).
fn harden_cmd_user_command(command: &str) -> String {
    let mut out = String::new();
    let mut prev: Option<&str> = None;
    for (i, token) in command.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let quote = should_quote_bare_path_token(token, prev);
        if quote {
            out.push('"');
            out.push_str(token);
            out.push('"');
        } else {
            out.push_str(token);
        }
        prev = Some(token);
    }
    out
}

fn should_quote_bare_path_token(token: &str, prev: Option<&str>) -> bool {
    if !is_bare_windows_path_token(token) {
        return false;
    }
    if is_likely_executable_path(token) {
        return false;
    }
    if prev.is_some_and(is_cmd_redirect_operator) {
        return false;
    }
    true
}

fn is_cmd_redirect_operator(token: &str) -> bool {
    matches!(token, ">" | ">>" | "<" | "2>" | "2>>" | ">&" | "2>&1" | "|")
}

fn is_likely_executable_path(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    lower.ends_with(".exe")
        || lower.ends_with(".com")
        || lower.ends_with(".bat")
        || lower.ends_with(".cmd")
}

fn is_bare_windows_path_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && bytes[2] == b'\\'
        && !token.starts_with('"')
        && !token.ends_with('"')
}

fn extract_shell_user_command(program: &str, args: &[String]) -> Option<String> {
    let prog = program.to_ascii_lowercase();
    if matches!(prog.as_str(), "powershell" | "pwsh")
        && args
            .first()
            .is_some_and(|a| a.eq_ignore_ascii_case("-Command"))
    {
        return Some(args[1..].join(" "));
    }
    if prog == "cmd" && args.first().is_some_and(|a| a.eq_ignore_ascii_case("/C")) {
        return Some(args[1..].join(" "));
    }
    if prog == "sh" && args.first().is_some_and(|a| a == "-c") {
        return Some(args[1..].join(" "));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_exec_normalizes_verbatim_cwd_for_cmd_spawn() {
        let plan = plan_exec(PlanInput {
            program: "powershell".into(),
            args: vec!["-Command".into(), "echo ok".into()],
            cwd: PathBuf::from(r"\\?\F:\DeepSeek-TUI-desktop"),
            env: HashMap::new(),
            writable_roots: vec![PathBuf::from(r"F:\DeepSeek-TUI-desktop")],
            protected_write_paths: vec![],
            network_allowed: false,
        })
        .expect("plan");
        assert_eq!(plan.cwd, PathBuf::from(r"F:\DeepSeek-TUI-desktop"));
    }

    #[test]
    fn cmd_spawn_cwd_strips_verbatim_prefix() {
        assert_eq!(
            cmd_spawn_cwd(Path::new(r"\\?\F:\DeepSeek-TUI-desktop")),
            PathBuf::from(r"F:\DeepSeek-TUI-desktop")
        );
        assert_eq!(
            cmd_spawn_cwd(Path::new(r"F:\DeepSeek-TUI-desktop")),
            PathBuf::from(r"F:\DeepSeek-TUI-desktop")
        );
    }

    #[test]
    fn harden_cmd_user_command_quotes_bare_drive_paths() {
        assert_eq!(
            harden_cmd_user_command("type C:\\Users\\alice\\.ssh\\id_rsa"),
            "type \"C:\\Users\\alice\\.ssh\\id_rsa\""
        );
        assert_eq!(
            harden_cmd_user_command(
                "C:\\Windows\\System32\\more.com C:\\Users\\alice\\.ssh\\id_rsa"
            ),
            "C:\\Windows\\System32\\more.com \"C:\\Users\\alice\\.ssh\\id_rsa\""
        );
        assert_eq!(
            harden_cmd_user_command("echo t2-ok > F:\\DeepSeek-TUI-desktop\\g1_probe.txt"),
            "echo t2-ok > F:\\DeepSeek-TUI-desktop\\g1_probe.txt"
        );
        assert_eq!(
            harden_cmd_user_command("type \"C:\\Users\\alice\\.ssh\\id_rsa\""),
            "type \"C:\\Users\\alice\\.ssh\\id_rsa\""
        );
        assert_eq!(harden_cmd_user_command("echo g1-ok"), "echo g1-ok");
    }

    #[test]
    fn enforced_argv_quotes_unquoted_type_paths() {
        let argv = enforced_shell_argv(
            "powershell",
            &[
                "-Command".into(),
                "type C:\\Users\\alice\\.ssh\\id_rsa".into(),
            ],
        );
        assert_eq!(argv[2], "type \"C:\\Users\\alice\\.ssh\\id_rsa\"");
    }

    #[test]
    fn enforced_argv_wraps_powershell_command_with_cmd() {
        let argv = enforced_shell_argv("powershell", &["-Command".into(), "echo g1-ok".into()]);
        assert_eq!(
            argv,
            vec![
                "cmd".to_string(),
                "/C".to_string(),
                "echo g1-ok".to_string(),
            ]
        );
    }

    #[test]
    fn enforced_argv_preserves_cmd_c_payload() {
        let argv = enforced_shell_argv(
            "cmd",
            &[
                "/C".into(),
                "type \"C:\\Users\\alice\\.ssh\\id_rsa\"".into(),
            ],
        );
        assert_eq!(argv[0], "cmd");
        assert_eq!(argv[1], "/C");
        assert_eq!(argv[2], "type \"C:\\Users\\alice\\.ssh\\id_rsa\"");
    }
}
