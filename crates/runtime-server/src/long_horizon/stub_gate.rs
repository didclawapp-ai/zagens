//! Task-agnostic stub / incompleteness scan (generic completion gate layer).
//!
//! Catches the most common false-completion shape — "the project compiles, but
//! a feature is still a stub" — without any per-task manifest. We scan tracked
//! source files for **high-signal** "intentionally unfinished" markers
//! (`todo!()` / `unimplemented!()` / `NotImplementedError` / a "not implemented"
//! throw/panic/raise). These are the markers that compile cleanly yet mean the
//! code path does nothing, so a green build can still hide them.
//!
//! Plain `TODO` / `FIXME` comments are recorded too, but classified as
//! **non-blocking**: they are far too common in real code to justify an enforce
//! block (see the 2026 "verification backfires when ceremonial" findings). Only
//! blocking-class hits gate the turn in `enforce`; everything is surfaced in
//! `observe` for "先量后调".

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

/// What kind of marker a hit matched. Only [`StubKind::is_blocking`] kinds gate
/// the turn in enforce mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StubKind {
    /// `unimplemented!()` / `todo!()` Rust macro — compiles, panics at runtime.
    UnfinishedMacro,
    /// `NotImplementedError` / `raise NotImplementedError` (Python et al.).
    NotImplementedError,
    /// A `throw` / `panic!` / `raise` / `return` of a "not implemented" sentinel.
    NotImplementedThrow,
    /// Bare `TODO` / `FIXME` comment — recorded, never blocks.
    TodoComment,
}

impl StubKind {
    #[must_use]
    pub fn is_blocking(self) -> bool {
        !matches!(self, StubKind::TodoComment)
    }

    fn label(self) -> &'static str {
        match self {
            StubKind::UnfinishedMacro => "unfinished_macro",
            StubKind::NotImplementedError => "not_implemented_error",
            StubKind::NotImplementedThrow => "not_implemented_throw",
            StubKind::TodoComment => "todo_comment",
        }
    }
}

/// One stub marker occurrence.
#[derive(Debug, Clone)]
pub struct StubHit {
    pub file: String,
    pub line: u32,
    pub kind: StubKind,
    pub snippet: String,
}

/// Result of a workspace stub scan.
#[derive(Debug, Clone, Default)]
pub struct StubScanResult {
    pub hits: Vec<StubHit>,
}

impl StubScanResult {
    /// Hits that gate the turn in enforce mode (blocking-class only).
    #[must_use]
    pub fn blocking(&self) -> Vec<&StubHit> {
        self.hits.iter().filter(|h| h.kind.is_blocking()).collect()
    }

    #[must_use]
    pub fn todo_count(&self) -> usize {
        self.hits
            .iter()
            .filter(|h| h.kind == StubKind::TodoComment)
            .count()
    }
}

const MAX_FILES: usize = 4_000;
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_HITS: usize = 60;
const SNIPPET_MAX: usize = 160;

/// Directories never worth scanning (deps, build output, VCS).
const SKIP_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".gradle",
    "vendor",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    "coverage",
    "Pods",
    "DerivedData",
    ".idea",
    ".vscode",
];

/// Source extensions worth scanning. Keeps the scan bounded and avoids matching
/// markers inside docs / lockfiles / generated assets.
const CODE_EXTS: &[&str] = &[
    "rs", "go", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "java", "kt", "kts", "c", "h", "cpp",
    "cc", "cxx", "hpp", "cs", "rb", "php", "swift", "scala", "vue", "svelte", "dart", "m", "mm",
];

// `unimplemented!(` / `todo!(` — Rust unfinished macros (compile, panic at runtime).
static MACRO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(unimplemented|todo)!\s*\(").expect("MACRO_RE"));
// `NotImplementedError` (Python / others).
static NOT_IMPL_ERR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\bNotImplementedError\b").expect("NOT_IMPL_ERR_RE"));
// A throw/panic/raise/return carrying a "not implemented" sentinel — language
// agnostic. Requires the keyword context so a doc comment "…if not implemented"
// does not block.
static NOT_IMPL_THROW_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(throw|panic!?|raise|return|reject)\b[^\n]{0,48}\bnot[\s_-]?implemented\b")
        .expect("NOT_IMPL_THROW_RE")
});
// Bare TODO / FIXME comment markers (uppercase by convention → fewer FPs).
static TODO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(TODO|FIXME)\b").expect("TODO_RE"));

fn classify_line(line: &str) -> Option<StubKind> {
    if MACRO_RE.is_match(line) {
        return Some(StubKind::UnfinishedMacro);
    }
    if NOT_IMPL_ERR_RE.is_match(line) {
        return Some(StubKind::NotImplementedError);
    }
    if NOT_IMPL_THROW_RE.is_match(line) {
        return Some(StubKind::NotImplementedThrow);
    }
    if TODO_RE.is_match(line) {
        return Some(StubKind::TodoComment);
    }
    None
}

fn has_code_ext(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| CODE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Paths that are harness infrastructure, not the task artifact — skip during
/// stub scan so dogfooding on the Zagens monorepo does not self-block on LHT
/// source markers (e.g. `unfinished_macro` in comments/strings).
fn is_harness_infra_path(workspace: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(workspace) else {
        return false;
    };
    let rel = rel.to_string_lossy().replace('\\', "/");
    const SKIP_PREFIXES: &[&str] = &[
        "crates/runtime-server/src/long_horizon/",
        "crates/desktop/binaries/",
        "crates/desktop/web-ui/src/i18n/locales/",
        "third-party/",
        ".cursor/",
    ];
    SKIP_PREFIXES.iter().any(|p| rel.starts_with(p))
}

fn snippet(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() <= SNIPPET_MAX {
        t.to_string()
    } else {
        let cut: String = t.chars().take(SNIPPET_MAX).collect();
        format!("{cut}…")
    }
}

/// Scan `workspace` (recursively, skipping dep/build dirs) for stub markers.
/// Bounded by [`MAX_FILES`] / [`MAX_HITS`]; non-UTF-8 and oversized files are
/// skipped. Synchronous + filesystem-only — callers should run it off the async
/// runtime (e.g. `spawn_blocking`).
#[must_use]
pub fn scan_workspace_stubs(workspace: &Path) -> StubScanResult {
    let mut result = StubScanResult::default();
    let mut files_scanned = 0usize;
    let mut stack: Vec<std::path::PathBuf> = vec![workspace.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                let skip = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| SKIP_DIRS.contains(&n) || n.starts_with('.') && n != ".")
                    .unwrap_or(false);
                if !skip {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file()
                || !has_code_ext(&path)
                || is_harness_infra_path(workspace, &path)
            {
                continue;
            }
            if files_scanned >= MAX_FILES {
                return result;
            }
            files_scanned += 1;
            if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_FILE_BYTES {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(workspace)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            for (idx, line) in content.lines().enumerate() {
                if let Some(kind) = classify_line(line) {
                    result.hits.push(StubHit {
                        file: rel.clone(),
                        line: (idx as u32).saturating_add(1),
                        kind,
                        snippet: snippet(line),
                    });
                    if result.hits.len() >= MAX_HITS {
                        return result;
                    }
                }
            }
        }
    }
    result
}

/// Compact JSON-ish telemetry payload for a scan (kind counts + a small sample).
#[must_use]
pub fn telemetry_payload(result: &StubScanResult, mode: &str) -> String {
    let blocking = result.blocking();
    let sample: Vec<String> = blocking
        .iter()
        .take(5)
        .map(|h| format!("{}:{} {}", h.file, h.line, h.kind.label()))
        .collect();
    format!(
        "{{\"mode\":\"{mode}\",\"blocking\":{},\"todo\":{},\"total\":{},\"sample\":{}}}",
        blocking.len(),
        result.todo_count(),
        result.hits.len(),
        serde_json::to_string(&sample).unwrap_or_else(|_| "[]".to_string())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn detects_high_signal_blocking_stubs() {
        let dir = std::env::temp_dir().join(format!("lht-stub-hi-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        write(
            &dir,
            "src/license.rs",
            "pub fn verify() -> bool {\n    todo!()\n}\n",
        );
        write(
            &dir,
            "src/mes.rs",
            "fn sync() { unimplemented!(\"MES sync\") }\n",
        );
        write(
            &dir,
            "api/printer.py",
            "def print_zpl():\n    raise NotImplementedError\n",
        );
        write(
            &dir,
            "web/tray.ts",
            "export function tray() { throw new Error('not implemented'); }\n",
        );
        let r = scan_workspace_stubs(&dir);
        let blocking = r.blocking();
        assert_eq!(blocking.len(), 4, "all four high-signal stubs are blocking");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn todo_comments_are_recorded_but_not_blocking() {
        let dir = std::env::temp_dir().join(format!("lht-stub-todo-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        write(
            &dir,
            "src/lib.rs",
            "// TODO: optimize this later\nfn ok() -> u8 { 1 }\n",
        );
        let r = scan_workspace_stubs(&dir);
        assert_eq!(r.todo_count(), 1);
        assert!(r.blocking().is_empty(), "a bare TODO must never block");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn skips_dep_and_build_dirs_and_non_code() {
        let dir = std::env::temp_dir().join(format!("lht-stub-skip-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Stub markers inside skipped dirs / non-code files must not be counted.
        write(
            &dir,
            "node_modules/pkg/index.js",
            "throw new Error('not implemented')\n",
        );
        write(&dir, "target/debug/gen.rs", "todo!()\n");
        write(&dir, "README.md", "This feature is not implemented yet.\n");
        let r = scan_workspace_stubs(&dir);
        assert!(r.hits.is_empty(), "deps/build/docs are out of scope");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_project_has_no_hits() {
        let dir = std::env::temp_dir().join(format!("lht-stub-clean-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        write(&dir, "src/main.rs", "fn main() { println!(\"hi\"); }\n");
        let r = scan_workspace_stubs(&dir);
        assert!(r.hits.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
