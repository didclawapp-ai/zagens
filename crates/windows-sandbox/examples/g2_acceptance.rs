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
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use zagens_windows_sandbox::{
    PlanInput, WindowsSandboxMode, add_session_read_dir, extract_spawn_denial_code,
    path_traverses_excluded_profile_meta, plan_exec, protected_subdirs_for_root,
    sandbox_setup_is_complete, spawn_background_elevated, spawn_sync, zagens_home_from_env,
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
    // Workspace must not live under grant-excluded profile dirs (e.g. ~/.zagens).
    let workspace = if path_traverses_excluded_profile_meta(&home) {
        PathBuf::from(&profile)
            .join("Documents")
            .join("Zagens")
            .join(format!("g2-workspace-{}", std::process::id()))
    } else {
        home.join(format!("g2-workspace-{}", std::process::id()))
    };
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

    // --- elevated background spawn (IPC runner path) ---
    results.push(probe_bg_spawn_stdout(&workspace));
    results.push(probe_bg_write_stdin(&workspace));
    results.push(probe_bg_kill(&workspace));
    results.push(probe_conpty_echo(&workspace));
    results.push(probe_add_read_dir(&workspace, &home));

    // --- structured spawn denial (PR-2.13) ---
    results.push(probe_spawn_denial_code(&workspace));

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
    workspace: &std::path::Path,
    command: &str,
    network_allowed: bool,
) -> anyhow::Result<zagens_windows_sandbox::CapturedOutput> {
    let plan = elevated_plan(workspace, "cmd", vec!["/C", command], network_allowed)?;
    spawn_sync(&plan, None, Some(Duration::from_secs(30)))
}

fn elevated_plan(
    workspace: &std::path::Path,
    program: &str,
    args: Vec<&str>,
    network_allowed: bool,
) -> anyhow::Result<zagens_windows_sandbox::WindowsExecPlan> {
    plan_exec(PlanInput {
        program: program.into(),
        args: args.into_iter().map(str::to_string).collect(),
        cwd: workspace.to_path_buf(),
        env: HashMap::new(),
        writable_roots: vec![workspace.to_path_buf()],
        protected_write_paths: protected_subdirs_for_root(workspace),
        network_allowed,
        mode: WindowsSandboxMode::Elevated,
        private_desktop: false,
        tty: false,
    })
}

fn probe_bg_spawn_stdout(workspace: &std::path::Path) -> ProbeResult {
    let id = "bg_spawn_stdout";
    let plan = match elevated_plan(workspace, "cmd", vec!["/C", "echo bg-spawn-ok"], false) {
        Ok(plan) => plan,
        Err(err) => {
            return ProbeResult {
                id,
                pass: false,
                detail: format!("plan error: {err:#}"),
            };
        }
    };
    match spawn_background_elevated(&plan, None) {
        Ok(mut child) => {
            let stdout = Arc::new(Mutex::new(Vec::new()));
            let stderr = Arc::new(Mutex::new(Vec::new()));
            let out_buf = Arc::clone(&stdout);
            let err_buf = Arc::clone(&stderr);
            if let Err(err) = child.start_output_pump(
                move |chunk| {
                    if let Ok(mut guard) = out_buf.lock() {
                        guard.extend_from_slice(chunk);
                    }
                },
                move |chunk| {
                    if let Ok(mut guard) = err_buf.lock() {
                        guard.extend_from_slice(chunk);
                    }
                },
            ) {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("pump error: {err:#}"),
                };
            }
            let exit = match child.wait(Some(Duration::from_secs(30))) {
                Ok(code) => code,
                Err(err) => {
                    return ProbeResult {
                        id,
                        pass: false,
                        detail: format!("wait error: {err:#}"),
                    };
                }
            };
            let stdout_bytes = stdout.lock().map(|g| g.clone()).unwrap_or_default();
            let stdout = String::from_utf8_lossy(&stdout_bytes);
            ProbeResult {
                id,
                pass: exit == 0 && stdout.contains("bg-spawn-ok"),
                detail: format!("exit={exit} stdout={stdout:?}"),
            }
        }
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("spawn error: {err:#}"),
        },
    }
}

fn probe_bg_write_stdin(workspace: &std::path::Path) -> ProbeResult {
    let id = "bg_write_stdin";
    let plan = match elevated_plan(workspace, "cmd", vec!["/C", "more"], false) {
        Ok(plan) => plan,
        Err(err) => {
            return ProbeResult {
                id,
                pass: false,
                detail: format!("plan error: {err:#}"),
            };
        }
    };
    match spawn_background_elevated(&plan, None) {
        Ok(mut child) => {
            let stdout = Arc::new(Mutex::new(Vec::new()));
            let out_buf = Arc::clone(&stdout);
            if let Err(err) = child.start_output_pump(
                move |chunk| {
                    if let Ok(mut guard) = out_buf.lock() {
                        guard.extend_from_slice(chunk);
                    }
                },
                |_| {},
            ) {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("pump error: {err:#}"),
                };
            }
            std::thread::sleep(Duration::from_millis(300));
            if let Err(err) = child.write_stdin(b"g2-stdin-line\n") {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("write_stdin error: {err:#}"),
                };
            }
            child.close_stdin();
            let exit = match child.wait(Some(Duration::from_secs(30))) {
                Ok(code) => code,
                Err(err) => {
                    return ProbeResult {
                        id,
                        pass: false,
                        detail: format!("wait error: {err:#}"),
                    };
                }
            };
            let stdout_bytes = stdout.lock().map(|g| g.clone()).unwrap_or_default();
            let stdout = String::from_utf8_lossy(&stdout_bytes);
            ProbeResult {
                id,
                pass: exit == 0 && stdout.contains("g2-stdin-line"),
                detail: format!("exit={exit} stdout={stdout:?}"),
            }
        }
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("spawn error: {err:#}"),
        },
    }
}

fn probe_bg_kill(workspace: &std::path::Path) -> ProbeResult {
    let id = "bg_kill";
    let plan = match elevated_plan(
        workspace,
        "powershell",
        vec!["-NoProfile", "-Command", "Start-Sleep -Seconds 60"],
        false,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            return ProbeResult {
                id,
                pass: false,
                detail: format!("plan error: {err:#}"),
            };
        }
    };
    match spawn_background_elevated(&plan, None) {
        Ok(mut child) => {
            if let Err(err) = child.start_output_pump(|_| {}, |_| {}) {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("pump error: {err:#}"),
                };
            }
            std::thread::sleep(Duration::from_millis(500));
            let still_running = child.try_wait().ok().flatten().is_none();
            let started = Instant::now();
            if let Err(err) = child.kill() {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("kill error: {err:#}"),
                };
            }
            let exit = match child.wait(Some(Duration::from_secs(15))) {
                Ok(code) => code,
                Err(err) => {
                    return ProbeResult {
                        id,
                        pass: false,
                        detail: format!("wait after kill error: {err:#}"),
                    };
                }
            };
            let elapsed = started.elapsed();
            ProbeResult {
                id,
                pass: still_running && elapsed < Duration::from_secs(10) && exit != 0,
                detail: format!(
                    "still_running={still_running} exit={exit} kill_elapsed_ms={}",
                    elapsed.as_millis()
                ),
            }
        }
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("spawn error: {err:#}"),
        },
    }
}

/// ConPTY path (PR-3.1): elevated runner spawns with `CreatePseudoConsole`.
fn probe_conpty_echo(workspace: &std::path::Path) -> ProbeResult {
    let id = "conpty_echo";
    let mut plan = match elevated_plan(workspace, "cmd", vec!["/C", "echo g2-conpty-ok"], false) {
        Ok(plan) => plan,
        Err(err) => {
            return ProbeResult {
                id,
                pass: false,
                detail: format!("plan error: {err:#}"),
            };
        }
    };
    plan.tty = true;
    match spawn_background_elevated(&plan, None) {
        Ok(mut child) => {
            let stdout = Arc::new(Mutex::new(Vec::new()));
            let out_buf = Arc::clone(&stdout);
            if let Err(err) = child.start_output_pump(
                move |chunk| {
                    if let Ok(mut guard) = out_buf.lock() {
                        guard.extend_from_slice(chunk);
                    }
                },
                |_| {},
            ) {
                return ProbeResult {
                    id,
                    pass: false,
                    detail: format!("pump error: {err:#}"),
                };
            }
            let exit = match child.wait(Some(Duration::from_secs(45))) {
                Ok(code) => code,
                Err(err) => {
                    return ProbeResult {
                        id,
                        pass: false,
                        detail: format!("wait error: {err:#}"),
                    };
                }
            };
            let stdout_bytes = stdout.lock().map(|g| g.clone()).unwrap_or_default();
            let stdout = String::from_utf8_lossy(&stdout_bytes);
            ProbeResult {
                id,
                pass: exit == 0 && stdout.contains("g2-conpty-ok"),
                detail: format!("exit={exit} stdout={stdout:?}"),
            }
        }
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("spawn error: {err:#}"),
        },
    }
}

/// Session read-dir grant (PR-3.3): ACL + state file update.
fn probe_add_read_dir(workspace: &std::path::Path, home: &std::path::Path) -> ProbeResult {
    let id = "add_read_dir";
    let target = workspace.join("read-grant-probe");
    if let Err(err) = std::fs::create_dir_all(&target) {
        return ProbeResult {
            id,
            pass: false,
            detail: format!("mkdir error: {err}"),
        };
    }
    match add_session_read_dir(home, &target) {
        Ok(granted) => {
            let state = home.join(".sandbox").join("system_read_grants.json");
            let state_ok = std::fs::read_to_string(&state)
                .map(|txt| txt.contains("read-grant-probe"))
                .unwrap_or(false);
            ProbeResult {
                id,
                pass: state_ok,
                detail: format!("granted={} state_ok={state_ok}", granted.display()),
            }
        }
        Err(err) => ProbeResult {
            id,
            pass: false,
            detail: format!("add_session_read_dir error: {err:#}"),
        },
    }
}

fn probe_spawn_denial_code(workspace: &std::path::Path) -> ProbeResult {
    let id = "spawn_denial_code";
    let fake_exe = workspace.join("g2-not-an-exe.txt");
    if let Err(err) = std::fs::write(&fake_exe, "not executable") {
        return ProbeResult {
            id,
            pass: false,
            detail: format!("fixture write error: {err}"),
        };
    }
    let plan = match plan_exec(PlanInput {
        program: fake_exe.to_string_lossy().into(),
        args: vec![],
        cwd: workspace.to_path_buf(),
        env: HashMap::new(),
        writable_roots: vec![workspace.to_path_buf()],
        protected_write_paths: protected_subdirs_for_root(workspace),
        // Online user so logon succeeds; denial must come from CreateProcessAsUserW
        // on a non-executable path (runner IPC / PR-2.13), not from logon lockout.
        network_allowed: true,
        mode: WindowsSandboxMode::Elevated,
        private_desktop: false,
        tty: false,
    }) {
        Ok(plan) => plan,
        Err(err) => {
            return ProbeResult {
                id,
                pass: false,
                detail: format!("plan error: {err:#}"),
            };
        }
    };
    match spawn_sync(&plan, None, Some(Duration::from_secs(15))) {
        Ok(out) => ProbeResult {
            id,
            pass: false,
            detail: format!(
                "expected spawn denial, got exit={} stdout={:?}",
                out.exit_code,
                tail(&out.stdout, 80)
            ),
        },
        Err(err) => {
            let code = extract_spawn_denial_code(&err);
            ProbeResult {
                id,
                pass: code.is_some(),
                detail: format!("denial_code={code:?} err={err:#}"),
            }
        }
    }
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
