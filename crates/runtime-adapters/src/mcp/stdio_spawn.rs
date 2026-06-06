//! Helpers for spawning MCP stdio servers reliably across platforms.
//!
//! GUI-launched runtimes (Zagens sidecar) often inherit a stripped `PATH` without
//! Node/npm. We augment `PATH` with common install locations and resolve `npx` /
//! `node` to a concrete executable (on Windows, `.cmd` shims).

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::process::Command;

/// Build a configured [`Command`] for an MCP stdio transport child.
pub fn build_stdio_command(
    command: &str,
    args: &[String],
    extra_env: &HashMap<String, String>,
) -> Result<Command> {
    let program = resolve_stdio_program(command)
        .with_context(|| format!("MCP stdio program not found: {command:?}"))?;

    let (program, spawn_args) = wrap_cmd_shim(&program, args);
    let mut cmd = Command::new(&program);
    cmd.args(&spawn_args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    cmd.env("PATH", augmented_path());
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    Ok(cmd)
}

/// Resolve `command` to an executable path when possible.
fn resolve_stdio_program(command: &str) -> Result<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty MCP stdio command");
    }

    let path = Path::new(trimmed);
    if path.is_absolute() || trimmed.contains(['/', '\\']) {
        if path.is_file() {
            return Ok(trimmed.to_string());
        }
        anyhow::bail!("MCP stdio command path does not exist: {trimmed}");
    }

    if let Some(resolved) = resolve_on_path(trimmed) {
        return Ok(resolved);
    }

    // Last resort: let the OS try (e.g. bare name on Unix with POSIX PATH lookup).
    Ok(trimmed.to_string())
}

fn resolve_on_path(name: &str) -> Option<String> {
    let path_dirs = augmented_path();
    let extensions = executable_extensions(name);
    for dir in std::env::split_paths(&path_dirs) {
        for ext in &extensions {
            let candidate = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{name}{ext}"))
            };
            if candidate.is_file() {
                return Some(candidate.to_string_lossy().into_owned());
            }
        }
    }
    None
}

fn executable_extensions(name: &str) -> Vec<String> {
    if name.contains('.') && !name.ends_with('.') {
        return vec![String::new()];
    }
    #[cfg(windows)]
    {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
        let mut exts: Vec<String> = pathext
            .split(';')
            .filter_map(|e| {
                let e = e.trim();
                (!e.is_empty()).then(|| e.to_string())
            })
            .collect();
        exts.push(String::new());
        exts
    }
    #[cfg(not(windows))]
    {
        vec![String::new()]
    }
}

/// `PATH` plus well-known Node/npm install dirs (prepended when missing).
fn augmented_path() -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for extra in extra_path_dirs() {
        let normalized = extra.to_string_lossy().to_string();
        if extra.is_dir() && seen.insert(normalized) {
            dirs.push(extra);
        }
    }

    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let normalized = dir.to_string_lossy().to_string();
            if seen.insert(normalized) {
                dirs.push(dir);
            }
        }
    }

    std::env::join_paths(dirs).unwrap_or_else(|_| {
        std::env::var_os("PATH").unwrap_or_else(|| OsString::from(""))
    })
}

/// On Windows, `.cmd` / `.bat` shims (e.g. `npx.cmd`) must run under `cmd.exe /C`.
fn wrap_cmd_shim(program: &str, args: &[String]) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        let lower = program.to_lowercase();
        if lower.ends_with(".cmd") || lower.ends_with(".bat") {
            let mut spawn_args = vec!["/C".to_string(), program.to_string()];
            spawn_args.extend(args.iter().cloned());
            return ("cmd.exe".to_string(), spawn_args);
        }
    }
    (program.to_string(), args.to_vec())
}

fn extra_path_dirs() -> Vec<PathBuf> {
    let mut out = Vec::new();
    #[cfg(windows)]
    {
        if let Ok(pf) = std::env::var("ProgramFiles") {
            out.push(PathBuf::from(pf).join("nodejs"));
        }
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            out.push(PathBuf::from(pf86).join("nodejs"));
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            out.push(PathBuf::from(appdata).join("npm"));
        }
        if let Ok(nvm_home) = std::env::var("NVM_HOME") {
            out.push(PathBuf::from(nvm_home));
        }
        if let Ok(nvm_link) = std::env::var("NVM_SYMLINK") {
            out.push(PathBuf::from(nvm_link));
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);
        out.push(home.join(".local").join("bin"));
        out.push(home.join(".cargo").join("bin"));
        out.push(home.join(".fnm").join("current").join("bin"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(&local);
        out.push(local.join("Programs").join("fnm"));
        out.push(local.join("Programs").join("uv"));
        out.push(local.join("uv").join("bin"));
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        out.push(PathBuf::from(&userprofile).join(".local").join("bin"));
        out.push(PathBuf::from(&userprofile).join(".cargo").join("bin"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn augmented_path_includes_nodejs_on_windows_when_present() {
        #[cfg(windows)]
        {
            let path = augmented_path();
            let path_str = path.to_string_lossy().to_lowercase();
            if PathBuf::from(r"C:\Program Files\nodejs").is_dir() {
                assert!(
                    path_str.contains("nodejs"),
                    "expected nodejs dir in augmented PATH: {path_str}"
                );
            }
        }
    }

    #[test]
    fn resolve_npx_when_nodejs_installed() {
        #[cfg(windows)]
        {
            if !PathBuf::from(r"C:\Program Files\nodejs\npx.cmd").is_file() {
                return;
            }
            let resolved = resolve_on_path("npx").expect("npx should resolve");
            assert!(
                resolved.to_lowercase().ends_with("npx.cmd"),
                "got {resolved}"
            );
        }
    }
}
