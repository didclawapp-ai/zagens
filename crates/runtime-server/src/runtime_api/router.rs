//! HTTP route table for the runtime API (R-003 A4.1).

use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};

use zagens_runtime_api::{compose_router, require_runtime_token};

use super::stream;
use super::{
    RuntimeApiState, add_mcp_server, browse_thread_workspace, browse_workspace_by_root,
    cancel_task, clear_tasks, compact_thread, create_automation, create_night_queue_task,
    create_skill, create_task, create_thread, delete_automation, delete_mcp_server, delete_session,
    delete_thread_config_field, discover_mcp, edit_last_thread_turn, fork_thread,
    fork_thread_at_user_message, get_agent_health, get_automation, get_blackboard,
    get_kernel_thread_replay, get_kernel_turn_replay, get_mcp_server, get_night_queue,
    get_office_environment, get_resume_task, get_routing_rules, get_runtime_active_turns,
    get_session, get_task, get_thread, get_thread_checklist, get_thread_config, get_thread_context,
    get_thread_context_breakdown, get_thread_harness_cycles, get_thread_harness_task_graph,
    get_thread_scratchpad_status, get_thread_trace_report, get_topic_memory, get_trace_compare,
    get_usage, import_skill_local, init_thread_scratchpad, install_skill_remote,
    interrupt_thread_turn, list_automation_runs, list_automations, list_blackboards,
    list_gate_presets, list_mcp_calls, list_mcp_servers, list_mcp_tools, list_sessions,
    list_skills, list_tasks, list_thread_snapshots, list_threads, list_threads_summary,
    merge_mcp_config_json, pause_automation, persist_thread_session, post_night_queue_briefing,
    post_thread_channel_event, put_thread_config, read_thread_workspace_file,
    read_workspace_file_by_root, rebuild_symbol_index, reload_mcp_config, resolve_approval,
    restore_thread_snapshot, resume_automation, resume_session_thread, resume_thread,
    revert_thread_workspace_turn, run_automation, run_night_queue, search_symbol_index,
    set_routing_rules, start_thread_turn, steer_thread_turn, update_automation, update_mcp_server,
    update_thread, workspace_status,
};

pub fn build_router(state: RuntimeApiState) -> Router {
    let cors_origins = state.cors_origins.clone();
    let api_routes = Router::new()
        .route("/v1/sessions", get(list_sessions))
        .route("/v1/sessions/{id}", get(get_session).delete(delete_session))
        .route(
            "/v1/sessions/{id}/resume-thread",
            post(resume_session_thread),
        )
        .route("/v1/resume-tasks/{thread_id}", get(get_resume_task))
        .route("/v1/workspace/status", get(workspace_status))
        .route("/v1/runtime/active-turns", get(get_runtime_active_turns))
        .route(
            "/v1/runtime/kernel-replay/thread/{thread_id}",
            get(get_kernel_thread_replay),
        )
        .route(
            "/v1/runtime/kernel-replay/{turn_id}",
            get(get_kernel_turn_replay),
        )
        .route("/v1/office/environment", get(get_office_environment))
        .route("/v1/workspace/browse", get(browse_workspace_by_root))
        .route("/v1/workspace/file", get(read_workspace_file_by_root))
        .route("/v1/stream", post(stream::stream_turn))
        .route("/v1/threads", get(list_threads).post(create_thread))
        .route("/v1/threads/summary", get(list_threads_summary))
        .route("/v1/threads/{id}", get(get_thread).patch(update_thread))
        .route(
            "/v1/threads/{id}/config",
            get(get_thread_config).put(put_thread_config),
        )
        .route(
            "/v1/threads/{id}/config/{field}",
            delete(delete_thread_config_field),
        )
        .route("/v1/threads/{id}/checklist", get(get_thread_checklist))
        .route(
            "/v1/threads/{id}/harness/task-graph",
            get(get_thread_harness_task_graph),
        )
        .route(
            "/v1/threads/{id}/harness/cycles",
            get(get_thread_harness_cycles),
        )
        .route(
            "/v1/threads/{id}/scratchpad/status",
            get(get_thread_scratchpad_status),
        )
        .route(
            "/v1/threads/{id}/scratchpad/init",
            post(init_thread_scratchpad),
        )
        .route("/v1/threads/{id}/context", get(get_thread_context))
        .route(
            "/v1/threads/{id}/context/breakdown",
            get(get_thread_context_breakdown),
        )
        .route("/v1/threads/{id}/resume", post(resume_thread))
        .route("/v1/threads/{id}/fork", post(fork_thread))
        .route(
            "/v1/threads/{id}/fork-at-user-message",
            post(fork_thread_at_user_message),
        )
        .route(
            "/v1/threads/{id}/edit-last-turn",
            post(edit_last_thread_turn),
        )
        .route("/v1/threads/{id}/turns", post(start_thread_turn))
        .route(
            "/v1/threads/{id}/turns/{turn_id}/steer",
            post(steer_thread_turn),
        )
        .route(
            "/v1/threads/{id}/turns/{turn_id}/resolve-approval",
            post(resolve_approval),
        )
        .route(
            "/v1/threads/{id}/turns/{turn_id}/interrupt",
            post(interrupt_thread_turn),
        )
        .route("/v1/threads/{id}/compact", post(compact_thread))
        .route(
            "/v1/threads/{id}/persist-session",
            post(persist_thread_session),
        )
        .route("/v1/threads/{id}/snapshots", get(list_thread_snapshots))
        .route(
            "/v1/threads/{id}/snapshots/restore",
            post(restore_thread_snapshot),
        )
        .route(
            "/v1/threads/{id}/workspace/revert-turn",
            post(revert_thread_workspace_turn),
        )
        .route(
            "/v1/threads/{id}/workspace/browse",
            get(browse_thread_workspace),
        )
        .route(
            "/v1/threads/{id}/workspace/file",
            get(read_thread_workspace_file),
        )
        .route(
            "/v1/threads/{id}/events",
            get(stream::stream_thread_events).post(post_thread_channel_event),
        )
        .route(
            "/v1/events/status",
            get(stream::stream_global_status_events),
        )
        .route(
            "/v1/threads/{id}/trace-report",
            get(get_thread_trace_report),
        )
        .route("/v1/trace/compare", get(get_trace_compare))
        .route("/v1/tasks", get(list_tasks).post(create_task))
        .route("/v1/tasks/clear", post(clear_tasks))
        .route("/v1/tasks/{id}", get(get_task))
        .route("/v1/tasks/{id}/cancel", post(cancel_task))
        .route("/v1/blackboards", get(list_blackboards))
        .route("/v1/blackboards/{id}", get(get_blackboard))
        .route("/v1/topic-memory", get(get_topic_memory))
        .route("/v1/skills", get(list_skills).post(create_skill))
        .route("/v1/skills/import", post(import_skill_local))
        .route("/v1/skills/install", post(install_skill_remote))
        .route(
            "/v1/apps/mcp/servers",
            get(list_mcp_servers).post(add_mcp_server),
        )
        .route(
            "/v1/apps/mcp/servers/{name}",
            get(get_mcp_server)
                .put(update_mcp_server)
                .delete(delete_mcp_server),
        )
        .route("/v1/apps/mcp/config/merge", post(merge_mcp_config_json))
        .route("/v1/apps/mcp/reload", post(reload_mcp_config))
        .route("/v1/apps/mcp/discover", get(discover_mcp))
        .route("/v1/apps/mcp/calls", get(list_mcp_calls))
        .route("/v1/apps/mcp/tools", get(list_mcp_tools))
        .route(
            "/v1/automations",
            get(list_automations).post(create_automation),
        )
        .route(
            "/v1/automations/{id}",
            get(get_automation)
                .patch(update_automation)
                .delete(delete_automation),
        )
        .route("/v1/automations/{id}/run", post(run_automation))
        .route("/v1/automations/{id}/pause", post(pause_automation))
        .route("/v1/automations/{id}/resume", post(resume_automation))
        .route("/v1/automations/{id}/runs", get(list_automation_runs))
        .route("/v1/usage", get(get_usage))
        .route("/v1/agent-health", get(get_agent_health))
        .route(
            "/v1/night-queue",
            get(get_night_queue).post(create_night_queue_task),
        )
        .route("/v1/night-queue/run", post(run_night_queue))
        .route("/v1/night-queue/briefing", post(post_night_queue_briefing))
        .route("/v1/night-queue/gate-presets", get(list_gate_presets))
        .route(
            "/v1/apps/routing/rules",
            get(get_routing_rules).put(set_routing_rules),
        )
        .route("/v1/symbol-index/rebuild", post(rebuild_symbol_index))
        .route("/v1/symbol-index/search", get(search_symbol_index))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            require_runtime_token::<RuntimeApiState>,
        ));

    compose_router(state, &cors_origins, api_routes)
}
