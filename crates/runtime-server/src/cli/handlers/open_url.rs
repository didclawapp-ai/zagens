use anyhow::{Context, Result, bail};
use serde_json::json;

use zagens_config::{build_open_url, parse_open_url};

use crate::cli::args::OpenUrlArgs;

pub fn run(args: OpenUrlArgs) -> Result<()> {
    let link = parse_open_url(&args.url).map_err(|e| anyhow::anyhow!("{e}"))?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "url": args.url,
                "workspace": link.workspace_display(),
                "prompt": link.prompt,
                "taskType": link.task_type,
                "useWorktree": link.use_worktree,
                "canonical_url": build_open_url(
                    &link.workspace,
                    link.prompt.as_deref(),
                    link.task_type.as_deref(),
                    link.use_worktree.unwrap_or(false),
                ),
            }))?
        );
        if args.validate_only {
            return Ok(());
        }
    } else if args.validate_only {
        println!("✓ valid deep link");
        println!("  workspace: {}", link.workspace_display());
        if let Some(prompt) = &link.prompt {
            println!("  prompt: {prompt}");
        }
        if let Some(task_type) = &link.task_type {
            println!("  task_type: {task_type}");
        }
        if link.use_worktree == Some(true) {
            println!("  use_worktree: true");
        }
        return Ok(());
    }

    if args.validate_only {
        return Ok(());
    }

    launch_desktop(&args.url).context("failed to launch Zagens desktop")?;
    if !args.json {
        println!("Launched desktop with deep link.");
    }
    Ok(())
}

fn launch_desktop(url: &str) -> Result<()> {
    if let Ok(exe) = std::env::var("ZAGENS_DESKTOP_EXE")
        && !exe.trim().is_empty()
    {
        return spawn_desktop_exe(&exe, url);
    }

    for candidate in desktop_exe_candidates() {
        if candidate.is_file() {
            return spawn_desktop_exe(candidate.as_os_str().to_string_lossy().as_ref(), url);
        }
    }

    if open::that(url).is_ok() {
        return Ok(());
    }

    bail!(
        "Zagens desktop not found. Install the desktop app, set ZAGENS_DESKTOP_EXE, \
         or register the `zagens://` protocol handler."
    );
}

fn spawn_desktop_exe(exe: &str, url: &str) -> Result<()> {
    std::process::Command::new(exe)
        .arg(url)
        .spawn()
        .with_context(|| format!("spawn {exe}"))?;
    Ok(())
}

fn desktop_exe_candidates() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        if let Some(local) = dirs::data_local_dir() {
            out.push(local.join("Programs").join("Zagens").join("Zagens.exe"));
            out.push(local.join("Programs").join("zagens").join("zagens.exe"));
        }
    } else if cfg!(target_os = "macos") {
        out.push(std::path::PathBuf::from(
            "/Applications/Zagens.app/Contents/MacOS/Zagens",
        ));
    }
    out
}
