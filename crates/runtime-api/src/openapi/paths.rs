//! OpenAPI `paths` — kept in sync with `router.rs` (R-003 A4.1).

use serde_json::{Map, Value, json};

fn schema_ref(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

fn json_response(schema: &str) -> Value {
    json!({
        "description": "OK",
        "content": {
            "application/json": { "schema": schema_ref(schema) }
        }
    })
}

fn sse_response() -> Value {
    json!({
        "description": "Server-Sent Events stream",
        "content": {
            "text/event-stream": { "schema": { "type": "string" } }
        }
    })
}

fn operation(
    method: &str,
    operation_id: &str,
    summary: &str,
    body: Option<&str>,
    response: Value,
    secured: bool,
) -> (String, Value) {
    let mut op = Map::new();
    op.insert("operationId".into(), json!(operation_id));
    op.insert("summary".into(), json!(summary));
    if let Some(body_schema) = body {
        op.insert(
            "requestBody".into(),
            json!({
                "required": true,
                "content": {
                    "application/json": { "schema": schema_ref(body_schema) }
                }
            }),
        );
    }
    op.insert("responses".into(), json!({ "200": response }));
    if secured {
        op.insert("security".into(), json!([{ "BearerAuth": [] }]));
    }
    (method.into(), Value::Object(op))
}

fn json_op(
    method: &str,
    operation_id: &str,
    summary: &str,
    body: Option<&str>,
    schema: &str,
    secured: bool,
) -> (String, Value) {
    operation(
        method,
        operation_id,
        summary,
        body,
        json_response(schema),
        secured,
    )
}

fn path_item(ops: Vec<(String, Value)>) -> Value {
    let mut m = Map::new();
    for (k, v) in ops {
        m.insert(k, v);
    }
    Value::Object(m)
}

/// Build the OpenAPI `paths` object (Axum `{param}` syntax).
pub fn build_paths() -> Map<String, Value> {
    let mut paths = Map::new();
    let u = true;
    let mut add = |path: &str, ops: Vec<(String, Value)>| {
        paths.insert(path.into(), path_item(ops));
    };

    add(
        "/health",
        vec![json_op(
            "get",
            "health",
            "Liveness probe",
            None,
            "ErrorBody",
            false,
        )],
    );
    add(
        "/internal/probe",
        vec![json_op(
            "get",
            "internalProbe",
            "Internal readiness probe",
            None,
            "ErrorBody",
            false,
        )],
    );
    add(
        "/v1/sessions",
        vec![json_op(
            "get",
            "listSessions",
            "List saved sessions",
            None,
            "SessionsListResponse",
            u,
        )],
    );
    add(
        "/v1/sessions/{id}",
        vec![
            json_op(
                "get",
                "getSession",
                "Load session detail",
                None,
                "SessionDetailResponse",
                u,
            ),
            operation(
                "delete",
                "deleteSession",
                "Delete saved session",
                None,
                json!({ "description": "Deleted" }),
                u,
            ),
        ],
    );
    add(
        "/v1/sessions/{id}/resume-thread",
        vec![json_op(
            "post",
            "resumeSessionThread",
            "Resume session into runtime thread",
            None,
            "ResumeSessionResponse",
            u,
        )],
    );
    add(
        "/v1/resume-tasks/{thread_id}",
        vec![json_op(
            "get",
            "getResumeTask",
            "Resume seeding status",
            None,
            "ResumeSessionResponse",
            u,
        )],
    );
    add(
        "/v1/runtime/active-turns",
        vec![json_op(
            "get",
            "getRuntimeActiveTurns",
            "Runtime-wide active turns (restart gate)",
            None,
            "RuntimeActiveTurns",
            u,
        )],
    );
    add(
        "/v1/workspace/status",
        vec![json_op(
            "get",
            "workspaceStatus",
            "Workspace status",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/workspace/browse",
        vec![json_op(
            "get",
            "browseWorkspace",
            "Browse workspace",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/workspace/file",
        vec![json_op(
            "get",
            "readWorkspaceFile",
            "Read workspace file",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/stream",
        vec![operation(
            "post",
            "streamTurn",
            "Create thread and stream turn (SSE)",
            Some("StreamTurnRequest"),
            sse_response(),
            u,
        )],
    );
    add(
        "/v1/threads",
        vec![
            json_op(
                "get",
                "listThreads",
                "List threads",
                None,
                "ThreadRecord",
                u,
            ),
            json_op(
                "post",
                "createThread",
                "Create thread",
                Some("CreateThreadRequest"),
                "ThreadRecord",
                u,
            ),
        ],
    );
    add(
        "/v1/threads/summary",
        vec![json_op(
            "get",
            "listThreadsSummary",
            "Thread summaries",
            None,
            "ThreadSummary",
            u,
        )],
    );
    add(
        "/v1/threads/{id}",
        vec![
            json_op("get", "getThread", "Thread detail", None, "ThreadDetail", u),
            json_op(
                "patch",
                "updateThread",
                "Patch thread",
                Some("UpdateThreadRequest"),
                "ThreadRecord",
                u,
            ),
        ],
    );
    add(
        "/v1/threads/{id}/config",
        vec![
            json_op(
                "get",
                "getThreadConfig",
                "Per-session config (base/overlay/effective)",
                None,
                "ThreadConfigResponse",
                u,
            ),
            json_op(
                "put",
                "putThreadConfig",
                "Patch per-session config overlay (zero restart, next turn)",
                Some("ThreadConfigOverlay"),
                "ThreadConfigResponse",
                u,
            ),
        ],
    );
    add(
        "/v1/threads/{id}/config/{field}",
        vec![json_op(
            "delete",
            "deleteThreadConfigField",
            "Clear one overlay section (inherit global)",
            None,
            "ThreadConfigResponse",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/checklist",
        vec![json_op(
            "get",
            "getThreadChecklist",
            "Thread checklist",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/scratchpad/status",
        vec![json_op(
            "get",
            "getThreadScratchpadStatus",
            "Scratchpad status",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/context",
        vec![json_op(
            "get",
            "getThreadContext",
            "Thread context",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/resume",
        vec![json_op(
            "post",
            "resumeThread",
            "Resume thread",
            None,
            "ThreadRecord",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/fork",
        vec![json_op(
            "post",
            "forkThread",
            "Fork thread",
            None,
            "ThreadRecord",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/fork-at-user-message",
        vec![json_op(
            "post",
            "forkThreadAtUserMessage",
            "Fork at user message",
            None,
            "ThreadRecord",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/edit-last-turn",
        vec![json_op(
            "post",
            "editLastThreadTurn",
            "Edit last turn",
            None,
            "StartTurnResponse",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/turns",
        vec![json_op(
            "post",
            "startThreadTurn",
            "Start turn",
            Some("StartTurnRequest"),
            "StartTurnResponse",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/turns/{turn_id}/steer",
        vec![json_op(
            "post",
            "steerThreadTurn",
            "Steer in-flight turn",
            Some("SteerTurnRequest"),
            "TurnRecord",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/turns/{turn_id}/resolve-approval",
        vec![json_op(
            "post",
            "resolveApproval",
            "Resolve exec approval",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/turns/{turn_id}/interrupt",
        vec![json_op(
            "post",
            "interruptThreadTurn",
            "Interrupt turn",
            None,
            "TurnRecord",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/compact",
        vec![json_op(
            "post",
            "compactThread",
            "Compact thread context",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/persist-session",
        vec![json_op(
            "post",
            "persistThreadSession",
            "Persist thread to session file",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/snapshots",
        vec![json_op(
            "get",
            "listThreadSnapshots",
            "List snapshots",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/snapshots/restore",
        vec![json_op(
            "post",
            "restoreThreadSnapshot",
            "Restore snapshot",
            None,
            "ThreadDetail",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/workspace/browse",
        vec![json_op(
            "get",
            "browseThreadWorkspace",
            "Browse thread workspace",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/workspace/file",
        vec![json_op(
            "get",
            "readThreadWorkspaceFile",
            "Read thread workspace file",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/threads/{id}/events",
        vec![operation(
            "get",
            "streamThreadEvents",
            "Thread events SSE",
            None,
            sse_response(),
            u,
        )],
    );
    add(
        "/v1/tasks",
        vec![
            json_op("get", "listTasks", "List tasks", None, "TasksResponse", u),
            json_op("post", "createTask", "Create task", None, "TaskRecord", u),
        ],
    );
    add(
        "/v1/tasks/clear",
        vec![json_op(
            "post",
            "clearTasks",
            "Clear tasks",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/tasks/{id}",
        vec![json_op("get", "getTask", "Get task", None, "TaskRecord", u)],
    );
    add(
        "/v1/tasks/{id}/cancel",
        vec![json_op(
            "post",
            "cancelTask",
            "Cancel task",
            None,
            "TaskRecord",
            u,
        )],
    );
    add(
        "/v1/blackboards",
        vec![json_op(
            "get",
            "listBlackboards",
            "List blackboards",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/blackboards/{id}",
        vec![json_op(
            "get",
            "getBlackboard",
            "Get blackboard",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/topic-memory",
        vec![json_op(
            "get",
            "getTopicMemory",
            "Topic memory graph",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/skills",
        vec![
            json_op("get", "listSkills", "List skills", None, "ErrorBody", u),
            json_op("post", "createSkill", "Create skill", None, "ErrorBody", u),
        ],
    );
    add(
        "/v1/skills/import",
        vec![json_op(
            "post",
            "importSkillLocal",
            "Import local skill",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/skills/install",
        vec![json_op(
            "post",
            "installSkillRemote",
            "Install remote skill",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/apps/mcp/servers",
        vec![
            json_op(
                "get",
                "listMcpServers",
                "List MCP servers",
                None,
                "ErrorBody",
                u,
            ),
            json_op(
                "post",
                "addMcpServer",
                "Add MCP server",
                None,
                "ErrorBody",
                u,
            ),
        ],
    );
    add(
        "/v1/apps/mcp/servers/{name}",
        vec![
            json_op(
                "get",
                "getMcpServer",
                "Get MCP server",
                None,
                "ErrorBody",
                u,
            ),
            json_op(
                "put",
                "updateMcpServer",
                "Update MCP server",
                None,
                "ErrorBody",
                u,
            ),
            operation(
                "delete",
                "deleteMcpServer",
                "Delete MCP server",
                None,
                json!({ "description": "Deleted" }),
                u,
            ),
        ],
    );
    add(
        "/v1/apps/mcp/config/merge",
        vec![json_op(
            "post",
            "mergeMcpConfig",
            "Merge MCP config JSON",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/apps/mcp/tools",
        vec![json_op(
            "get",
            "listMcpTools",
            "List MCP tools",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/automations",
        vec![
            json_op(
                "get",
                "listAutomations",
                "List automations",
                None,
                "ErrorBody",
                u,
            ),
            json_op(
                "post",
                "createAutomation",
                "Create automation",
                None,
                "ErrorBody",
                u,
            ),
        ],
    );
    add(
        "/v1/automations/{id}",
        vec![
            json_op(
                "get",
                "getAutomation",
                "Get automation",
                None,
                "ErrorBody",
                u,
            ),
            json_op(
                "patch",
                "updateAutomation",
                "Update automation",
                None,
                "ErrorBody",
                u,
            ),
            operation(
                "delete",
                "deleteAutomation",
                "Delete automation",
                None,
                json!({ "description": "Deleted" }),
                u,
            ),
        ],
    );
    add(
        "/v1/automations/{id}/run",
        vec![json_op(
            "post",
            "runAutomation",
            "Run automation",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/automations/{id}/pause",
        vec![json_op(
            "post",
            "pauseAutomation",
            "Pause automation",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/automations/{id}/resume",
        vec![json_op(
            "post",
            "resumeAutomation",
            "Resume automation",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/automations/{id}/runs",
        vec![json_op(
            "get",
            "listAutomationRuns",
            "List automation runs",
            None,
            "ErrorBody",
            u,
        )],
    );
    add(
        "/v1/usage",
        vec![json_op(
            "get",
            "getUsage",
            "Usage aggregation",
            None,
            "UsageAggregation",
            u,
        )],
    );
    add(
        "/v1/apps/routing/rules",
        vec![
            json_op(
                "get",
                "getRoutingRules",
                "Get routing rules",
                None,
                "RoutingRulesDoc",
                u,
            ),
            json_op(
                "put",
                "setRoutingRules",
                "Set routing rules",
                Some("RoutingRulesDoc"),
                "RoutingRulesDoc",
                u,
            ),
        ],
    );
    add(
        "/v1/symbol-index/rebuild",
        vec![json_op(
            "post",
            "rebuildSymbolIndex",
            "Rebuild symbol index",
            None,
            "ErrorBody",
            u,
        )],
    );

    paths
}

/// Path templates registered in this module (guard drift vs `router.rs`).
pub fn path_template_count() -> usize {
    build_paths().len()
}
