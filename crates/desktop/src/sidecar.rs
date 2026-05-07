use std::time::Duration;

use anyhow::{Context, Result};
use tauri::AppHandle;
use tokio::process::Command;
use tokio::time::sleep;
use reqwest::StatusCode;

const HEALTH_CHECK_INTERVAL_SECS: u64 = 5;
const MAX_STARTUP_RETRIES: u32 = 10;
const STARTUP_RETRY_DELAY_MS: u64 = 1000;
const MAX_HEALTH_FAILURES: u32 = 3;

async fn is_healthy(port: u16, _token: &str) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");
    let client = reqwest::Client::new();
    match client.get(&url).timeout(Duration::from_secs(2)).send().await {
        Ok(resp) => resp.status() == StatusCode::OK,
        Err(_) => false,
    }
}

pub async fn start_and_monitor(_app: &AppHandle, port: u16, token: &str) -> Result<()> {
    if is_healthy(port, token).await {
        return Ok(());
    }

    let deepseek_bin = find_deepseek_binary();
    let mut child = Command::new(&deepseek_bin)
        .args([
            "serve",
            "--http",
            "--host",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--auth-token",
            token,
        ])
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("failed to start sidecar: {deepseek_bin}"))?;

    for i in 0..MAX_STARTUP_RETRIES {
        sleep(Duration::from_millis(STARTUP_RETRY_DELAY_MS)).await;
        if is_healthy(port, token).await {
            break;
        }
        if i == MAX_STARTUP_RETRIES - 1 {
            child.kill().await.ok();
            anyhow::bail!("sidecar failed to become healthy after {MAX_STARTUP_RETRIES} retries");
        }
    }

    let mut failures = 0u32;
    loop {
        sleep(Duration::from_secs(HEALTH_CHECK_INTERVAL_SECS)).await;
        if is_healthy(port, token).await {
            failures = 0;
            continue;
        }
        failures += 1;
        if failures < MAX_HEALTH_FAILURES {
            continue;
        }

        child.kill().await.ok();
        child = Command::new(&deepseek_bin)
            .args([
                "serve",
                "--http",
                "--host",
                "127.0.0.1",
                "--port",
                &port.to_string(),
                "--auth-token",
                token,
            ])
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("failed to restart sidecar: {deepseek_bin}"))?;

        for i in 0..MAX_STARTUP_RETRIES {
            sleep(Duration::from_millis(STARTUP_RETRY_DELAY_MS)).await;
            if is_healthy(port, token).await {
                break;
            }
            if i == MAX_STARTUP_RETRIES - 1 {
                child.kill().await.ok();
                anyhow::bail!("sidecar failed to become healthy after restart");
            }
        }
        failures = 0;
    }
}

fn find_deepseek_binary() -> String {
    let candidates = [
        "deepseek-tui",
        "deepseek",
    ];

    for name in &candidates {
        if std::process::Command::new(name)
            .arg("--version")
            .output()
            .is_ok()
        {
            return name.to_string();
        }
    }

    let profile = if cfg!(debug_assertions) { "debug" } else { "release" };
    for name in &candidates {
        let path = format!("../../target/{profile}/{name}");
        if std::path::Path::new(&path).exists() || std::path::Path::new(&format!("{path}.exe")).exists() {
            return path;
        }
    }

    candidates[0].to_string()
}
