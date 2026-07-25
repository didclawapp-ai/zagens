//! Output truncation and summarization helpers for shell tools.

use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Maximum output size before truncation (30KB like Claude Code).
const MAX_OUTPUT_SIZE: usize = 30_000;
/// Limits for summary strings in tool metadata.
const SUMMARY_MAX_LINES: usize = 3;
const SUMMARY_MAX_CHARS: usize = 240;
/// Maximum number of preserved high-signal lines extracted from the tail
/// when output is truncated (#242). Bounded so the preserved summary
/// itself can never blow up the context window.
const MAX_PRESERVED_SUMMARY_LINES: usize = 80;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TruncationMeta {
    pub(crate) original_len: usize,
    pub(crate) omitted: usize,
    pub(crate) truncated: bool,
}

pub(crate) fn truncate_with_meta(output: &str) -> (String, TruncationMeta) {
    let original_len = output.len();
    if original_len <= MAX_OUTPUT_SIZE {
        return (
            output.to_string(),
            TruncationMeta {
                original_len,
                omitted: 0,
                truncated: false,
            },
        );
    }

    let cut_index = char_boundary_at_or_before(output, MAX_OUTPUT_SIZE);
    let head = &output[..cut_index];
    let tail = &output[cut_index..];
    let omitted = original_len.saturating_sub(cut_index);
    let note =
        format!("...\n\n[Output truncated at {MAX_OUTPUT_SIZE} bytes. {omitted} bytes omitted.]");

    // Preserve high-signal summary lines from the tail (cargo test results,
    // rustc errors, panics, completion markers). Without this the agent
    // re-runs `cargo test | tail` repeatedly to find pass/fail (#242).
    let mut combined = format!("{head}{note}");
    let preserved = collect_summary_lines(tail);
    if !preserved.is_empty() {
        combined.push_str("\n\n[Preserved summary lines from omitted tail]\n");
        combined.push_str(&preserved.join("\n"));
    }

    (
        combined,
        TruncationMeta {
            original_len,
            omitted,
            truncated: true,
        },
    )
}

/// Spill metadata when truncated shell output is persisted under `.zagens/shell-output/`.
#[derive(Debug, Clone)]
pub(crate) struct ShellSpillInfo {
    pub read_path_hint: String,
    pub absolute_path: PathBuf,
}

/// Truncate stdout/stderr; when either exceeds [`MAX_OUTPUT_SIZE`], persist the full
/// combined log under `{workspace}/.zagens/shell-output/`.
pub(crate) fn truncate_shell_streams_with_spill(
    workspace: &Path,
    stdout_full: &str,
    stderr_full: &str,
) -> (
    String,
    TruncationMeta,
    String,
    TruncationMeta,
    Option<ShellSpillInfo>,
) {
    let (stdout, stdout_meta) = truncate_with_meta(stdout_full);
    let (stderr, stderr_meta) = truncate_with_meta(stderr_full);
    let spill = if stdout_meta.truncated || stderr_meta.truncated {
        persist_shell_combined_output(workspace, stdout_full, stderr_full).ok()
    } else {
        None
    };
    (stdout, stdout_meta, stderr, stderr_meta, spill)
}

/// Apply truncation + optional spill to shell buffer fields on [`ShellResult`].
pub(crate) fn assign_truncated_shell_streams(
    result: &mut crate::tools::shell::ShellResult,
    workspace: &Path,
    stdout_full: &str,
    stderr_full: &str,
) {
    let (stdout, stdout_meta, stderr, stderr_meta, spill) =
        truncate_shell_streams_with_spill(workspace, stdout_full, stderr_full);
    result.stdout = stdout;
    result.stderr = stderr;
    result.stdout_len = stdout_meta.original_len;
    result.stderr_len = stderr_meta.original_len;
    result.stdout_omitted = stdout_meta.omitted;
    result.stderr_omitted = stderr_meta.omitted;
    result.stdout_truncated = stdout_meta.truncated;
    result.stderr_truncated = stderr_meta.truncated;
    result.full_output_spill_path = spill.map(|s| s.read_path_hint);
}

/// Append the model-visible spill hint when full output was saved to disk.
pub(crate) fn append_shell_spill_note(content: &mut String, spill_path: Option<&str>) {
    if let Some(path) = spill_path.filter(|p| !p.is_empty()) {
        content.push_str(&format!(
            "\n\nFull output saved to: {path}. Use read_file or grep_files to inspect the complete log."
        ));
    }
}

fn persist_shell_combined_output(
    workspace: &Path,
    stdout_full: &str,
    stderr_full: &str,
) -> std::io::Result<ShellSpillInfo> {
    let spill_id = format!("shout_{}", &Uuid::new_v4().to_string()[..8]);
    let rel = format!("shell-output/{spill_id}.log");
    let path = zagens_config::workspace_meta_file_write(workspace, &rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = format!(
        "=== STDOUT ({} bytes) ===\n{stdout_full}\n\n=== STDERR ({} bytes) ===\n{stderr_full}\n",
        stdout_full.len(),
        stderr_full.len()
    );
    std::fs::write(&path, body)?;
    Ok(ShellSpillInfo {
        read_path_hint: zagens_config::workspace_meta_rel(&rel),
        absolute_path: path,
    })
}

/// Extract high-signal summary lines from a chunk of output that would
/// otherwise be discarded by truncation. Recognises Cargo/rustc output,
/// generic test framework summaries, panic markers, exit-status lines,
/// and `Finished`/`running ...` markers. Returns at most
/// `MAX_PRESERVED_SUMMARY_LINES` lines, oldest-first within each match
/// class so the most actionable signal is at the end.
pub(crate) fn collect_summary_lines(text: &str) -> Vec<String> {
    let mut preserved: Vec<String> = Vec::new();
    for line in text.lines() {
        if preserved.len() >= MAX_PRESERVED_SUMMARY_LINES {
            break;
        }
        if is_summary_line(line) {
            preserved.push(line.to_string());
        }
    }
    preserved
}

/// Heuristics for "this line is worth preserving even when most of the
/// output is dropped." Tuned for Cargo/rustc and generic test runner
/// vocabulary. Intentionally conservative: false positives only cost a
/// handful of bytes; false negatives force the agent to re-run gates.
fn is_summary_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return false;
    }
    // Cargo / rustc canonical markers. Note `trim_start` already stripped
    // any leading whitespace, so match the bare word — the indentation
    // Cargo prints (e.g. "    Finished") would never reach this point.
    if trimmed.starts_with("test result:")
        || trimmed.starts_with("failures:")
        || trimmed.starts_with("FAILED")
        || trimmed.starts_with("error[")
        || trimmed.starts_with("error:")
        || trimmed.starts_with("warning:")
        || trimmed.starts_with("panicked at")
        || trimmed.starts_with("note:")
        || trimmed.starts_with("help:")
        || trimmed.starts_with("Finished")
        || trimmed.starts_with("Compiling")
        || trimmed.starts_with("Building")
        || trimmed.starts_with("Running")
        || trimmed.starts_with("running ")
        || trimmed.starts_with("Doc-tests")
        || trimmed.starts_with("---- ")
    {
        return true;
    }
    // Generic test runner vocabulary.
    if trimmed.contains("PASS") || trimmed.contains("FAIL") || trimmed.contains("ASSERT") {
        return true;
    }
    // Process-level signal lines.
    if trimmed.starts_with("Killed")
        || trimmed.starts_with("Aborted")
        || trimmed.starts_with("Segmentation fault")
        || trimmed.starts_with("Error:")
        || trimmed.starts_with("exit status")
        || trimmed.starts_with("exit code")
    {
        return true;
    }
    // `test some::name ... ok|FAILED|ignored` is the per-test result line in
    // libtest. Cheap to match and useful for pinpointing the failing case.
    if trimmed.starts_with("test ") && (trimmed.ends_with("FAILED") || trimmed.ends_with("ignored"))
    {
        return true;
    }
    false
}

fn char_boundary_at_or_before(text: &str, max_bytes: usize) -> usize {
    if max_bytes >= text.len() {
        return text.len();
    }

    let mut last_end = 0usize;
    for (idx, ch) in text.char_indices() {
        let end = idx.saturating_add(ch.len_utf8());
        if end > max_bytes {
            break;
        }
        last_end = end;
    }

    last_end.min(text.len())
}

fn strip_truncation_note(text: &str) -> &str {
    text.split_once("\n\n[Output truncated at")
        .map_or(text, |(prefix, _)| prefix)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut end = text.len();
    for (count, (idx, _)) in text.char_indices().enumerate() {
        if count == max_chars {
            end = idx;
            break;
        }
    }

    format!("{}...", &text[..end])
}

pub(crate) fn summarize_output(text: &str) -> String {
    // High-signal lines (test result:/failures/errors/panics/exit status) almost
    // always live at the *tail* of build/test output. Scanning the full text
    // (including any "[Preserved summary lines ...]" block that truncation
    // appended) and biasing toward the last matches surfaces the conclusion —
    // taking the first N lines would only show `Compiling`/`running N tests`
    // and silently drop `test result:` (C7).
    let mut tail_signal: Vec<&str> = text
        .lines()
        .rev()
        .filter(|line| is_summary_line(line))
        .take(SUMMARY_MAX_LINES)
        .collect();
    if !tail_signal.is_empty() {
        tail_signal.reverse();
        let joined = tail_signal.join("\n");
        let joined = joined.trim();
        if !joined.is_empty() {
            return truncate_chars(joined, SUMMARY_MAX_CHARS);
        }
    }

    // Fallback: no recognised signal lines, summarise from the head.
    let stripped = strip_truncation_note(text);
    let summary = stripped
        .lines()
        .take(SUMMARY_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();

    if summary.is_empty() {
        String::new()
    } else {
        truncate_chars(&summary, SUMMARY_MAX_CHARS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_preserves_cargo_test_summary_lines_from_tail() {
        let mut head = String::with_capacity(MAX_OUTPUT_SIZE + 4_000);
        head.push_str("running 5 tests\n");
        for i in 0..3_000 {
            head.push_str(&format!("test test::case_{i} ... ok\n"));
        }
        // Pad to force tail truncation
        while head.len() < MAX_OUTPUT_SIZE {
            head.push_str("...padding line below threshold...\n");
        }
        head.push_str("\ntest result: ok. 1687 passed; 0 failed; 2 ignored\n");
        head.push_str("    Finished `dev` profile target(s) in 4.87s\n");

        let (truncated, meta) = truncate_with_meta(&head);
        assert!(meta.truncated, "expected truncation");
        assert!(
            truncated.contains("test result: ok. 1687 passed"),
            "summary line must be preserved\nGot: {}",
            &truncated[truncated.len().saturating_sub(400)..]
        );
        assert!(
            truncated.contains("Finished"),
            "Finished marker must be preserved"
        );
    }

    #[test]
    fn truncation_preserves_failure_lines_from_tail() {
        let mut head = String::with_capacity(MAX_OUTPUT_SIZE + 1_000);
        for _ in 0..MAX_OUTPUT_SIZE {
            head.push('a');
        }
        head.push_str("\nfailures:\n  test::flaky_thing FAILED\n");
        head.push_str("test result: FAILED. 0 passed; 1 failed\n");

        let (truncated, _meta) = truncate_with_meta(&head);
        assert!(truncated.contains("failures:"), "must preserve failures:");
        assert!(truncated.contains("FAILED"), "must preserve FAILED");
    }

    #[test]
    fn collect_summary_lines_skips_noise() {
        let body = "\nblah blah\nrandom line\nokay\n\n";
        assert!(collect_summary_lines(body).is_empty());
    }

    #[test]
    fn summarize_output_surfaces_tail_test_result() {
        let body = "\
Compiling zagens-cli v0.8.15
   Compiling foo v1.0
running 1687 tests
test a ... ok
test b ... ok
test result: ok. 1687 passed; 0 failed; 2 ignored
";
        let summary = summarize_output(body);
        assert!(
            summary.contains("test result: ok. 1687 passed"),
            "summary must surface the tail conclusion, got: {summary}"
        );
        assert!(
            !summary.contains("Compiling deepseek"),
            "summary should not be dominated by head noise, got: {summary}"
        );
    }

    #[test]
    fn summarize_output_falls_back_to_head_without_signal() {
        let body = "first line\nsecond line\nthird line\nfourth line\n";
        let summary = summarize_output(body);
        assert!(summary.contains("first line"), "got: {summary}");
    }

    #[test]
    fn summarize_output_finds_result_in_preserved_tail_block() {
        let mut head = String::with_capacity(MAX_OUTPUT_SIZE + 1_000);
        head.push_str("running 5 tests\n");
        while head.len() < MAX_OUTPUT_SIZE {
            head.push_str("noise line that is not a summary line\n");
        }
        head.push_str("\ntest result: FAILED. 4 passed; 1 failed\n");
        let (truncated, meta) = truncate_with_meta(&head);
        assert!(meta.truncated);
        let summary = summarize_output(&truncated);
        assert!(
            summary.contains("test result: FAILED"),
            "summary must read the preserved tail block, got: {summary}"
        );
    }

    #[test]
    fn collect_summary_lines_picks_rustc_errors() {
        let body = "\
some preamble
error[E0277]: the trait `Foo` is not implemented for `Bar`
  --> src/lib.rs:42:9
warning: unused variable
note: see help
";
        let preserved = collect_summary_lines(body);
        assert!(preserved.iter().any(|line| line.contains("error[E0277]")));
        assert!(preserved.iter().any(|line| line.contains("warning:")));
    }

    #[test]
    fn spill_persists_combined_output_when_truncated() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static SPILL_COUNTER: AtomicUsize = AtomicUsize::new(0);
        let workspace = std::env::temp_dir().join(format!(
            "zagens-shell-spill-test-{}",
            SPILL_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&workspace).expect("workspace dir");

        let mut stdout = String::with_capacity(MAX_OUTPUT_SIZE + 500);
        stdout.push_str("running 1 test\n");
        while stdout.len() < MAX_OUTPUT_SIZE {
            stdout.push('x');
        }
        stdout.push_str("\ntest result: ok. 1 passed\n");

        let (_, stdout_meta, _, stderr_meta, spill) =
            truncate_shell_streams_with_spill(&workspace, &stdout, "");
        assert!(stdout_meta.truncated);
        assert!(!stderr_meta.truncated);
        let spill = spill.expect("spill path");
        assert!(spill.absolute_path.is_file());
        let saved = std::fs::read_to_string(&spill.absolute_path).expect("read spill");
        assert!(saved.contains("=== STDOUT"));
        assert!(saved.contains("test result: ok. 1 passed"));
        assert!(spill.read_path_hint.starts_with(".zagens/shell-output/"));

        let _ = std::fs::remove_dir_all(&workspace);
    }
}
