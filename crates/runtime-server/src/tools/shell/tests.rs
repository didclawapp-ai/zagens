use super::*;

use crate::tools::shell_output::{summarize_output, truncate_with_meta};
use crate::tools::spec::{ToolContext, ToolSpec};
use serde_json::{Value, json};
use std::time::Duration;
use tempfile::tempdir;

fn echo_command(message: &str) -> String {
    format!("echo {message}")
}

fn sleep_command(seconds: u64) -> String {
    #[cfg(windows)]
    {
        format!("Start-Sleep -Seconds {seconds}")
    }
    #[cfg(not(windows))]
    {
        format!("sleep {seconds}")
    }
}

fn sleep_then_echo_command(seconds: u64, message: &str) -> String {
    #[cfg(windows)]
    {
        format!("Start-Sleep -Seconds {seconds}; Write-Output '{message}'")
    }
    #[cfg(not(windows))]
    {
        format!("sleep {seconds} && echo {message}")
    }
}

fn echo_stdin_command() -> String {
    #[cfg(windows)]
    {
        "more".to_string()
    }
    #[cfg(not(windows))]
    {
        "cat".to_string()
    }
}

#[cfg(windows)]
#[test]
fn prepare_windows_plan_blocks_id_rsa_via_spawn_sync() {
    use crate::sandbox::{CommandSpec, SandboxManager, SandboxPolicy};
    use std::time::Duration;
    use zagens_windows_sandbox::unelevated_deny_read_enabled;

    if !unelevated_deny_read_enabled() {
        return;
    }
    let profile = std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Users\Administrator"));
    let id_rsa = profile.join(".ssh").join("id_rsa");
    if !id_rsa.is_file() {
        return;
    }
    let workspace = std::path::PathBuf::from(r"F:\DeepSeek-TUI-desktop");
    if !workspace.is_dir() {
        return;
    }
    let command = format!("type {}", id_rsa.display());
    let policy = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![workspace.clone()],
        network_access: false,
        exclude_tmpdir: false,
        exclude_slash_tmp: false,
    };
    let spec = CommandSpec::shell(&command, workspace, Duration::from_secs(15)).with_policy(policy);
    let exec_env = SandboxManager::new().prepare(&spec);
    assert!(
        exec_env.is_enforced(),
        "expected enforced prepare_windows plan"
    );
    let plan = exec_env.windows_plan.as_ref().expect("windows plan");
    let out = zagens_windows_sandbox::spawn_sync(plan, None, Some(Duration::from_secs(15)))
        .expect("spawn_sync");
    assert!(
        !out.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
        "prepare_windows plan leaked via spawn_sync (argv={:?})",
        plan.argv
    );
}

#[cfg(windows)]
#[tokio::test]
async fn shell_manager_sync_vs_background_id_rsa_isolation() {
    use zagens_windows_sandbox::unelevated_deny_read_enabled;

    if !unelevated_deny_read_enabled() {
        return;
    }
    let profile = std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Users\Administrator"));
    let id_rsa = profile.join(".ssh").join("id_rsa");
    if !id_rsa.is_file() {
        return;
    }
    let workspace = std::path::PathBuf::from(r"F:\DeepSeek-TUI-desktop");
    if !workspace.is_dir() {
        return;
    }
    let policy = crate::sandbox::SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![workspace.clone()],
        network_access: false,
        exclude_tmpdir: false,
        exclude_slash_tmp: false,
    };
    let command = format!("type {}", id_rsa.display());

    let mut sync_manager = ShellManager::with_sandbox(workspace.clone(), policy.clone());
    let sync = sync_manager
        .execute(&command, None, 15_000, false)
        .expect("sync execute");
    assert!(
        !sync.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
        "sync path leaked: {}",
        sync.stdout
    );
    assert!(sync.sandbox_enforced);

    let mut bg_manager = ShellManager::with_sandbox(workspace, policy);
    let started = bg_manager
        .execute(&command, None, 15_000, true)
        .expect("bg start");
    let task_id = started.task_id.expect("task id");
    let bg = bg_manager
        .get_output(&task_id, true, 15_000)
        .expect("bg output");
    assert!(
        !bg.stdout.contains("BEGIN OPENSSH PRIVATE KEY"),
        "background path leaked: {}",
        bg.stdout
    );
    assert!(bg.sandbox_enforced);
}

#[cfg(windows)]
#[tokio::test]
async fn exec_shell_foreground_blocks_id_rsa_read_when_g0_passes() {
    use zagens_windows_sandbox::unelevated_deny_read_enabled;

    if !unelevated_deny_read_enabled() {
        eprintln!("skip: G0 deny-read PoC not pass");
        return;
    }
    let profile = std::env::var("USERPROFILE")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Users\Administrator"));
    let id_rsa = profile.join(".ssh").join("id_rsa");
    if !id_rsa.is_file() {
        eprintln!("skip: no id_rsa at {}", id_rsa.display());
        return;
    }

    let workspace = std::path::PathBuf::from(r"F:\DeepSeek-TUI-desktop");
    if !workspace.is_dir() {
        eprintln!("skip: workspace missing at {}", workspace.display());
        return;
    }

    let ctx = ToolContext::new(workspace.clone()).with_elevated_sandbox_policy(
        crate::sandbox::SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![workspace.clone()],
            network_access: false,
            exclude_tmpdir: false,
            exclude_slash_tmp: false,
        },
    );
    let tool = ExecShellTool;

    // Anti-false-positive control: a `type` with an identical command shape against a
    // readable workspace file MUST return content. A previous bug double-quoted the
    // `cmd /C` tail, so every `type "C:\..."` failed with a CMD syntax error and looked
    // "blocked" even though deny-read never fired. This control proves the command shape
    // works, so an empty id_rsa read below is a genuine block, not a parse error.
    let control = workspace.join("g1_control.txt");
    std::fs::write(&control, "g1-control-ok").expect("write control file");
    let control_result = tool
        .execute(
            json!({ "command": format!(r"type {}", control.display()) }),
            &ctx,
        )
        .await
        .expect("control execute");
    let _ = std::fs::remove_file(&control);
    assert!(
        control_result.content.contains("g1-control-ok"),
        "control `type` must read a workspace file (command shape is valid): {}",
        control_result.content
    );

    for command in [
        format!("type {}", id_rsa.display()),
        format!(r"C:\Windows\System32\more.com {}", id_rsa.display()),
    ] {
        let result = tool
            .execute(json!({ "command": command }), &ctx)
            .await
            .expect("execute");
        assert!(
            !result.content.contains("BEGIN OPENSSH PRIVATE KEY"),
            "exec_shell must not leak id_rsa for `{command}`: {}",
            result.content
        );
        let meta = result.metadata.as_ref().expect("metadata");
        assert_eq!(
            meta.get("sandbox_enforced").and_then(Value::as_bool),
            Some(true),
            "expected enforced spawn for `{command}`"
        );
    }
}

/// G1 fifth-round: workspace-write with runtime canonical workspace (`\\?\F:\…`).
#[cfg(windows)]
#[tokio::test]
async fn g1_smoke_workspace_write_with_canonical_cwd() {
    let workspace = std::path::PathBuf::from(r"F:\DeepSeek-TUI-desktop");
    if !workspace.is_dir() {
        eprintln!("skip: workspace missing at {}", workspace.display());
        return;
    }
    let workspace = workspace.canonicalize().expect("canonicalize workspace");
    assert!(
        workspace.display().to_string().starts_with(r"\\?\"),
        "expected verbatim canonical workspace for G1 T2 probe, got {}",
        workspace.display()
    );

    let policy = crate::sandbox::SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![workspace.clone()],
        network_access: false,
        exclude_tmpdir: false,
        exclude_slash_tmp: false,
    };
    let ctx = ToolContext::new(workspace.clone()).with_elevated_sandbox_policy(policy);
    let tool = ExecShellTool;

    let probe_path = workspace.join("g1_probe.txt");
    let _ = std::fs::remove_file(&probe_path);

    // T2a — absolute redirect target (Agent probe form).
    let write = tool
        .execute(
            json!({ "command": r"echo t2-ok > F:\DeepSeek-TUI-desktop\g1_probe.txt" }),
            &ctx,
        )
        .await
        .expect("T2a execute");
    assert!(write.success, "T2a write failed: {}", write.content);
    let write_meta = write.metadata.as_ref().expect("T2a metadata");
    assert_eq!(
        write_meta.get("sandbox_enforced").and_then(Value::as_bool),
        Some(true)
    );
    let stderr = write_meta
        .get("stderr_summary")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        !stderr.contains("UNC paths are not supported"),
        "T2a stderr must not contain UNC CWD warning: {stderr}"
    );

    // T2b
    let read = tool
        .execute(
            json!({ "command": r"type F:\DeepSeek-TUI-desktop\g1_probe.txt" }),
            &ctx,
        )
        .await
        .expect("T2b execute");
    assert!(
        read.content.contains("t2-ok"),
        "T2b read failed: {}",
        read.content
    );
    let read_meta = read.metadata.as_ref().expect("T2b metadata");
    let read_stderr = read_meta
        .get("stderr_summary")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        !read_stderr.contains("UNC paths are not supported"),
        "T2b stderr must not contain UNC CWD warning: {read_stderr}"
    );

    let _ = std::fs::remove_file(&probe_path);
}

#[test]
fn test_sync_execution() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let result = manager
        .execute(&echo_command("hello"), None, 5000, false)
        .expect("execute");

    assert_eq!(result.status, ShellStatus::Completed);
    assert!(result.stdout.contains("hello"));
    assert!(result.task_id.is_none());
}

#[test]
fn test_background_execution() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let result = manager
        .execute(&sleep_then_echo_command(1, "done"), None, 5000, true)
        .expect("execute");

    assert_eq!(result.status, ShellStatus::Running);
    assert!(result.task_id.is_some());

    let task_id = result
        .task_id
        .expect("background execution should return task_id");

    // Wait for completion
    let final_result = manager
        .get_output(&task_id, true, 5000)
        .expect("get_output");

    assert_eq!(final_result.status, ShellStatus::Completed);
    assert!(final_result.stdout.contains("done"));
}

#[test]
fn test_timeout() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let result = manager
        .execute(&sleep_command(10), None, 1000, false)
        .expect("execute");

    assert_eq!(result.status, ShellStatus::TimedOut);
}

#[test]
fn test_kill() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let result = manager
        .execute(&sleep_command(60), None, 5000, true)
        .expect("execute");

    let task_id = result
        .task_id
        .expect("background execution should return task_id");

    // Kill it
    let killed = manager.kill(&task_id).expect("kill");
    assert_eq!(killed.status, ShellStatus::Killed);
}

#[test]
fn test_write_stdin_streams_output() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let result = manager
        .execute_with_options(&echo_stdin_command(), None, 5000, true, None, false, None)
        .expect("execute");

    let task_id = result
        .task_id
        .expect("background execution should return task_id");

    manager
        .write_stdin(&task_id, "hello\n", true)
        .expect("write stdin");

    let delta = manager
        .get_output_delta(&task_id, true, 5000)
        .expect("get_output_delta");

    assert!(delta.result.stdout.contains("hello"));

    let delta2 = manager
        .get_output_delta(&task_id, false, 0)
        .expect("get_output_delta");
    assert!(delta2.result.stdout.is_empty());
}

#[test]
fn test_job_list_poll_cancel_and_stale_snapshot() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let started = manager
        .execute(&sleep_then_echo_command(1, "done"), None, 5000, true)
        .expect("execute");
    let task_id = started.task_id.expect("task id");
    manager
        .tag_linked_task(&task_id, Some("task_123".to_string()))
        .expect("tag linked task");

    let running = manager.list_jobs();
    let job = running
        .iter()
        .find(|job| job.id == task_id)
        .expect("running job");
    assert_eq!(job.status, ShellStatus::Running);
    assert_eq!(job.linked_task_id.as_deref(), Some("task_123"));
    assert!(job.command.contains("done"));
    assert_eq!(job.cwd, tmp.path());

    let completed = manager
        .poll_delta(&task_id, true, 5000)
        .expect("poll delta");
    assert_eq!(completed.result.status, ShellStatus::Completed);
    assert!(completed.result.stdout.contains("done"));

    let detail = manager.inspect_job(&task_id).expect("inspect");
    assert!(detail.stdout.contains("done"));
    assert_eq!(detail.snapshot.status, ShellStatus::Completed);

    manager.remember_stale_job(
        "shell_stale",
        "cargo test",
        tmp.path().to_path_buf(),
        Some("task_old".to_string()),
    );
    let stale = manager
        .list_jobs()
        .into_iter()
        .find(|job| job.id == "shell_stale")
        .expect("stale job");
    assert!(stale.stale);
    assert_eq!(stale.linked_task_id.as_deref(), Some("task_old"));
}

#[test]
fn test_job_cancel_updates_completion_state() {
    let tmp = tempdir().expect("tempdir");
    let mut manager = ShellManager::new(tmp.path().to_path_buf());

    let started = manager
        .execute(&sleep_command(60), None, 5000, true)
        .expect("execute");
    let task_id = started.task_id.expect("task id");

    let killed = manager.kill(&task_id).expect("kill");
    assert_eq!(killed.status, ShellStatus::Killed);
    let job = manager.inspect_job(&task_id).expect("inspect");
    assert_eq!(job.snapshot.status, ShellStatus::Killed);
    assert!(!job.snapshot.stdin_available);
}

#[test]
fn test_output_truncation() {
    let long_output = "x".repeat(50_000);
    let (truncated, _meta) = truncate_with_meta(&long_output);

    assert!(truncated.len() < long_output.len());
    assert!(truncated.contains("truncated"));
}

#[test]
fn test_truncate_with_meta_reports_omission_counts() {
    let long_output = format!("line1\nline2\n{}", "x".repeat(60_000));
    let (truncated, meta) = truncate_with_meta(&long_output);

    assert!(meta.truncated);
    assert!(meta.original_len >= long_output.len());
    assert!(meta.omitted > 0);
    assert!(truncated.contains("bytes omitted"));
}

#[test]
fn test_summarize_output_strips_truncation_note() {
    let long_output = "x".repeat(60_000);
    let (truncated, _meta) = truncate_with_meta(&long_output);
    let summary = summarize_output(&truncated);
    assert!(!summary.contains("Output truncated at"));
}

#[tokio::test]
async fn test_exec_shell_metadata_includes_summaries() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let tool = ExecShellTool;

    let result = tool
        .execute(json!({"command": echo_command("hello")}), &ctx)
        .await
        .expect("execute");
    assert!(result.success);

    let meta = result.metadata.expect("metadata");
    let summary = meta
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    assert!(summary.contains("hello"));
    assert!(meta.get("stdout_len").is_some());
    assert!(meta.get("stdout_truncated").is_some());
}

#[tokio::test]
async fn test_exec_shell_foreground_respects_cwd() {
    // Regression for C2: foreground exec_shell used to drop the `cwd` arg and
    // run from the workspace root. A relative write must land in the subdir.
    let tmp = tempdir().expect("tempdir");
    let subdir = tmp.path().join("nested");
    std::fs::create_dir(&subdir).expect("create subdir");
    let ctx = ToolContext::new(tmp.path());
    let tool = ExecShellTool;

    let result = tool
        .execute(
            json!({
                "command": "echo marker > marker.txt",
                "cwd": "nested"
            }),
            &ctx,
        )
        .await
        .expect("execute");

    assert!(result.success, "command failed: {}", result.content);
    assert!(
        subdir.join("marker.txt").exists(),
        "marker.txt should be created inside the cwd subdir, not the workspace root"
    );
    assert!(
        !tmp.path().join("marker.txt").exists(),
        "marker.txt must not leak to the workspace root"
    );
}

#[tokio::test]
async fn test_exec_shell_foreground_timeout_guides_background_rerun() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let tool = ExecShellTool;

    let result = tool
        .execute(
            json!({
                "command": sleep_command(10),
                "timeout_ms": 1000
            }),
            &ctx,
        )
        .await
        .expect("execute");

    assert!(!result.success);
    assert!(result.content.contains("task_shell_start"));
    assert!(result.content.contains("background: true"));
    assert!(result.content.contains("process killed"));
    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("TimedOut"));
    let recovery = meta
        .get("foreground_timeout_recovery")
        .expect("timeout recovery metadata");
    assert_eq!(
        recovery
            .get("exec_shell_background")
            .and_then(Value::as_bool),
        Some(true)
    );
    assert!(
        recovery
            .get("hint")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .contains("exec_shell_wait")
    );
}

#[tokio::test]
async fn test_exec_shell_foreground_cancel_kills_process() {
    let tmp = tempdir().expect("tempdir");
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let ctx = ToolContext::new(tmp.path()).with_cancel_token(cancel_token.clone());
    let command = sleep_command(30);

    let task = tokio::spawn(async move {
        ExecShellTool
            .execute(
                json!({
                    "command": command,
                    "timeout_ms": 600_000
                }),
                &ctx,
            )
            .await
            .expect("execute")
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel_token.cancel();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("foreground shell should observe cancellation")
        .expect("task should not panic");

    assert!(!result.success);
    assert!(result.content.contains("Command canceled"));
    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("Killed"));
    assert_eq!(meta.get("canceled").and_then(Value::as_bool), Some(true));
}

#[tokio::test]
async fn test_exec_shell_foreground_can_move_to_background() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let shell_manager = ctx.shell_manager.clone();
    let command = sleep_command(30);
    let task_ctx = ctx.clone();

    let task = tokio::spawn(async move {
        ExecShellTool
            .execute(
                json!({
                    "command": command,
                    "timeout_ms": 600_000
                }),
                &task_ctx,
            )
            .await
            .expect("execute")
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    shell_manager
        .lock()
        .expect("shell manager lock")
        .request_foreground_background();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("foreground shell should detach")
        .expect("task should not panic");

    assert!(result.success);
    assert!(result.content.contains("Command moved to background"));
    assert!(result.content.contains("exec_shell_cancel"));

    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("Running"));
    assert_eq!(
        meta.get("backgrounded").and_then(Value::as_bool),
        Some(true)
    );
    let task_id = meta
        .get("task_id")
        .and_then(Value::as_str)
        .expect("task id")
        .to_string();

    let mut manager = shell_manager.lock().expect("shell manager lock");
    let job = manager.inspect_job(&task_id).expect("inspect job");
    assert_eq!(job.snapshot.status, ShellStatus::Running);
    let killed = manager.kill(&task_id).expect("kill");
    assert_eq!(killed.status, ShellStatus::Killed);
}

#[tokio::test]
async fn test_exec_shell_wait_cancel_leaves_background_process_running() {
    let tmp = tempdir().expect("tempdir");
    let cancel_token = tokio_util::sync::CancellationToken::new();
    let ctx = ToolContext::new(tmp.path()).with_cancel_token(cancel_token.clone());
    let shell_manager = ctx.shell_manager.clone();
    let started = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&sleep_command(30), None, 600_000, true)
        .expect("execute");
    let task_id = started.task_id.expect("task id");
    let wait_task_id = task_id.clone();
    let task_ctx = ctx.clone();

    let task = tokio::spawn(async move {
        ShellWaitTool::new("exec_shell_wait")
            .execute(
                json!({
                    "task_id": wait_task_id,
                    "wait": true,
                    "timeout_ms": 600_000
                }),
                &task_ctx,
            )
            .await
            .expect("wait")
    });

    tokio::time::sleep(Duration::from_millis(150)).await;
    cancel_token.cancel();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("wait should observe cancellation")
        .expect("task should not panic");

    assert!(result.success);
    assert!(result.content.contains("still running"));
    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("Running"));
    assert_eq!(
        meta.get("wait_canceled").and_then(Value::as_bool),
        Some(true)
    );

    let mut manager = shell_manager.lock().expect("shell manager lock");
    let job = manager.inspect_job(&task_id).expect("inspect job");
    assert_eq!(job.snapshot.status, ShellStatus::Running);
    let killed = manager.kill(&task_id).expect("kill");
    assert_eq!(killed.status, ShellStatus::Killed);
}

#[tokio::test]
async fn test_completed_background_shell_releases_process_handles() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let shell_manager = ctx.shell_manager.clone();
    let started = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&echo_command("done"), None, 600_000, true)
        .expect("execute");
    let task_id = started.task_id.expect("task id");

    let result = ShellWaitTool::new("exec_shell_wait")
        .execute(
            json!({
                "task_id": task_id.clone(),
                "wait": true,
                "timeout_ms": 5_000
            }),
            &ctx,
        )
        .await
        .expect("wait");

    assert!(result.success);
    let mut manager = shell_manager.lock().expect("shell manager lock");
    let shell = manager.processes.get_mut(&task_id).expect("tracked shell");
    shell.poll();
    assert_eq!(shell.status, ShellStatus::Completed);
    assert!(shell.stdin.is_none());
    assert!(shell.child.is_none());
    assert!(shell.stdout_thread.is_none());
    assert!(shell.stderr_thread.is_none());
}

#[tokio::test]
async fn test_exec_shell_cancel_tool_kills_background_process() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let shell_manager = ctx.shell_manager.clone();
    let started = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&sleep_command(30), None, 600_000, true)
        .expect("execute");
    let task_id = started.task_id.expect("task id");

    let result = ShellCancelTool
        .execute(json!({ "task_id": task_id }), &ctx)
        .await
        .expect("cancel");

    assert!(result.success);
    assert!(result.content.contains("Canceled background shell job"));
    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("Killed"));

    let task_id = meta
        .get("task_id")
        .and_then(Value::as_str)
        .expect("task id");
    let mut manager = shell_manager.lock().expect("shell manager lock");
    let job = manager.inspect_job(task_id).expect("inspect job");
    assert_eq!(job.snapshot.status, ShellStatus::Killed);
}

#[tokio::test]
async fn test_exec_shell_cancel_tool_can_kill_all_running_processes() {
    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let shell_manager = ctx.shell_manager.clone();
    let first = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&sleep_command(30), None, 600_000, true)
        .expect("execute first")
        .task_id
        .expect("first task id");
    let second = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&sleep_command(30), None, 600_000, true)
        .expect("execute second")
        .task_id
        .expect("second task id");

    let result = ShellCancelTool
        .execute(json!({ "all": true }), &ctx)
        .await
        .expect("cancel all");

    assert!(result.success);
    let meta = result.metadata.expect("metadata");
    assert_eq!(meta.get("status").and_then(Value::as_str), Some("Killed"));
    assert_eq!(meta.get("canceled").and_then(Value::as_u64), Some(2));

    let mut manager = shell_manager.lock().expect("shell manager lock");
    let first_job = manager.inspect_job(&first).expect("inspect first");
    let second_job = manager.inspect_job(&second).expect("inspect second");
    assert_eq!(first_job.snapshot.status, ShellStatus::Killed);
    assert_eq!(second_job.snapshot.status, ShellStatus::Killed);
}

/// Tool surface audit T3 / C1 — killing a background shell must terminate the
/// whole process tree, not just the direct child. A bare `child.kill()` on
/// Windows leaves `Start-Process` grandchildren orphaned (the 7878/6379 port
/// leaks). The parent records its detached grandchild's PID, we kill the
/// parent, then assert the grandchild is gone.
#[cfg(windows)]
#[tokio::test]
async fn test_exec_shell_kill_terminates_grandchild_process_tree() {
    use std::time::Duration as StdDuration;

    fn pid_is_alive(pid: u32) -> bool {
        // `tasklist /FI "PID eq <pid>"` prints the image name when the PID is
        // running, and "No tasks are running…" otherwise.
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .expect("tasklist");
        let text = String::from_utf8_lossy(&out.stdout);
        text.contains(&pid.to_string())
    }

    let tmp = tempdir().expect("tempdir");
    let ctx = ToolContext::new(tmp.path());
    let shell_manager = ctx.shell_manager.clone();

    let pid_file = tmp.path().join("gc_pid.txt");
    // Parent spawns a detached grandchild (Start-Process => CreateProcess child)
    // that sleeps long, writes its PID to a file, then the parent also sleeps so
    // it stays Running until we kill it.
    let command = format!(
        "$gc = Start-Process powershell -PassThru -WindowStyle Hidden \
         -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 120'; \
         Set-Content -LiteralPath '{pid_path}' -Value $gc.Id; \
         Start-Sleep -Seconds 120",
        pid_path = pid_file.display()
    );

    let task_id = shell_manager
        .lock()
        .expect("shell manager lock")
        .execute(&command, None, 600_000, true)
        .expect("execute")
        .task_id
        .expect("task id");

    // Wait for the grandchild to register its PID (up to ~15s).
    let mut grandchild_pid: Option<u32> = None;
    for _ in 0..150 {
        if let Ok(text) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = text.trim().parse::<u32>()
        {
            grandchild_pid = Some(pid);
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(100)).await;
    }
    let grandchild_pid = grandchild_pid.expect("grandchild should have recorded its PID");
    assert!(
        pid_is_alive(grandchild_pid),
        "grandchild {grandchild_pid} should be running before kill"
    );

    shell_manager
        .lock()
        .expect("shell manager lock")
        .kill(&task_id)
        .expect("kill");

    // The tree kill is best-effort/asynchronous; give taskkill a moment.
    let mut terminated = false;
    for _ in 0..50 {
        if !pid_is_alive(grandchild_pid) {
            terminated = true;
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(100)).await;
    }
    if !terminated {
        // Avoid leaking the orphan if the assertion is about to fail.
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &grandchild_pid.to_string()])
            .output();
    }
    assert!(
        terminated,
        "grandchild {grandchild_pid} must be terminated when the parent shell is killed"
    );
}
