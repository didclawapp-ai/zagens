//! Task-agnostic layer-2 verify sources (generic completion gate).
//!
//! Unlike the operator manifest (per-task, hand-authored), these two sources
//! need **zero per-task config** — one global switch covers all code tasks:
//!
//! 1. **Model `[verify:]` replay** — at `graph_complete` the harness actively
//!    re-runs the verify commands the model *itself* declared on its completed
//!    checklist items, turning "claimed but maybe never ran" into an exit-code
//!    oracle. No new trust surface: these commands are already within the
//!    model's existing exec scope.
//! 2. **Toolchain gate** — detect the project's build system from marker files
//!    and run its canonical build/test (`go build && go test`, `cargo build &&
//!    cargo test`, …). Built-in commands, no manifest required.
//!
//! Both feed the same harness-active executor as the operator manifest
//! ([`super::manifest_gate::run_manifest_gate`]); provenance is tracked via
//! [`VerifySource`] so the flow can apply the right enforce/observe mode and
//! surface per-source counts in telemetry.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use deepseek_core::long_horizon::{
    CompletionGateVerifyEntry, GenericGateMode, ManifestShell, VerifySource,
};

use crate::tools::todo::{TodoListSnapshot, TodoStatus};

use super::verify::{normalize_cmd, parse_verify_command};

/// Generic gates may shell out to full build/test runs — give them more head
/// room than the per-command operator default.
const GENERIC_TIMEOUT_SECS: u32 = 600;

/// Build-system marker files that identify a project root.
const PROJECT_MARKERS: &[&str] = &[
    "go.mod",
    "Cargo.toml",
    "package.json",
    "pyproject.toml",
    "setup.py",
    "pom.xml",
    "build.gradle",
    "build.gradle.kts",
];

/// Resolve nested Rust backend root for polyglot layouts (npm/vue frontend at
/// workspace root + Cargo backend in `src-tauri/` or a unique depth-1 child).
#[must_use]
pub fn nested_rust_backend_root(workspace: &Path) -> Option<PathBuf> {
    if workspace.join("Cargo.toml").exists() {
        return None;
    }
    let tauri = workspace.join("src-tauri");
    if tauri.join("Cargo.toml").exists() {
        return Some(tauri);
    }
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return None;
    };
    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("Cargo.toml").exists() {
            matches.push(path);
            if matches.len() > 1 {
                return None;
            }
        }
    }
    matches.into_iter().next()
}

/// True when the workspace looks like a polyglot frontend + nested Cargo backend
/// (Tauri/Electron→Tauri class). Toolchain gate should prefer `cargo` probes and
/// skip root `npm test` false-REDs (P1-3).
#[must_use]
pub fn is_polyglot_rust_backend(workspace: &Path) -> bool {
    nested_rust_backend_root(workspace).is_some()
        && (workspace.join("package.json").exists()
            || workspace.join("pnpm-lock.yaml").exists()
            || workspace.join("yarn.lock").exists())
}

/// Resolve the directory the gate should run build/test commands from.
///
/// The harness workspace root is normally the project root, but a model can
/// nest the whole project one level down (e.g. create `microstack/` and drop
/// `go.mod` there). When that happens, running `go build ./...` (or the model's
/// `[verify:]` replay / toolchain gate) from the workspace root fails with "no
/// go.mod" even though the code is fine — a false gate failure.
///
/// Rule: if the workspace root itself has a build marker, use it. Otherwise, if
/// **exactly one** immediate child directory has a marker, that child is the
/// real project root. Bounded to depth 1; ambiguous cases (0 or >1 matching
/// children) fall back to the workspace root unchanged.
#[must_use]
pub fn resolve_project_root(workspace: &Path) -> PathBuf {
    let has_marker = |dir: &Path| PROJECT_MARKERS.iter().any(|m| dir.join(m).exists());
    if has_marker(workspace) {
        return workspace.to_path_buf();
    }
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return workspace.to_path_buf();
    };
    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has_marker(&path) {
            matches.push(path);
            if matches.len() > 1 {
                // Ambiguous — don't guess; keep the workspace root.
                return workspace.to_path_buf();
            }
        }
    }
    match matches.len() {
        1 => matches
            .into_iter()
            .next()
            .unwrap_or_else(|| workspace.to_path_buf()),
        _ => workspace.to_path_buf(),
    }
}

/// Marker files a command's toolchain expects in its working directory.
/// `None` for commands with no directory-bound toolchain (run as-is).
fn command_markers(command: &str) -> Option<&'static [&'static str]> {
    let first = command.trim_start().split_whitespace().next().unwrap_or("");
    // Strip any path prefix and a trailing `.exe` so `./node`, `C:\…\cargo.exe`
    // and bare `cargo` all map to the same verb.
    let verb = first.rsplit(['/', '\\']).next().unwrap_or(first);
    let verb = verb.strip_suffix(".exe").unwrap_or(verb);
    match verb {
        "cargo" | "rustc" => Some(&["Cargo.toml"]),
        "go" | "gofmt" => Some(&["go.mod"]),
        "npm" | "pnpm" | "yarn" | "npx" | "node" => Some(&["package.json"]),
        "pytest" | "python" | "python3" | "uv" | "poetry" => Some(&["pyproject.toml", "setup.py"]),
        "mvn" => Some(&["pom.xml"]),
        "gradle" => Some(&["build.gradle", "build.gradle.kts"]),
        _ => None,
    }
}

/// Resolve the working directory for a **single** verify command by its
/// toolchain marker. Unlike [`resolve_project_root`] (one root for the whole
/// gate), this is per-command, so a polyglot/nested layout works — e.g. an npm
/// frontend at the workspace root with a Cargo backend in `src-tauri/`: the
/// model's `cargo check` replay runs in `src-tauri/` while `npm test` stays at
/// the root. Rule mirrors `resolve_project_root`: marker at the workspace root
/// wins; otherwise a **unique** depth-1 child with the marker is used;
/// ambiguous/none falls back to the workspace root unchanged.
#[must_use]
pub fn resolve_command_root(workspace: &Path, command: &str) -> PathBuf {
    let Some(markers) = command_markers(command) else {
        return workspace.to_path_buf();
    };
    let has = |dir: &Path| markers.iter().any(|m| dir.join(m).exists());
    if has(workspace) {
        return workspace.to_path_buf();
    }
    let Ok(entries) = std::fs::read_dir(workspace) else {
        return workspace.to_path_buf();
    };
    let mut matches: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && has(&path) {
            matches.push(path);
            if matches.len() > 1 {
                return workspace.to_path_buf();
            }
        }
    }
    matches
        .into_iter()
        .next()
        .unwrap_or_else(|| workspace.to_path_buf())
}

/// Extract the model's own `[verify: cmd]` commands from **completed** checklist
/// items (the ones it claims are done). Deduplicated by normalized command so a
/// command listed on several items runs once.
#[must_use]
pub fn collect_model_verify_entries(
    checklist: &TodoListSnapshot,
    mode: GenericGateMode,
) -> Vec<CompletionGateVerifyEntry> {
    if !mode.is_on() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for item in &checklist.items {
        if item.status != TodoStatus::Completed {
            continue;
        }
        let Some(cmd) = parse_verify_command(&item.content) else {
            continue;
        };
        let norm = normalize_cmd(&cmd);
        if norm.is_empty() || !seen.insert(norm) {
            continue;
        }
        out.push(CompletionGateVerifyEntry {
            id: format!("model_verify_{}", item.id),
            cmd: Some(cmd),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: GENERIC_TIMEOUT_SECS,
            source: VerifySource::ModelDeclared,
        });
    }
    out
}

/// Detect the project toolchain from marker files at the workspace root and
/// return its canonical build/test gate commands. Empty when no known
/// toolchain is detected (the gate then simply has no toolchain entries).
#[must_use]
pub fn detect_toolchain_entries(
    workspace: &Path,
    mode: GenericGateMode,
) -> Vec<CompletionGateVerifyEntry> {
    if !mode.is_on() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let exists = |dir: &Path, name: &str| dir.join(name).exists();
    let mut push = |id: &str, cmd: &str| {
        out.push(CompletionGateVerifyEntry {
            id: format!("toolchain_{id}"),
            cmd: Some(cmd.to_string()),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: GENERIC_TIMEOUT_SECS,
            source: VerifySource::Toolchain,
        });
    };

    // P1-3: polyglot Tauri / npm+cargo — prefer backend cargo probes; skip root
    // `npm test` which often fails unrelated to the migration backend.
    if is_polyglot_rust_backend(workspace) {
        push("cargo_check", "cargo check");
        push("cargo_build", "cargo build");
        return out;
    }

    if exists(workspace, "go.mod") {
        push("go_build", "go build ./...");
        // `-cover` so [`go_toolchain_audit`] can enforce a minimum coverage %;
        // plain `go test` exits 0 with all `[no test files]` (false green).
        push("go_test", "go test -cover ./...");
    } else if exists(workspace, "Cargo.toml") {
        push("cargo_build", "cargo build");
        push("cargo_test", "cargo test");
    } else if exists(workspace, "package.json") {
        // `npm test` exits non-zero when there is no test script, which would be
        // a false infra failure; gate on build first, tests are model/operator's
        // job to declare. We still run the build/typecheck if present via test.
        push("npm_test", "npm test --silent");
    } else if exists(workspace, "pyproject.toml") || exists(workspace, "setup.py") {
        push("pytest", "pytest -q");
    } else if exists(workspace, "pom.xml") {
        push("maven_test", "mvn -q -B test");
    } else if exists(workspace, "build.gradle") || exists(workspace, "build.gradle.kts") {
        push("gradle_test", "gradle test -q");
    }
    out
}

/// Merge operator + toolchain + model verify entries into the effective layer-2
/// list, deduplicated by normalized command. Priority on collision:
/// **operator > toolchain > model** (the more authoritative source's entry,
/// including its id/timeout/shell, wins).
#[must_use]
pub fn merge_verify_entries(
    operator: &[CompletionGateVerifyEntry],
    toolchain: Vec<CompletionGateVerifyEntry>,
    model: Vec<CompletionGateVerifyEntry>,
) -> Vec<CompletionGateVerifyEntry> {
    let mut out: Vec<CompletionGateVerifyEntry> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let key = |e: &CompletionGateVerifyEntry| -> String {
        if !e.argv.is_empty() {
            normalize_cmd(&e.argv.join(" "))
        } else {
            normalize_cmd(e.cmd.as_deref().unwrap_or_default())
        }
    };

    for e in operator
        .iter()
        .cloned()
        .chain(toolchain.into_iter())
        .chain(model.into_iter())
    {
        let k = key(&e);
        // Empty-key entries (e.g. argv-only operator entries with odd shape)
        // are kept as-is; only dedup non-empty command keys.
        if !k.is_empty() && !seen.insert(k) {
            continue;
        }
        out.push(e);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::todo::TodoItem;

    fn item(id: u32, content: &str, status: TodoStatus) -> TodoItem {
        TodoItem {
            id,
            content: content.to_string(),
            status,
        }
    }

    fn snapshot(items: Vec<TodoItem>) -> TodoListSnapshot {
        TodoListSnapshot {
            items,
            completion_pct: 100,
            in_progress_id: None,
        }
    }

    #[test]
    fn model_replay_off_returns_empty() {
        let snap = snapshot(vec![item(
            1,
            "[verify: go test ./...] tests pass",
            TodoStatus::Completed,
        )]);
        assert!(collect_model_verify_entries(&snap, GenericGateMode::Off).is_empty());
    }

    #[test]
    fn model_replay_extracts_completed_verify_commands() {
        let snap = snapshot(vec![
            item(
                1,
                "[verify: go test ./...] tests pass",
                TodoStatus::Completed,
            ),
            item(
                2,
                "[verify: go vet ./...] no warnings",
                TodoStatus::Completed,
            ),
            // pending item ignored
            item(3, "[verify: go build ./...] builds", TodoStatus::Pending),
            // untagged ignored
            item(4, "implement handler", TodoStatus::Completed),
        ]);
        let entries = collect_model_verify_entries(&snap, GenericGateMode::Enforce);
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .all(|e| e.source == VerifySource::ModelDeclared)
        );
        assert_eq!(entries[0].cmd.as_deref(), Some("go test ./..."));
    }

    #[test]
    fn model_replay_dedups_same_command() {
        let snap = snapshot(vec![
            item(1, "[verify: go test ./...] a", TodoStatus::Completed),
            item(2, "[verify: go  test  ./...] b", TodoStatus::Completed),
        ]);
        assert_eq!(
            collect_model_verify_entries(&snap, GenericGateMode::Observe).len(),
            1
        );
    }

    #[test]
    fn toolchain_detects_go() {
        let dir = std::env::temp_dir().join(format!("lht-tc-go-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("go.mod"), b"module x\n").unwrap();
        let entries = detect_toolchain_entries(&dir, GenericGateMode::Enforce);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.source == VerifySource::Toolchain));
        assert!(
            entries
                .iter()
                .any(|e| e.cmd.as_deref() == Some("go build ./..."))
        );
        assert!(
            entries
                .iter()
                .any(|e| e.cmd.as_deref() == Some("go test -cover ./..."))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toolchain_off_or_unknown_is_empty() {
        let dir = std::env::temp_dir().join(format!("lht-tc-none-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(detect_toolchain_entries(&dir, GenericGateMode::Enforce).is_empty());
        std::fs::write(dir.join("go.mod"), b"module x\n").unwrap();
        assert!(detect_toolchain_entries(&dir, GenericGateMode::Off).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_root_is_workspace_when_marker_at_root() {
        let dir = std::env::temp_dir().join(format!("lht-pr-root-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("go.mod"), b"module x\n").unwrap();
        assert_eq!(resolve_project_root(&dir), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_root_descends_into_single_nested_module() {
        let dir = std::env::temp_dir().join(format!("lht-pr-nest-{}", std::process::id()));
        let sub = dir.join("microstack");
        let _ = std::fs::create_dir_all(&sub);
        std::fs::write(sub.join("go.mod"), b"module x\n").unwrap();
        // Root has no marker; exactly one child does → descend.
        assert_eq!(resolve_project_root(&dir), sub);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn project_root_stays_at_workspace_when_ambiguous() {
        let dir = std::env::temp_dir().join(format!("lht-pr-amb-{}", std::process::id()));
        let a = dir.join("a");
        let b = dir.join("b");
        let _ = std::fs::create_dir_all(&a);
        let _ = std::fs::create_dir_all(&b);
        std::fs::write(a.join("go.mod"), b"module a\n").unwrap();
        std::fs::write(b.join("Cargo.toml"), b"[package]\n").unwrap();
        // Two candidate children → don't guess, keep workspace root.
        assert_eq!(resolve_project_root(&dir), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_root_descends_to_nested_cargo_under_npm_root() {
        // Polyglot layout: npm frontend at root, Cargo backend in src-tauri/.
        // The exact F:\LHT_TEST\lable_Standalone false-RED: `cargo check` ran at
        // the root (package.json) and could not find Cargo.toml.
        let dir = std::env::temp_dir().join(format!("lht-cr-poly-{}", std::process::id()));
        let backend = dir.join("src-tauri");
        let _ = std::fs::create_dir_all(&backend);
        std::fs::write(dir.join("package.json"), b"{}\n").unwrap();
        std::fs::write(backend.join("Cargo.toml"), b"[package]\n").unwrap();
        // cargo → needs Cargo.toml → descends into src-tauri.
        assert_eq!(resolve_command_root(&dir, "cargo check --lib"), backend);
        // npm → marker at root → stays at root.
        assert_eq!(resolve_command_root(&dir, "npm test --silent"), dir);
        // unknown verb → unchanged.
        assert_eq!(resolve_command_root(&dir, "echo hello"), dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn command_root_handles_path_prefixed_and_exe_verbs() {
        let dir = std::env::temp_dir().join(format!("lht-cr-exe-{}", std::process::id()));
        let backend = dir.join("api");
        let _ = std::fs::create_dir_all(&backend);
        std::fs::write(backend.join("go.mod"), b"module x\n").unwrap();
        assert_eq!(resolve_command_root(&dir, "go build ./..."), backend);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toolchain_prefers_cargo_over_npm_for_tauri_polyglot() {
        let dir = std::env::temp_dir().join(format!("lht-tc-tauri-{}", std::process::id()));
        let backend = dir.join("src-tauri");
        let _ = std::fs::create_dir_all(&backend);
        std::fs::write(dir.join("package.json"), b"{}\n").unwrap();
        std::fs::write(backend.join("Cargo.toml"), b"[package]\n").unwrap();
        let entries = detect_toolchain_entries(&dir, GenericGateMode::Enforce);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.source == VerifySource::Toolchain));
        assert!(
            entries
                .iter()
                .all(|e| e.cmd.as_deref().unwrap_or("").starts_with("cargo "))
        );
        assert!(entries.iter().any(|e| e.id == "toolchain_cargo_build"));
        assert!(!entries.iter().any(|e| e.id == "toolchain_cargo_test"));
        assert!(
            !entries
                .iter()
                .any(|e| e.cmd.as_deref().unwrap_or("").contains("npm"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_dedups_with_operator_priority() {
        let operator = vec![CompletionGateVerifyEntry {
            id: "op_build".into(),
            cmd: Some("go build ./...".into()),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: 300,
            source: VerifySource::Operator,
        }];
        let toolchain = vec![CompletionGateVerifyEntry {
            id: "toolchain_go_build".into(),
            cmd: Some("go build ./...".into()),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: 600,
            source: VerifySource::Toolchain,
        }];
        let model = vec![CompletionGateVerifyEntry {
            id: "model_verify_1".into(),
            cmd: Some("go test ./...".into()),
            argv: Vec::new(),
            shell: ManifestShell::Default,
            timeout_secs: 600,
            source: VerifySource::ModelDeclared,
        }];
        let merged = merge_verify_entries(&operator, toolchain, model);
        // go build collapses to the operator entry; go test from model survives.
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].source, VerifySource::Operator);
        assert_eq!(merged[0].id, "op_build");
        assert!(
            merged
                .iter()
                .any(|e| e.cmd.as_deref() == Some("go test ./..."))
        );
    }
}
