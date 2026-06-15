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

Run: `cargo test -p zagens-core golden_replay`

Thread replay API: `GET /v1/runtime/kernel-replay/thread/{thread_id}` returns
`message_timeline` anchors. Optional `?session_message_count=N` adds structured
`message_coverage` and `message_timeline_coverage` (session vs log counters and
timeline anchor coherence). `message_plane_index` summarizes rebuildable counters
and estimated minimum session depth.
