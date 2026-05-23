# P2 Desktop / `TurnLoopHost` — 架构 Spike（2026-05-23）

> **状态：** Spike 收官（文档 + 边界测试）  
> **关联：** [P2_PR4_SESSION_HANDOFF.md](./P2_PR4_SESSION_HANDOFF.md)、[P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md) §4、[RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §3.1

## 1. 问题

PR4 将 `handle_deepseek_turn<H: TurnLoopHost>` 迁入 `deepseek-core`，TUI 通过 `engine/turn_loop/host_impl.rs` 实现 `TurnLoopHost`。

**DS Pick（`deepseek-desktop`）是否需要在 crate 内再实现一份 `TurnLoopHost`？**

## 2. 结论（可签收）

| 断言 | 结论 |
|------|------|
| Desktop **不应** `path` 依赖 `deepseek-tui` 或链接 tui `Engine` | ✅ 生产路径已是 sidecar 进程 |
| Desktop 的「L2 宿主」 | **`deepseek-tui serve --http`**（`runtime_api` + `RuntimeThreadManager` + `spawn_engine`） |
| `TurnLoopHost` 在桌面路径上的落点 | Sidecar 内 **`impl TurnLoopHost for Engine`**（`host_impl.rs`），非 Tauri crate |
| 未来嵌入式 runtime（无 sidecar） | 需新 host 实现或共享 L2 crate；**不在 P2 PR4 范围** |

```
DS Pick WebView
  → Tauri (runtime_proxy, secrets, workspace)
    → HTTP/SSE 127.0.0.1  (deepseek-tui serve)
      → RuntimeThreadManager::start_turn
        → EngineHandle::start_turn (TurnEnginePort)
          → handle_deepseek_turn<Engine as TurnLoopHost>
```

## 3. 验证

| 检查 | 机制 |
|------|------|
| `deepseek-desktop/Cargo.toml` 无 `deepseek-tui` / `../tui` | `crates/desktop/tests/architecture_boundary.rs` |
| Turn 执行仍在 sidecar | 现有 Phase 1 冒烟：`startThreadTurn` / `pollThreadTurnEvents`（`web-ui`） |
| Core trait 与桌面解耦 | `TurnLoopHost` 仅在 `deepseek-core` + tui `host_impl`；desktop 不 import |

## 4. 非目标（本 spike）

- 在 `crates/desktop` 增加 `deepseek-core` 依赖并实现 `TurnLoopHost`
- 将 sidecar 合并进 Tauri 进程（违反路线 B / D1）
- PR4 深迁 `tool_catalog` / `tool_execution`（仍属 tui L2）

## 5. 后续（若路线变更）

若产品要求 **无 sidecar 嵌入式 Engine**：

1. 新建 `deepseek-runtime-host` 或扩展 `deepseek-core` 的 L2 适配层，实现 `TurnLoopHost`（工具/MCP/终端仍 L2）。
2. Desktop 仅依赖该适配层 + core，**仍不** 直接 `use` tui `Engine` 单体文件。
3. 回放/契约测（A5.5、A+.4）须覆盖新宿主。

---

*维护：路线 B 不变则本文保持「已收官」；嵌入式 runtime 立项时升 ADR 版本。*
