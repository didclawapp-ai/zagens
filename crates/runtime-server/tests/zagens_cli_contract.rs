//! Headless `zagens` CLI binary contract tests (Headless CLI Phase 3).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;
use uuid::Uuid;

const READY_TIMEOUT: Duration = Duration::from_secs(120);

async fn wait_for_ready(child: &mut Child) -> Result<u16> {
    let stdout = child.stdout.take().context("sidecar stdout not captured")?;
    let mut lines = BufReader::new(stdout).lines();

    let (port, _) = timeout(READY_TIMEOUT, async {
        while let Some(line) = lines.next_line().await? {
            let line = line.trim_end_matches('\r');
            if let Some(json_str) = line.strip_prefix("DS_PICK_READY ") {
                let v: serde_json::Value =
                    serde_json::from_str(json_str).context("parse DS_PICK_READY JSON")?;
                let port = v["port"].as_u64().context("DS_PICK_READY missing port")? as u16;
                return Ok((port, ()));
            }
        }
        anyhow::bail!("process exited without DS_PICK_READY")
    })
    .await
    .context("timed out waiting for DS_PICK_READY")??;

    Ok(port)
}

fn write_test_config(path: &Path) -> Result<()> {
    std::fs::write(
        path,
        r#"
[capacity]
enabled = false
"#,
    )
    .context("write test config.toml")
}

fn test_env(root: &Path, runtime_dir: &Path, config_path: &Path) -> Vec<(String, String)> {
    vec![
        (
            "DEEPSEEK_RUNTIME_DIR".into(),
            runtime_dir.to_string_lossy().into_owned(),
        ),
        (
            "DEEPSEEK_TASKS_DIR".into(),
            root.join("tasks").to_string_lossy().into_owned(),
        ),
        (
            "DEEPSEEK_CONFIG_PATH".into(),
            config_path.to_string_lossy().into_owned(),
        ),
    ]
}

#[tokio::test]
async fn zagens_serve_http_matches_runtime_health_contract() -> Result<()> {
    let token = Uuid::new_v4().to_string();
    let root = std::env::temp_dir().join(format!("zagens-serve-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).context("create temp root")?;
    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir).context("create runtime dir")?;
    let config_path = root.join("config.toml");
    write_test_config(&config_path)?;

    let bin = env!("CARGO_BIN_EXE_zagens");
    let mut child = Command::new(bin)
        .args([
            "serve",
            "--http",
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
        .envs(test_env(&root, &runtime_dir, &config_path))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("spawn zagens serve --http")?;

    let port = wait_for_ready(&mut child).await?;
    let client = reqwest::Client::new();
    let health: serde_json::Value = client
        .get(format!("http://127.0.0.1:{port}/health"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assert_eq!(health["status"], "ok");
    assert!(health.get("event_schema_version").is_some());

    let _ = child.kill().await;
    let _ = child.wait().await;
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[tokio::test]
async fn zagens_exec_json_one_shot_uses_mock_api() -> Result<()> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "chatcmpl-test",
            "model": "deepseek-chat",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "mock-cli-reply"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 2
            }
        })))
        .mount(&server)
        .await;

    let root = std::env::temp_dir().join(format!("zagens-exec-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).context("create temp root")?;
    let runtime_dir = root.join("runtime");
    std::fs::create_dir_all(&runtime_dir).context("create runtime dir")?;
    let config_path = root.join("config.toml");
    write_test_config(&config_path)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zagens"))
        .args([
            "exec",
            "say hi",
            "--json",
            "--config",
            config_path.to_str().context("config path utf8")?,
            "--workspace",
            root.to_str().context("workspace path utf8")?,
        ])
        .env("DEEPSEEK_API_KEY", "test-key")
        .env("DEEPSEEK_BASE_URL", server.uri())
        .envs(test_env(&root, &runtime_dir, &config_path))
        .output()
        .await
        .context("run zagens exec --json")?;

    assert!(
        output.status.success(),
        "exec --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse exec JSON")?;
    assert_eq!(report["mode"], "one-shot");
    assert_eq!(report["success"], true);
    assert!(
        report["content"]
            .as_str()
            .is_some_and(|text| text.contains("mock-cli-reply"))
    );

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[tokio::test]
async fn zagens_cli_doctor_json_smoke() -> Result<()> {
    let root = std::env::temp_dir().join(format!("zagens-doctor-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&root).context("create temp root")?;
    let config_path = root.join("config.toml");
    write_test_config(&config_path)?;

    let output = Command::new(env!("CARGO_BIN_EXE_zagens"))
        .args(["doctor", "--json"])
        .env("DEEPSEEK_CONFIG_PATH", &config_path)
        .current_dir(&root)
        .output()
        .await
        .context("run zagens doctor --json")?;

    assert!(
        output.status.success(),
        "doctor --json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("parse doctor JSON")?;
    assert!(report.get("version").is_some());
    assert!(report.get("workspace").is_some());
    assert!(report.get("mcp").is_some());

    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[tokio::test]
async fn zagens_cli_help_uses_zagens_branding() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_zagens"))
        .arg("--help")
        .output()
        .await
        .context("run zagens --help")?;
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("zagens"));
    assert!(stdout.contains("exec"));
    assert!(stdout.contains("doctor"));
    Ok(())
}

#[tokio::test]
async fn zagens_cli_no_subcommand_exits_nonzero() -> Result<()> {
    let output = Command::new(env!("CARGO_BIN_EXE_zagens"))
        .output()
        .await
        .context("run bare zagens")?;
    assert!(!output.status.success());
    Ok(())
}
