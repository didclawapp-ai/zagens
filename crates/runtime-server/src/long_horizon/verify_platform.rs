//! Cross-platform native execution for common `[verify:]` shell patterns.
//!
//! Models often declare Unix-only probes (`grep`, `rg`, `test -d`) that fail on
//! Windows with `exit_class: infra` even when the codebase is fine. The manifest
//! gate runs these in-process (no external `grep`/`bash`) so layer-2 oracle
//! semantics match the operator's intent on every OS.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use regex::Regex;

/// Per-verify exit classification (mirrors manifest_gate; kept local to avoid cycles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeExitClass {
    Ok,
    Assertion,
    Infra,
}

/// Outcome of an in-process verify probe.
#[derive(Debug, Clone)]
pub struct NativeVerifyResult {
    pub command_display: String,
    pub exit_code: i32,
    pub exit_class: NativeExitClass,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

struct NativeOutcome {
    exit_code: i32,
    stdout: String,
    stderr: String,
    exit_class: NativeExitClass,
}

/// True when the pattern looks like a "zero stubs" LHT acceptance probe.
fn is_stub_absence_pattern(pattern: &str) -> bool {
    let lower = pattern.to_ascii_lowercase();
    const MARKERS: &[&str] = &[
        "not_impl",
        "todo!",
        "unimplemented!",
        "unimplemented",
        "not implemented",
        "notimplemented",
    ];
    MARKERS.iter().any(|m| lower.contains(m))
}

/// Run a checklist verify command in-process when we recognize the pattern.
#[must_use]
pub fn try_native_verify(workspace: &Path, command: &str) -> Option<NativeVerifyResult> {
    let (root, rest) = strip_cd_prefix(command.trim());
    let root = root.as_deref().map(|d| workspace.join(d)).unwrap_or_else(|| workspace.to_path_buf());

    if let Some(out) = try_grep_like(&root, rest) {
        return Some(to_result("native_grep", command, out));
    }
    if let Some(out) = try_test_directory(&root, rest) {
        return Some(to_result("native_test", command, out));
    }
    None
}

fn to_result(id_suffix: &str, display: &str, out: NativeOutcome) -> NativeVerifyResult {
    NativeVerifyResult {
        command_display: format!("{display} [harness:{id_suffix}]"),
        exit_code: out.exit_code,
        exit_class: out.exit_class,
        stdout_tail: out.stdout,
        stderr_tail: out.stderr,
    }
}

/// Optional PowerShell rewrite for patterns we do not execute natively (fallback).
#[must_use]
pub fn adapt_verify_command_for_platform(command: &str) -> Cow<'_, str> {
    #[cfg(windows)]
    {
        let trimmed = command.trim();
        if trimmed.starts_with("bash ") || trimmed.starts_with("/bin/bash ") {
            return Cow::Owned(trimmed.replacen("bash ", "bash -lc ", 1));
        }
    }
    Cow::Borrowed(command)
}

/// Extra normalized command forms that should satisfy `[verify:]` replay matching.
#[must_use]
pub fn verify_command_equivalents(command: &str) -> Vec<String> {
    use super::verify::normalize_cmd;
    let mut out = vec![normalize_cmd(command)];
    let (_, rest) = strip_cd_prefix(command.trim());
    if parse_grep_like(rest).is_some() || parse_test_directory(rest).is_some() {
        let native_tag = normalize_cmd(&format!("{rest} [harness:native]"));
        if !out.contains(&native_tag) {
            out.push(native_tag);
        }
    }
    out
}

fn strip_cd_prefix(command: &str) -> (Option<PathBuf>, &str) {
    let Some(rest) = command.strip_prefix("cd ") else {
        return (None, command);
    };
    let Some((dir, tail)) = rest.split_once("&&") else {
        return (None, command);
    };
    let dir = dir.trim().trim_matches('"').trim_matches('\'');
    if dir.is_empty() {
        return (None, command);
    }
    (Some(PathBuf::from(dir)), tail.trim())
}

fn try_grep_like(workspace: &Path, command: &str) -> Option<NativeOutcome> {
    let spec = parse_grep_like(command)?;
    run_pattern_file_probe(workspace, &spec)
}

fn try_test_directory(workspace: &Path, command: &str) -> Option<NativeOutcome> {
    let spec = parse_test_directory(command)?;
    let path = workspace.join(&spec.path);
    let exists = path.is_dir();
    let want_exists = spec.want_exists;
    let ok = exists == want_exists;
    Some(NativeOutcome {
        exit_code: if ok { 0 } else { 1 },
        stdout: if ok {
            format!(
                "directory `{}` {} as expected",
                spec.path,
                if exists { "exists" } else { "absent" }
            )
        } else {
            String::new()
        },
        stderr: if ok {
            String::new()
        } else {
            format!(
                "directory `{}` {} (expected {})",
                spec.path,
                if exists { "exists" } else { "missing" },
                if want_exists { "present" } else { "absent" }
            )
        },
        exit_class: if ok {
            NativeExitClass::Ok
        } else {
            NativeExitClass::Assertion
        },
    })
}

struct GrepLikeSpec {
    pattern: String,
    file: String,
    count_mode: bool,
    absence_ok: bool,
}

struct TestDirSpec {
    path: String,
    want_exists: bool,
}

fn parse_grep_like(command: &str) -> Option<GrepLikeSpec> {
    let tokens = tokenize_command(command);
    if tokens.is_empty() {
        return None;
    }
    let tool = tokens[0].to_ascii_lowercase();
    if tool != "grep" && tool != "rg" {
        return None;
    }
    let mut count_mode = false;
    let mut i = 1usize;
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "-c" || t == "--count" {
            count_mode = true;
            i += 1;
            continue;
        }
        if t == "-q" || t == "--quiet" {
            count_mode = false;
            i += 1;
            continue;
        }
        if t.starts_with('-') {
            i += 1;
            continue;
        }
        break;
    }
    if i + 1 >= tokens.len() {
        return None;
    }
    let pattern = tokens[i].clone();
    let file = tokens[i + 1].clone();
    let absence_ok = is_stub_absence_pattern(&pattern);
    Some(GrepLikeSpec {
        pattern,
        file,
        count_mode,
        absence_ok,
    })
}

fn parse_test_directory(command: &str) -> Option<TestDirSpec> {
    let tokens = tokenize_command(command);
    if tokens.first().map(String::as_str) != Some("test") {
        return None;
    }
    let mut i = 1usize;
    let mut negated = false;
    if i < tokens.len() && tokens[i] == "!" {
        negated = true;
        i += 1;
    }
    if i >= tokens.len() || tokens[i] != "-d" {
        return None;
    }
    i += 1;
    if i >= tokens.len() {
        return None;
    }
    Some(TestDirSpec {
        path: tokens[i].clone(),
        want_exists: !negated,
    })
}

fn tokenize_command(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for ch in command.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => cur.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            None => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn run_pattern_file_probe(workspace: &Path, spec: &GrepLikeSpec) -> Option<NativeOutcome> {
    let path = workspace.join(&spec.file);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            return Some(NativeOutcome {
                exit_code: 2,
                stdout: String::new(),
                stderr: format!("{}: {e}", path.display()),
                exit_class: NativeExitClass::Infra,
            });
        }
    };
    let count = count_pattern_matches(&spec.pattern, &content);
    let pass = if spec.absence_ok {
        count == 0
    } else if spec.count_mode {
        count > 0
    } else {
        count > 0
    };
    let stdout = if spec.count_mode || spec.absence_ok {
        format!("{count}")
    } else if pass {
        "match".to_string()
    } else {
        String::new()
    };
    Some(NativeOutcome {
        exit_code: if pass { 0 } else { 1 },
        stdout,
        stderr: if pass {
            String::new()
        } else if spec.absence_ok {
            format!(
                "found {count} match(es) for `{}` in {}",
                spec.pattern, spec.file
            )
        } else {
            format!("no matches for `{}` in {}", spec.pattern, spec.file)
        },
        exit_class: if pass {
            NativeExitClass::Ok
        } else {
            NativeExitClass::Assertion
        },
    })
}

fn count_pattern_matches(pattern: &str, content: &str) -> usize {
    if let Ok(re) = Regex::new(pattern) {
        return re.find_iter(content).count();
    }
    content.matches(pattern).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn grep_count_stub_absence_passes_when_zero() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/commands.rs"), "fn ok() {}").unwrap();
        let cmd = "grep -c not_impl src/commands.rs";
        let result = try_native_verify(dir.path(), cmd).unwrap();
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.exit_class, NativeExitClass::Ok);
        assert_eq!(result.stdout_tail, "0");
    }

    #[test]
    fn grep_count_stub_absence_fails_when_present() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("commands.rs"),
            "fn x() { not_impl() }\n",
        )
        .unwrap();
        let result = try_native_verify(dir.path(), "grep -c not_impl commands.rs").unwrap();
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.exit_class, NativeExitClass::Assertion);
    }

    #[test]
    fn rg_regex_pattern_with_cd_prefix() {
        let dir = TempDir::new().unwrap();
        let tauri = dir.path().join("src-tauri");
        fs::create_dir_all(tauri.join("src")).unwrap();
        fs::write(tauri.join("src/commands.rs"), "ok();").unwrap();
        let cmd = "cd src-tauri && rg \"not_impl\\(\\)\" src/commands.rs";
        let result = try_native_verify(dir.path(), cmd).unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_directory_absent_passes() {
        let dir = TempDir::new().unwrap();
        let result = try_native_verify(dir.path(), "test ! -d electron").unwrap();
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_directory_present_fails_when_negated() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("electron")).unwrap();
        let result = try_native_verify(dir.path(), "test ! -d electron").unwrap();
        assert_eq!(result.exit_code, 1);
        assert_eq!(result.exit_class, NativeExitClass::Assertion);
    }

    #[test]
    fn non_grep_command_not_handled() {
        assert!(try_native_verify(Path::new("."), "cargo check").is_none());
    }

    #[test]
    fn stub_marker_detection() {
        assert!(is_stub_absence_pattern("not_impl"));
        assert!(is_stub_absence_pattern("todo!()"));
        assert!(!is_stub_absence_pattern("invoke("));
    }
}
