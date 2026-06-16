# Kernel v3 replay golden fixtures (Phase 3a/3b)

Synthetic `KernelEvent` sequences for projection/replay CI. Each file is a JSON
array of tagged `KernelEvent` records (same shape as `kernel_events.payload`).

| File | Scenario |
|------|----------|
| `pure_read.json` | Single turn, read-only tool batch |
| `write_batch.json` | Turn with mutating tool + deferred activation |
| `lht_continue.json` | Step-limit continuation + steer injection |
| `loop_guard.json` | Loop guard triggered on duplicate tool call |
| `scratchpad_compaction.json` | Scratchpad + compaction + cycle briefing |
| `cycle_handoff.json` | LHT cycle advance + overflow cycle handoff |
| `overflow_recovery.json` | Budget recompile overflow recovery |
| `capacity_checkpoint.json` | Pre/post capacity checkpoints with trim |
| `manual_compaction.json` | Manual `/compact` compaction artifact |
| `deferred_activation.json` | Deferred tool promotion without tool batch |
| `memory_plane_query.json` | WorkingSet + TopicMemory queries + dual `MemoryPlaneQueried` |
| `resume_thread_parity.json` | Resume log vs session-direct parity gate (steer + read tool turn) |
| `layered_context_seam.json` | Flash L2 seam + `RunLayeredContextCheckpoint` replay anchor |
| `system_prompt_refresh.json` | v3 refresh chain (`user_memory` + `topic_episodic` + `RefreshSystemPrompt` replay) |
| `message_body_rebuild.json` | Log-driven transcript rebuild from preview fields (5c) |
| `message_body_rebuild.session.json` | Canonical session JSON for 5c byte-parity gate |
| `resume_thread_parity.session.json` | Canonical session JSON paired with `resume_thread_parity.json` |

Resume (`POST /v1/sessions/{id}/resume`) cross-checks session compaction artifacts
against kernel log `replaced_range` anchors when SQLite compaction rows exist.
Continuation steps are validated for `InjectSteer` replay effects on threads with
step-limit / loop-guard continuation events.

Run: `cargo test -p zagens-core golden_replay`

Thread replay API: `GET /v1/runtime/kernel-replay/thread/{thread_id}` returns
`message_timeline` anchors, `continuation_anchor_ok` (when continuation events exist),
and `notify_lsp_anchor_ok` (when tool batches ran). Optional `?session_message_count=N` adds structured
`message_coverage` and `message_timeline_coverage` (session vs log counters and
timeline anchor coherence). Optional `session_assistant_count` / `session_tool_result_count` / `session_text_user_count`
enable role and memory-plane checks without loading session bodies. `message_plane_index` summarizes
rebuildable counters and estimated minimum session depth. `compaction_timeline` lists compaction
artifact anchors when present.
