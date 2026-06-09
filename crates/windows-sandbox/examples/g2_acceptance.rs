//! Gate G2 acceptance probes for the elevated Windows sandbox.
//!
//! Run (admin shell recommended; setup must have completed):
//! ```powershell
//! $env:ZAGENS_HOME = "F:\DeepSeek-TUI-desktop\.g2-home"
//! $env:PATH = "F:\DeepSeek-TUI-desktop\target\debug;$env:PATH"
//! cargo run --example g2_acceptance -p zagens-windows-sandbox
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use zagens_windows_sandbox::{
    PlanInput, WindowsSandboxMode, plan_exec, sandbox_setup_is_complete, spawn_sync,
    zagens_home_from_env,
};

#[derive(Debug, Serialize)]
struct ProbeResult {
    id: &'static str,
    pass: bool,
    detail: String,
}

fn main() -> anyhow::Result<()> {
    let home = zagens_home_from_env();
    if !sandbox_setup_is_complete(&home) {
        anyhow::bail!(
            "setup incomplete under {}; run `zagens sandbox setup` first",
            home.display()
        );
    }

    let profile = std::env::var("USERPROFILE").unwrap_or_else(|_| r"C:\Users\Administrator".into());
    // Sandbox users cannot use the real user's %TEMP% as spawn CWD (Win32 267).
    let workspace = home.join(format!("g2-workspace-{}", std::process::id()));
    std::fs::create_dir_all(&workspace)?;

    let mut results = Vec::new();

    let smoke_file = workspace.join("smoke.txt");
    let _ = std::fs::remove_file(&smoke_file);
    results.push(run_probe(
        "smoke_cmd_echo",
        elevated_cmd(
            &workspace,
            &format!(r#"echo g2-smoke-ok> "{}""#, smoke_file.display()),
            false,
        ),
        |_| {
            smoke_file.is_file()
                && std::fs::read_to_string(&smoke_file)
                    .unwrap_or_default()
                    .contains("g2-smoke")
        },
        |out| {
            let file = std::fs::read_to_string(&smoke_file).unwrap_or_default();
            format!(
                "exit={} file={:?} stdout={:?} stderr={:?}",
                out.exit_code,
                tail(&file, 40),
                tail(&out.stdout, 40),
                tail(&out.stderr, 40)
            )
        },
    ));

    // --- read isolation ---
    let ssh_key = PathBuf::from(&profile).join(".ssh").join("id_rsa");
    if ssh_key.is_file() {
        results.push(run_probe(
            "read_ssh_denied",
            elevated_cmd(
                &workspace,
                &format!(r#"type "{}""#, ssh_key.display()),
                false,
            ),
            |out| {
                out.exit_code != 0
                    || out.stderr.to_ascii_lowercase().contains("denied")
                    || out.stderr.to_ascii_lowercase().contains("拒绝")
                    || !out.stdout.contains("OPENSSH")
            },
            |out| {
                format!(
                    "exit={} stdout_len={} stderr={}",
                    out.exit_code,
                    out.stdout.len(),
                    tail(&out.stderr, 200)
                )
            },
        ));
    } else {
        results.push(ProbeResult {
            id: "read_ssh_denied",
            pass: true,
            detail: "skipped: no id_rsa at profile/.ssh".into(),
        });
    }

    results.push(run_probe(
        "read_program_files",
        elevated_cmd(&workspace, r#"type "C:\Program Files\desktop.ini""#, false),
        |out| out.exit_code == 0 && !out.stdout.is_empty(),
        |out| format!("exit={} stdout_len={}", out.exit_code, out.stdout.len()),
    ));

    // Profile root gets a non-inheritable read grant during setup; listing the
    // root (not Documents/) is the G2 signal that grant-read pinned the root ACE.
    results.push(run_probe(
        "read_profile_root",
        elevated_cmd(&workspace, &format!(r#"dir "{}""#, profile), false),
        |out| out.exit_code == 0 && out.stdout.len() > 20,
        |out| format!("exit={} stdout_len={}", out.exit_code, out.stdout.len()),
    ));

    // --- write isolation ---
    let inside = workspace.join("g2_write_inside.txt");
    let outside = PathBuf::from(r"C:\g2_write_outside_zagens.txt");
    let _ = std::fs::remove_file(&outside);
    results.push(run_probe(
        "write_workspace_ok",
        elevated_cmd(
            &workspace,
            &format!(r#"echo g2-inside > "{}""#, inside.display()),
            false,
        ),
        |_| inside.is_file(),
        |out| format!("exit={} file_exists={}", out.exit_code, inside.is_file()),
    ));
    results.push(run_probe(
        "write_outside_denied",
        elevated_cmd(
            &workspace,
            &format!(r#"echo g2-outside > "{}""#, outside.display()),
            false,
        ),
        |_| !outside.exists(),
        |out| format!("exit={} outside_exists={}", out.exit_code, outside.exists()),
    ));

    // --- network (offline user) ---
    results.push(run_probe(
        "net_offline_blocked",
        elevated_cmd(
            &workspace,
            r#"C:\Windows\System32\curl.exe -s -m 5 http://example.com"#,
            false,
        ),
        |out| out.exit_code != 0 || out.stdout.is_empty(),
        |out| format!("exit={} stdout_len={}", out.exit_code, out.stdout.len()),
    ));

    // --- network (online user) ---
    results.push(run_probe(
        "net_online_allowed",
        elevated_cmd(
            &workspace,
            r#"C:\Windows\System32\curl.exe -s -m 10 http://example.com"#,
            true,
        ),
        |out| out.exit_code == 0 && out.stdout.contains("Example Domain"),
        |out| format!("exit={} stdout_len={}", out.exit_code, out.stdout.len()),
    ));

    let report_path = home.join(".sandbox").join("g2_acceptance_report.json");
    std::fs::create_dir_all(home.join(".sandbox"))?;
    let pass_count = results.iter().filter(|r| r.pass).count();
    let summary = format!("{pass_count}/{} probes passed", results.len());
    let json = serde_json::json!({
        "summary": summary,
        "probes": results,
    });
    std::fs::write(&report_path, serde_json::to_string_pretty(&json)?)?;
    println!("{summary}");
    println!("report: {}", report_path.display());
    for r in &results {
        println!(
            "[{}] {} — {}",
            if r.pass { "PASS" } else { "FAIL" },
            r.id,
            r.detail
        );
    }

    let _ = std::fs::remove_dir_all(&workspace);
    if pass_count != results.len() {
        std::process::exit(1);
    }
    Ok(())
}

fn elevated_cmd(
    workspace: &PathBuf,
    command: &str,
    network_allowed: bool,
) -> anyhow::Result<zagens_windows_sandbox::CapturedOutput> {
    let plan = plan_exec(PlanInput {
        program: "cmd".into(),
        args: vec!["/C".into(), command.into()],
        cwd: workspace.clone(),
        env: HashMap::new(),
        writable_roots: vec![workspace.clone()],
        protected_write_paths: zagens_windows_sandbox::protected_subdirs_for_root(workspace),
        network_allowed,
        mode: WindowsSandboxMode::Elevated,
    })?;
    spawn_sync(&plan, None, Some(Duration::from_secs(30)))
}

fn run_probe<F, G>(
    id: &'static str,
    run: anyhow::Result<zagens_windows_sandbox::CapturedOutput>,
    ok: F,
    detail: G,
) -> ProbeResult
where
    F: FnOnce(&zagens_windows_sandbox::CapturedOutput) -> bool,
    G: FnOnce(&zagens_windows_sandbox::CapturedOutput) -> String,
{
    match run {
        Ok(out) => ProbeResult {
            id,
            pass: ok(&out),
            detail: detail(&out),
        },
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("spawn error: {err:#}"),
        },
    }
}

fn tail(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars()
        .rev()
        .take(max)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}
