# Kernel v3 replay golden fixtures (Phase 3a/3b)

Synthetic `KernelEvent` sequences for projection/replay CI. Each file is a JSON
array of tagged `KernelEvent` records (same shape as `kernel_events.payload`).

| File | Scenario |
|------|----------|
| `pure_read.json` | Single turn, read-only tool batch |
| `write_batch.json` | Turn with mutating tool + deferred activation |
| `lht_continue.json` | Step-limit continuation + steer injection |

Run: `cargo test -p zagens-core golden_replay`
