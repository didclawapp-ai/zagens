//! D6 Phase A+ — spawn the real `deepseek-runtime` binary and run the sidecar contract.
//!
//! Complements `deepseek_runtime::runtime_api::tests::sidecar_contract_full_lifecycle` (in-process axum).

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use uuid::Uuid;

const READY_TIMEOUT: Duration = Duration::from_secs(120);

async fn wait_for_ready(child: &mut Child) -> Result<(u16, String)> {
    let stdout = child.stdout.take().context("sidecar stdout not captured")?;
    let mut lines = BufReader::new(stdout).lines();

    let ready = timeout(READY_TIMEOUT, async {
        while let Some(line) = lines.next_line().await? {
            if let Some(json_str) = line.strip_prefix("DS_PICK_READY ") {
                let v: serde_json::Value =
                    serde_json::from_str(json_str).context("parse DS_PICK_READY JSON")?;
                let port = v["port"].as_u64().context("DS_PICK_READY missing port")? as u16;
                let token_fp = v["token_fp"].as_str().unwrap_or("").to_string();
                return Ok((port, token_fp));
            }
        }
        anyhow::bail!("sidecar exited without DS_PICK_READY")
    })
    .await
    .context("timed out waiting for DS_PICK_READY")??;

    Ok(ready)
}

fn write_test_config(path: &PathBuf) -> Result<()> {
    std::fs::write(
        path,
        r#"
[capacity]
enabled = false
"#,
    )
    .context("write test config.toml")
}

#[tokio::test]
async fn sidecar_binary_contract_full_lifecycle() -> Result<()> {
    let token = Uuid::new_v4().to_string();
    let root = std::env::temp_dir().join(format!("deepseek-runtime-bin-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).context("create temp root")?;
    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir).context("create runtime dir")?;
    let config_path = root.join("config.toml");
    write_test_config(&config_path)?;

    let bin = env!("CARGO_BIN_EXE_zagens-runtime");

    let mut child = Command::new(bin)
        .args([
            "--host",
            "127.0.0.1",
            "--port",
            "0",
            "--config",
            config_path.to_str().context("config path utf8")?,
            "--workspace",
            root.to_str().context("workspace path utf8")?,
            "--auth-token",
            &token,
        ])
        .env("DEEPSEEK_RUNTIME_TOKEN", &token)
        .env("DEEPSEEK_RUNTIME_DIR", &runtime_dir)
        .env(
            "DEEPSEEK_TASKS_DIR",
            root.join("tasks").to_str().context("tasks dir utf8")?,
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawn deepseek-runtime")?;

    let (port, _token_fp) = wait_for_ready(&mut child).await?;
    let base = format!("http://127.0.0.1:{port}");
    let client = reqwest::Client::new();

    // 1. Health (no auth)
    let health: serde_json::Value = client
        .get(format!("{base}/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(health["status"], "ok");
    assert!(health.get("event_schema_version").is_some());

    let auth = format!("Bearer {token}");

    // 2. Create thread
    let thread: serde_json::Value = client
        .post(format!("{base}/v1/threads"))
        .header("Authorization", &auth)
        .json(&json!({"model": "deepseek-chat"}))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let thread_id = thread["id"]
        .as_str()
        .context("missing thread id")?
        .to_string();

    // 3. Start turn
    let turn: serde_json::Value = client
        .post(format!("{base}/v1/threads/{thread_id}/turns"))
        .header("Authorization", &auth)
        .json(&json!({
            "prompt": "binary contract test turn",
            "model": "deepseek-chat",
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let turn_id = turn["turn"]["id"]
        .as_str()
        .context("missing turn id")?
        .to_string();

    // 4. SSE replay subset
    let events_url = format!("{base}/v1/threads/{thread_id}/events?replay_only=1");
    let events_resp = client
        .get(&events_url)
        .header("Authorization", &auth)
        .send()
        .await?
        .error_for_status()?;
    let events_body = events_resp.text().await?;
    assert!(
        events_body.contains("event: turn.started")
            || events_body.contains("event: turn.completed"),
        "expected SSE replay events, got: {}",
        &events_body[..events_body.len().min(200)]
    );

    // 5. Interrupt
    let interrupt_resp = client
        .post(format!(
            "{base}/v1/threads/{thread_id}/turns/{turn_id}/interrupt"
        ))
        .header("Authorization", &auth)
        .send()
        .await?;
    assert!(
        interrupt_resp.status().is_success() || interrupt_resp.status() == StatusCode::CONFLICT,
        "interrupt should succeed or 409; got {}",
        interrupt_resp.status()
    );

    let _ = child.kill().await;
    let _ = child.wait().await;

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}
