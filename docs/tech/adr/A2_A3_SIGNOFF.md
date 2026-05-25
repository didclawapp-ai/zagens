# A2 / A3 — §12.1 #2/#3 维护者签收

> **日期：** 2026-05-25  
> **状态：** 维护者签收  
> **SSOT：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §7.2、§7.3、§12.1 #2/#3  
> **关联：** [A2_TURN_OBSERVABILITY_V1_DRAFT.md](./A2_TURN_OBSERVABILITY_V1_DRAFT.md)、`crates/core/src/error_taxonomy.rs`

## §12.1 #2 — A2 Turn 结构化可观测

**结论：** **✅ 达标**

| 验收项（§7.2） | 证据 |
|----------------|------|
| 单条 turn 可从日志 / EngineEvent 回答步数、工具、结束原因 | `TurnSummary` + `Event::TurnComplete`；`turn.completed` SSE 含 `turn_summary` |
| 自动化 | `turn_completed_event_includes_turn_summary`；`map_compat_stream_event` 测 |
| 维护者抽查 | `RUST_LOG=info` 单轮 turn：日志含 `turn complete` 与 `step_count` / tools（2026-05-25 通过） |

**A2.3 草案：** 仍为内部草案；L2 对外 SSOT 为 [API_DESIGN.md](../API_DESIGN.md) §3.2.1（不阻塞本签收）。

## §12.1 #3 — A3 错误分类与用户可见差异

**结论：** **✅ 达标**（A3.4 边缘 UI polish 记入 backlog，不阻塞）

| 验收项（§7.3） | 证据 |
|----------------|------|
| 网络断连 vs reasoning 400 等：用户可见文案不同 | `ErrorCategory` + `ErrorEnvelope`；TUI hint + HTTP wire 已统一核心路径 |
| 业务不可重试不刷 3 次 | `turn_loop` 仅对可重试类重试；36 golden/边界测绿 |
| 自动化 | `cargo test -p deepseek-core --lib error_taxonomy`（维护者 2026-05-25 通过） |

**遗留（非阻塞）：** A3.4 个别边缘 UI 文案可再抛光 — 产品 backlog。

## §12.1 签字对 #2/#3 的影响

| # | 标准 | 签收后状态 |
|---|------|------------|
| 2 | Turn 结构化可观测 | **✅** |
| 3 | 错误分类用户可见差异 | **✅** |

*注：§12.1 整体仍可能为 **🟡**（#1 A1 live 同构等余项）；不阻塞 A+/P2/F 门控。*

## 签收

| 角色 | 姓名 | 日期 |
|------|------|------|
| 维护者 | 维护者签收 | 2026-05-25 |
