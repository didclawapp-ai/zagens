use std::collections::HashMap;
use std::fs;

use self::diagnostics::{mask_url_secrets, redact_body_preview};

#[test]
fn test_mcp_config_defaults() {
    let config = McpConfig::default();
    assert_eq!(config.timeouts.connect_timeout, 10);
    assert_eq!(config.timeouts.execute_timeout, 60);
    assert_eq!(config.timeouts.read_timeout, 120);
    assert!(config.servers.is_empty());
}

#[test]
fn test_mcp_config_parse() {
    let json = r#"{
        "timeouts": {
            "connect_timeout": 15,
            "execute_timeout": 90
        },
        "servers": {
            "test": {
                "command": "node",
                "args": ["server.js"],
                "env": {"FOO": "bar"}
            }
        }
    }"#;

    let config: McpConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.timeouts.connect_timeout, 15);
    assert_eq!(config.timeouts.execute_timeout, 90);
    assert_eq!(config.timeouts.read_timeout, 120); // default
    assert!(config.servers.contains_key("test"));

    let server = config.servers.get("test").unwrap();
    assert_eq!(server.command, Some("node".to_string()));
    assert_eq!(server.args, vec!["server.js"]);
    assert_eq!(server.env.get("FOO"), Some(&"bar".to_string()));
}

#[test]
fn test_mcp_config_parse_mcp_servers_alias_and_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp.json");
    fs::write(
        &path,
        r#"{
          "mcpServers": {
            "disabled": {
              "command": "node",
              "args": ["server.js"],
              "disabled": true
            }
          }
        }"#,
    )
    .unwrap();

    let cfg = load_config(&path).unwrap();
    assert!(cfg.servers.contains_key("disabled"));
    let snapshot = manager_snapshot_from_config(&path, true).unwrap();
    assert!(snapshot.restart_required);
    assert_eq!(snapshot.servers[0].name, "disabled");
    assert!(!snapshot.servers[0].enabled);
    assert_eq!(snapshot.servers[0].error.as_deref(), Some("disabled"));
}

#[test]
fn test_mcp_config_manager_actions_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mcp.json");

    assert_eq!(init_config(&path, false).unwrap(), McpWriteStatus::Created);
    assert_eq!(
        init_config(&path, false).unwrap(),
        McpWriteStatus::SkippedExists
    );

    add_server_config(
        &path,
        "local".to_string(),
        Some("node".to_string()),
        None,
        vec!["server.js".to_string()],
    )
    .unwrap();
    set_server_enabled(&path, "local", false).unwrap();
    let disabled = manager_snapshot_from_config(&path, true).unwrap();
    let local = disabled
        .servers
        .iter()
        .find(|server| server.name == "local")
        .unwrap();
    assert!(!local.enabled);
    assert_eq!(local.transport, "stdio");

    remove_server_config(&path, "local").unwrap();
    let removed = manager_snapshot_from_config(&path, true).unwrap();
    assert!(removed.servers.iter().all(|server| server.name != "local"));
}

#[test]
fn test_server_effective_timeouts() {
    let global = McpTimeouts::default();

    let server_with_override = McpServerConfig {
        command: Some("test".to_string()),
        args: vec![],
        env: HashMap::new(),
        url: None,
        transport: None,
        headers: HashMap::new(),
        auth: None,
        connect_timeout: Some(20),
        execute_timeout: None,
        read_timeout: Some(180),
        disabled: false,
        enabled: true,
        required: false,
        enabled_tools: Vec::new(),
        disabled_tools: Vec::new(),
    };

    assert_eq!(server_with_override.effective_connect_timeout(&global), 20);
    assert_eq!(server_with_override.effective_execute_timeout(&global), 60); // global default
    assert_eq!(server_with_override.effective_read_timeout(&global), 180);
}

#[test]
fn test_mcp_pool_is_mcp_tool() {
    assert!(McpPool::is_mcp_tool("mcp_filesystem_read"));
    assert!(McpPool::is_mcp_tool("mcp_git_status"));
    assert!(McpPool::is_mcp_tool("list_mcp_resources"));
    assert!(McpPool::is_mcp_tool("list_mcp_resource_templates"));
    assert!(McpPool::is_mcp_tool("read_mcp_resource"));
    assert!(!McpPool::is_mcp_tool("read_file"));
    assert!(!McpPool::is_mcp_tool("exec_shell"));
}

#[test]
fn test_format_tool_result_text() {
    let result = serde_json::json!({
        "content": [
            {"type": "text", "text": "Hello, world!"}
        ]
    });
    assert_eq!(format_tool_result(&result), "Hello, world!");
}

#[test]
fn test_format_tool_result_error() {
    let result = serde_json::json!({
        "isError": true,
        "content": [
            {"type": "text", "text": "Something went wrong"}
        ]
    });
    assert_eq!(format_tool_result(&result), "Error: Something went wrong");
}

#[test]
fn test_format_tool_result_multiple_content() {
    let result = serde_json::json!({
        "content": [
            {"type": "text", "text": "Line 1"},
            {"type": "text", "text": "Line 2"},
            {"type": "image", "data": "base64..."}
        ]
    });
    let formatted = format_tool_result(&result);
    assert!(formatted.contains("Line 1"));
    assert!(formatted.contains("Line 2"));
    assert!(formatted.contains("[image content]"));
}

#[test]
fn test_is_tool_error_flag() {
    let ok = serde_json::json!({ "content": [{"type": "text", "text": "done"}] });
    assert!(!is_tool_error(&ok));

    let err = serde_json::json!({
        "isError": true,
        "content": [{"type": "text", "text": "boom"}]
    });
    assert!(is_tool_error(&err));
}

#[test]
fn test_extract_tool_content_drops_envelope() {
    // Only the text blocks survive — the isError/meta envelope is stripped.
    let err = serde_json::json!({
        "isError": true,
        "content": [{"type": "text", "text": "boom"}],
        "meta": {"trace": "abc"}
    });
    assert_eq!(extract_tool_content(&err), "boom");
}

#[test]
fn test_extract_tool_content_non_text_block() {
    let result = serde_json::json!({
        "content": [
            {"type": "image", "data": "base64..."},
            {"type": "text", "text": "caption"}
        ]
    });
    assert_eq!(extract_tool_content(&result), "[image content]\ncaption");
}

#[test]
fn test_parse_prefixed_name_underscore_server() {
    let config: McpConfig = serde_json::from_str(
        r#"{
            "servers": {
                "github_mcp": { "command": "node" },
                "git": { "command": "node" }
            }
        }"#,
    )
    .unwrap();
    let pool = McpPool::new(config);

    // Server name with an underscore must not be split at the first `_`.
    assert_eq!(
        pool.parse_prefixed_name("mcp_github_mcp_search").unwrap(),
        ("github_mcp", "search")
    );
    // A shorter, overlapping prefix still resolves to the right server.
    assert_eq!(
        pool.parse_prefixed_name("mcp_git_status").unwrap(),
        ("git", "status")
    );
    // Tool names may also contain underscores.
    assert_eq!(
        pool.parse_prefixed_name("mcp_github_mcp_create_issue")
            .unwrap(),
        ("github_mcp", "create_issue")
    );
}

#[test]
fn test_parse_prefixed_name_unknown_server_falls_back() {
    let pool = McpPool::new(McpConfig::default());
    // Server not in config: fall back to first-underscore split.
    assert_eq!(
        pool.parse_prefixed_name("mcp_unknown_tool").unwrap(),
        ("unknown", "tool")
    );
    assert!(pool.parse_prefixed_name("notmcp_x").is_err());
    assert!(pool.parse_prefixed_name("mcp_noseparator").is_err());
}

#[test]
fn test_transport_kind_inference() {
    let cfg: McpConfig = serde_json::from_str(
        r#"{
            "servers": {
                "local":  { "command": "node" },
                "legacy": { "url": "https://example.com/sse" }
            }
        }"#,
    )
    .unwrap();
    assert_eq!(
        cfg.servers["local"].transport_kind().unwrap(),
        McpTransportKind::Stdio
    );
    // url without explicit transport stays SSE for backward compatibility.
    assert_eq!(
        cfg.servers["legacy"].transport_kind().unwrap(),
        McpTransportKind::Sse
    );
}

#[test]
fn test_transport_kind_explicit_and_alias() {
    let cfg: McpConfig = serde_json::from_str(
        r#"{
            "servers": {
                "stream": { "url": "https://example.com/mcp", "transport": "http" },
                "aliased": { "url": "https://example.com/mcp", "type": "streamable-http" },
                "forced_sse": { "url": "https://example.com/mcp", "type": "sse" }
            }
        }"#,
    )
    .unwrap();
    assert_eq!(
        cfg.servers["stream"].transport_kind().unwrap(),
        McpTransportKind::Http
    );
    assert_eq!(
        cfg.servers["aliased"].transport_kind().unwrap(),
        McpTransportKind::Http
    );
    assert_eq!(
        cfg.servers["forced_sse"].transport_kind().unwrap(),
        McpTransportKind::Sse
    );
}

#[tokio::test]
async fn test_mcp_pool_reload_config_diff() {
    let cfg_a: McpConfig = serde_json::from_str(
        r#"{
            "servers": {
                "alpha": { "command": "node", "args": ["a.js"] },
                "beta": { "command": "node", "args": ["b.js"] }
            }
        }"#,
    )
    .unwrap();
    let mut pool = McpPool::new(cfg_a);

    let cfg_b: McpConfig = serde_json::from_str(
        r#"{
            "servers": {
                "alpha": { "command": "node", "args": ["a.v2.js"] },
                "gamma": { "command": "node", "args": ["g.js"] }
            }
        }"#,
    )
    .unwrap();
    let report = pool.reload_config(cfg_b, false).await;
    assert_eq!(report.removed, vec!["beta".to_string()]);
    assert!(report.updated.contains(&"alpha".to_string()));
    assert!(pool.config().servers.contains_key("gamma"));
    assert!(!pool.config().servers.contains_key("beta"));
}

#[test]
fn test_merge_preserved_secrets_on_save() {
    let old = McpServerConfig {
        command: None,
        args: vec![],
        env: HashMap::new(),
        url: Some("https://example.com/mcp".to_string()),
        transport: Some("http".to_string()),
        headers: HashMap::new(),
        auth: Some(McpAuthConfig {
            auth_type: Some("bearer".to_string()),
            token: Some("secret-token".to_string()),
            header: None,
            api_key: None,
        }),
        connect_timeout: None,
        execute_timeout: None,
        read_timeout: None,
        disabled: false,
        enabled: true,
        required: false,
        enabled_tools: vec![],
        disabled_tools: vec![],
    };
    let mut new = old.redacted_for_display();
    assert!(new.auth.as_ref().unwrap().token.is_none());
    merge_preserved_secrets(&mut new, &old);
    assert_eq!(new.auth.as_ref().unwrap().token.as_deref(), Some("secret-token"));
}

#[test]
fn test_redacted_display_keeps_env_placeholder() {
    let cfg = McpServerConfig {
        command: None,
        args: vec![],
        env: HashMap::new(),
        url: Some("https://example.com/mcp".to_string()),
        transport: None,
        headers: HashMap::from([(
            "Authorization".to_string(),
            "Bearer ${MCP_TOKEN}".to_string(),
        )]),
        auth: None,
        connect_timeout: None,
        execute_timeout: None,
        read_timeout: None,
        disabled: false,
        enabled: true,
        required: false,
        enabled_tools: vec![],
        disabled_tools: vec![],
    };
    let redacted = cfg.redacted_for_display();
    assert_eq!(
        redacted.headers.get("Authorization").map(String::as_str),
        Some("Bearer ${MCP_TOKEN}")
    );
}

#[test]
fn test_transport_kind_invalid() {
    // Unknown transport label.
    let bad: McpConfig = serde_json::from_str(
        r#"{ "servers": { "x": { "url": "https://e.com", "transport": "carrier-pigeon" } } }"#,
    )
    .unwrap();
    assert!(bad.servers["x"].transport_kind().is_err());

    // http transport without a url.
    let no_url: McpConfig = serde_json::from_str(
        r#"{ "servers": { "x": { "command": "node", "transport": "http" } } }"#,
    )
    .unwrap();
    assert!(no_url.servers["x"].transport_kind().is_err());
}

#[test]
fn test_streamable_http_sse_body_parsing() {
    let client = reqwest::Client::new();
    let mut transport =
        self::transport::StreamableHttpTransport::new(client, "https://example.com/mcp".to_string());
    transport.enqueue_sse_body(
        "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n\
         event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n",
    );
    let first = transport.inbox.pop_front().unwrap();
    assert_eq!(first["id"], 1);
    assert_eq!(first["result"]["ok"], true);
    let second = transport.inbox.pop_front().unwrap();
    assert_eq!(second["method"], "notifications/progress");
}

#[test]
fn test_streamable_http_json_batch_parsing() {
    let client = reqwest::Client::new();
    let mut transport =
        self::transport::StreamableHttpTransport::new(client, "https://example.com/mcp".to_string());
    transport.enqueue_json_body(r#"[{"id":1,"result":1},{"id":2,"result":2}]"#);
    assert_eq!(transport.inbox.len(), 2);
    assert_eq!(transport.inbox[0]["id"], 1);
    assert_eq!(transport.inbox[1]["id"], 2);
}

#[tokio::test]
async fn test_mcp_pool_empty_config() {
    let pool = McpPool::new(McpConfig::default());
    assert!(pool.server_names().is_empty());
    assert!(pool.all_tools().is_empty());
}

#[test]
fn mask_url_secrets_strips_userinfo() {
    let masked = mask_url_secrets("https://user:s3cret@host.example/api?foo=bar");
    assert!(masked.contains("***"), "expected masked userinfo: {masked}");
    assert!(!masked.contains("s3cret"), "secret leaked: {masked}");
    assert!(masked.contains("host.example"), "host preserved: {masked}");
}

#[test]
fn mask_url_secrets_passes_through_clean_url() {
    assert_eq!(
        mask_url_secrets("https://api.example.com/mcp"),
        "https://api.example.com/mcp"
    );
}

#[test]
fn redact_body_preview_masks_bearer_token() {
    let redacted = redact_body_preview("Authorization: Bearer abc.def.ghi end");
    assert!(redacted.contains("Bearer ***"), "redacted: {redacted}");
    assert!(!redacted.contains("abc.def.ghi"), "leaked: {redacted}");
}

#[test]
fn redact_body_preview_masks_api_key_param() {
    let redacted = redact_body_preview("error message api_key=sk-12345&other=val");
    assert!(redacted.contains("api_key=***"), "redacted: {redacted}");
    assert!(!redacted.contains("sk-12345"), "leaked: {redacted}");
    assert!(
        redacted.contains("other=val"),
        "non-secret preserved: {redacted}"
    );
}

/// #420: `StdioTransport::shutdown` reaps the child process by sending
/// SIGTERM and giving it a brief grace period before drop fires SIGKILL.
/// The test spawns `cat` (which exits immediately on stdin EOF / SIGTERM)
/// and verifies the transport tears down cleanly. Unix-only because
/// SIGTERM doesn't exist on Windows; on Windows the test would just
/// duplicate the kill_on_drop path.
#[cfg(unix)]
#[tokio::test]
async fn stdio_transport_shutdown_terminates_child() {
    use tokio::process::Command as TokioCommand;
    let mut cmd = TokioCommand::new("cat");
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let mut child = cmd.spawn().expect("spawn cat");
    let pid = child.id().expect("child pid");
    let stdin = child.stdin.take().expect("child stdin");
    let stdout = child.stdout.take().expect("child stdout");
    let mut transport = StdioTransport {
        child,
        stdin,
        reader: tokio::io::BufReader::new(stdout),
    };

    // shutdown() should send SIGTERM and complete within the grace window.
    let start = std::time::Instant::now();
    transport.shutdown().await;
    let elapsed = start.elapsed();
    assert!(
        elapsed < STDIO_SHUTDOWN_GRACE + Duration::from_millis(500),
        "shutdown blocked beyond grace window: {elapsed:?}"
    );

    // The child should be reaped — kill(pid, 0) returning ESRCH means
    // the pid is gone. If it's still alive, kill(0) returns 0, which
    // means our shutdown didn't terminate it.
    // SAFETY: pid was just collected from a tokio Child we spawned.
    // libc::kill with signal 0 only checks pid existence and is
    // async-signal-safe.
    let still_alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    assert!(
        !still_alive,
        "child {pid} survived StdioTransport::shutdown — SIGTERM not delivered"
    );
}
