//! Dynamic `exec_shell` tool description (OpenCode-style host/shell truth for the model).

use std::sync::OnceLock;

/// Cached model-visible description; pinned on first read (matches `ToolRegistry::api_cache`).
static DESCRIPTION: OnceLock<String> = OnceLock::new();

/// Which shell family the runtime will use for `CommandSpec::shell` / `exec_shell`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActiveShell {
    Pwsh,
    PowerShell51,
    Cmd,
    PosixSh,
}

impl ActiveShell {
    fn label(self) -> &'static str {
        match self {
            Self::Pwsh => "pwsh (PowerShell 7+)",
            Self::PowerShell51 => "powershell (Windows PowerShell 5.1)",
            Self::Cmd => "cmd",
            Self::PosixSh => "sh",
        }
    }

    fn short_name(self) -> &'static str {
        match self {
            Self::Pwsh => "pwsh",
            Self::PowerShell51 => "powershell",
            Self::Cmd => "cmd",
            Self::PosixSh => "sh",
        }
    }
}

/// Detect the shell family used for agent `exec_shell` on this host.
#[must_use]
pub(crate) fn detect_active_shell() -> ActiveShell {
    detect_active_shell_for(None)
}

/// Detect the shell family for a given agent.shell config preference.
#[must_use]
pub(crate) fn detect_active_shell_for(shell_preference: Option<&str>) -> ActiveShell {
    #[cfg(windows)]
    {
        let (program, _) = crate::sandbox::windows_shell_for(shell_preference);
        active_shell_from_program(program)
    }
    #[cfg(not(windows))]
    {
        let _ = shell_preference;
        ActiveShell::PosixSh
    }
}

fn active_shell_from_program(program: &str) -> ActiveShell {
    let lower = program.to_ascii_lowercase();
    if lower == "pwsh" || lower.ends_with("\\pwsh.exe") || lower.ends_with("/pwsh.exe") {
        ActiveShell::Pwsh
    } else if lower == "powershell"
        || lower.ends_with("\\powershell.exe")
        || lower.ends_with("/powershell.exe")
    {
        ActiveShell::PowerShell51
    } else {
        // Explicit cmd / cmd.exe and unknown Windows shells fall back to Cmd.
        ActiveShell::Cmd
    }
}

/// Render the full description for the detected host shell (used in production).
#[must_use]
pub fn render_exec_shell_description() -> String {
    render_for_shell(std::env::consts::OS, detect_active_shell())
}

/// Render the full description for a given agent.shell config preference.
#[must_use]
pub fn render_exec_shell_description_for(shell_preference: Option<&str>) -> String {
    render_for_shell(
        std::env::consts::OS,
        detect_active_shell_for(shell_preference),
    )
}

/// Render for a specific OS + shell (tests and future config overrides).
#[must_use]
pub(crate) fn render_for_shell(host_os: &str, shell: ActiveShell) -> String {
    let mut lines = vec![
        "Execute a shell command in the workspace directory.".to_string(),
        format!(
            "Be aware: Host OS: {host_os}, Active shell: {} (must match actual spawn).",
            shell.label()
        ),
        "Use parameter `cwd` to set the working directory; do NOT use `cd && …` in the command string."
            .to_string(),
        "For file search/read/edit use glob_files, grep_files, read_file, edit_file — not shell find/grep/cat/findstr/dir."
            .to_string(),
        "Do NOT pipe through `head`, `tail`, or `Select-Object -First` to trim output — the tool truncates at 30KB and spills the full log to `.zagens/shell-output/` when needed; use read_file or grep_files on the spill path."
            .to_string(),
        "Long-running commands: set background=true and poll with exec_shell_wait or task_shell_wait."
            .to_string(),
        "`task_id` returned from background exec_shell is a poll handle for exec_shell_wait — not a filesystem path."
            .to_string(),
        "Foreground mode is for bounded commands (default timeout 120000ms). External sandbox backends: no background/interactive/tty."
            .to_string(),
    ];

    match shell {
        ActiveShell::Pwsh => {
            lines.push(
                "PowerShell 7+ rules: you may use `&&`/`||` for dependent chains; quote paths with spaces via `& \"path\"`."
                    .to_string(),
            );
            lines.push(
                "Do NOT use cmd-style `%VAR%` or `%CD%`; use `$env:VAR` and `$pwd` (or Get-Location)."
                    .to_string(),
            );
            lines.push(
                "Do NOT use bash syntax (mkdir -p, export FOO=, [[ … ]], bare `if [ … ]`)."
                    .to_string(),
            );
        }
        ActiveShell::PowerShell51 => {
            lines.push(
                "Windows PowerShell 5.1 rules: do NOT rely on `&&`; chain with `cmd1; if ($?) { cmd2 }`."
                    .to_string(),
            );
            lines.push(
                "Do NOT use cmd-style `%VAR%` or `%CD%`; use `$env:VAR` and `$pwd` (or Get-Location)."
                    .to_string(),
            );
            lines.push(
                "Do NOT use bash syntax (mkdir -p, export FOO=, [[ … ]], bare `if [ … ]`)."
                    .to_string(),
            );
            lines.push(
                "Prefer cmdlets (New-Item, Get-ChildItem) over bash-isms; quote paths: `& \"C:\\Program Files\\…\"`."
                    .to_string(),
            );
        }
        ActiveShell::Cmd => {
            lines.push(
                "cmd.exe rules: use `%VAR%`, `if exist`, double-quote paths with spaces; no bash export/`&&` unless using cmd chaining."
                    .to_string(),
            );
            lines.push(
                "Do NOT use bash or PowerShell syntax unless you know cmd accepts it.".to_string(),
            );
        }
        ActiveShell::PosixSh => {
            lines.push(
                "POSIX sh rules: `&&` chains OK; do NOT assume bash-only features (arrays, `{a,b}` globs) unless /bin/bash is confirmed."
                    .to_string(),
            );
        }
    }

    lines.join("\n")
}

/// Pre-initialize the cached tool description with a configured shell preference.
/// Call before the first `exec_shell_tool_description()` access to reflect agent.shell config.
pub fn init_exec_shell_tool_description(shell_preference: Option<&str>) {
    let desc = render_exec_shell_description_for(shell_preference);
    let _ = DESCRIPTION.set(desc); // no-op if already set
}

/// Model-visible `exec_shell` description; initialized once per process.
#[must_use]
pub fn exec_shell_tool_description() -> &'static str {
    DESCRIPTION
        .get_or_init(render_exec_shell_description)
        .as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_includes_host_os_and_active_shell() {
        let shell = detect_active_shell();
        let text = render_for_shell(std::env::consts::OS, shell);
        assert!(
            text.contains("Be aware: Host OS:"),
            "missing OS line: {text}"
        );
        assert!(
            text.contains(&format!("Active shell: {}", shell.label())),
            "missing shell label for {:?}: {text}",
            shell
        );
    }

    #[test]
    fn pwsh_profile_allows_ampersand_chain_hint() {
        let text = render_for_shell("windows", ActiveShell::Pwsh);
        assert!(text.contains("&&"));
        assert!(!text.contains("do NOT rely on `&&`"));
    }

    #[test]
    fn pwsh_profile_forbids_cmd_percent_vars() {
        let text = render_for_shell("windows", ActiveShell::Pwsh);
        assert!(text.contains("%VAR%"));
        assert!(text.contains("$env:VAR"));
        assert!(text.contains("task_id"));
        assert!(text.contains("poll handle"));
    }

    #[test]
    fn powershell51_profile_forbids_bare_and_chain() {
        let text = render_for_shell("windows", ActiveShell::PowerShell51);
        assert!(text.contains("do NOT rely on `&&`"));
        assert!(text.contains("if ($?)"));
    }

    #[test]
    fn cmd_profile_mentions_cmd_rules() {
        let text = render_for_shell("windows", ActiveShell::Cmd);
        assert!(text.contains("cmd.exe rules"));
    }

    #[test]
    fn posix_profile_mentions_sh() {
        let text = render_for_shell("linux", ActiveShell::PosixSh);
        assert!(text.contains("Active shell: sh"));
        assert!(text.contains("POSIX sh rules"));
    }

    #[test]
    fn cached_description_matches_render() {
        let cached = exec_shell_tool_description();
        assert_eq!(cached, render_exec_shell_description());
    }

    #[test]
    fn active_shell_from_program_maps_pwsh_and_powershell() {
        assert_eq!(active_shell_from_program("pwsh"), ActiveShell::Pwsh);
        assert_eq!(
            active_shell_from_program("powershell"),
            ActiveShell::PowerShell51
        );
        assert_eq!(active_shell_from_program("cmd"), ActiveShell::Cmd);
    }

    #[cfg(windows)]
    #[test]
    fn detected_shell_matches_windows_shell_program() {
        let (program, _) = crate::sandbox::windows_shell();
        let detected = detect_active_shell();
        assert_eq!(
            detected.short_name(),
            active_shell_from_program(program).short_name()
        );
    }
}
