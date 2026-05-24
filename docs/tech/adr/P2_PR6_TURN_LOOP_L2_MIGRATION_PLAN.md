# P2 PR6 — Turn loop L2 阶段迁入 `deepseek-core`

> **日期：** 2026-05-24  
> **状态：** PR6a–d 已落地  
> **前置：** [P2_PR4_SESSION_HANDOFF.md](./P2_PR4_SESSION_HANDOFF.md)、[P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md)  
> **路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11、§12.3

---

## 1. 目标

把仍留在 `deepseek-tui` 的 turn loop **L2 阶段实现**迁入 `deepseek-core`，使 `handle_deepseek_turn` 的 streaming / tool 路径与 `TurnLoopHost` 同 crate，TUI 只保留 **Engine 字段接线**（`host_impl`）。

**不在本 PR 范围：** 整包 `Engine` struct 进 core、`capacity_flow` 全量端口、Desktop 第二 `TurnLoopHost` 实现。

---

## 2. 当前架构（PR6d 后）

```
deepseek-core::engine::handle_deepseek_turn<H: TurnLoopHost>
  ├─ streaming_phase::run_streaming_phase(host, …)     ← core
  ├─ capacity_policy::should_run_capacity_error_escalation  ← core（纯策略）
  └─ tool_phase::run_tool_execution_phase(host, …)     ← core（规划 + 结果）
        └─ host.execute_tool_plans(…)                   ← tui tool_plans_exec.rs
              ├─ parallel → detached_execute_with_lock (port.rs)
              └─ sequential → execute_plan_on_engine (port.rs)
```

`TurnLoopHost` 在 `host_impl/{mod,capacity,no_tool_uses}.rs`；`capacity_flow/` 用 `TurnLoopMode` + core 策略函数。

---

## 3. PR6 切片

| 切片 | 内容 | 验收 |
|------|------|------|
| **PR6a** | `tool_parser` → `core::engine::tool_parser`；`streaming_phase` → `core::engine::turn_loop`（泛型 `H: TurnLoopHost`） | ✅ |
| **PR6b** | `tool_phase`（规划 + 结果汇总）→ core；`tool_plans_exec`（执行）留 tui + `execute_tool_plans` 钩子 | ✅ `cargo test -p deepseek-core` / `deepseek-tui --lib` |
| **PR6c** | `run.rs` 直调 core 阶段；`host_impl/` 拆分；`tool_plans_exec` → `TurnLoopToolExecutor`/`detached_execute`；`protocol_recovery` 指向 core streaming | ✅ |
| **PR6d** | `capacity_policy`（core）；`capacity_flow` 用 `TurnLoopMode`；`execute_plan_on_engine`；`host_impl/capacity.rs` | ✅ |

---

## 4. PR6a 技术要点

### 4.1 `TurnLoopHost` 扩展（L2 钩子）

| 方法 | 理由 |
|------|------|
| `effective_reasoning_effort_for_request(&mut self)` | `auto_reasoning` 留在 tui；避免 `session_mut` + 闭包双借 |
| `parse_streaming_tool_input(&self, buffer)` | TUI `arg_repair` 包装 |
| `final_streaming_tool_input(&self, state)` | 同上 |

`run.rs` 直接调用 `streaming_phase::run_streaming_phase(host, …)`（**无** trait 默认实现，避免 `async_trait` + `Self` 尺寸问题）。

### 4.2 借用规则

`session_mut()` 与 `workspace()` / `strict_tool_mode()` 不可同时持有：先 `to_path_buf()` workspace，再 `session_mut()` 构建 `MessageRequest`。

### 4.3 日志

`crate::logging` → `tracing::{info,warn}`（core 已有 `tracing` 依赖）。

---

## 5. 风险与回滚

| 风险 | 缓解 |
|------|------|
| 流式工具 JSON 无 `arg_repair` | 钩子仍在 tui `host_impl` |
| 行为回归 | PR6a 后跑 `protocol_recovery` + engine 单测 |
| 文件过大 | `streaming_phase` ~700 行 — PR6b 后再评估拆 `stream_poll` 子模块 |

回滚：恢复 tui `turn_loop/streaming_phase.rs` 与 trait 默认实现删除。

---

## 6. 验证命令

```bash
cargo test -p deepseek-core --lib tool_parser
cargo test -p deepseek-core --lib capacity_policy
cargo test -p deepseek-tui --lib effective_max_output
cargo test -p deepseek-tui --lib capacity_escalation
cargo test -p deepseek-tui --lib capacity_disabled
cargo test -p deepseek-tui --test protocol_recovery
```

---

## 7. §12.3 关系

PR6 完成 **PR6a+PR6b** 后，turn loop **逻辑** 主体在 core；§12.3「L2 终态」仍要求 `host_impl` 薄化与多窗口回归 — 见 [P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)。
