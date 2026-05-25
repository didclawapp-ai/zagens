# A2 — Turn 可观测 v1 草案（内部 + L2 对齐）

> **状态：** 草案（A2.3）；L2 正式 SSOT 仍为 [API_DESIGN.md](../API_DESIGN.md) §3.2.1。  
> **路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §7.2 A2.1–A2.3

## 1. 目的

单条 turn 可从 **日志 / EngineEvent / runtime 事件** 回答：

- 几步（`step_count`）
- 哪些工具（`tool_names`）
- 为何结束（`end_reason` + `TurnOutcomeStatus`）

## 2. L1 — Engine 内部

| 来源 | 字段 |
|------|------|
| `Event::TurnComplete` | `step_count`, `tool_names`, `end_reason`, `status`, `usage` |
| `deepseek_core::events::TurnSummary` | 与上表前三项同构；`to_value()` 供 runtime |
| `tracing` | `turn loop start` / `turn step`；`turn_streaming` / `turn_tools` spans；`turn complete`（engine + `monitor_turn` 含 `thread_id`） |

**代码：** `crates/core/src/events.rs`（`TurnSummary`）、`turn_loop/run.rs`、`message_handlers.rs`、`runtime_threads/monitor.rs`。

## 3. L2 — Sidecar / SSE（已发布）

`turn.completed` payload 可选 `turn_summary` 子对象 — 见 [API_DESIGN.md](../API_DESIGN.md) §3.2.1。

`monitor_turn` 在 `EngineEvent::TurnComplete` 时构造 `TurnSummary` 并写入 `turn.completed` JSONL + SSE compat。

## 4. 验收（A2 抽查）

- [x] 回归测 `turn_completed_event_includes_turn_summary`（runtime JSONL）
- [x] `map_compat_stream_event` 测 `turn.completed` + `turn_summary`（`runtime_api/stream.rs`）
- [x] 维护者：启用 `RUST_LOG=info` 跑单轮 turn，日志含 `turn complete` + `step_count` / `tools` — [A2_A3_SIGNOFF.md](./A2_A3_SIGNOFF.md) 2026-05-25

## 5. 非目标（本草案）

- 扩展 SSE 事件名（属 A+ / 桌面 client 变更）
- app-server turn 路径（D4 冻结）
