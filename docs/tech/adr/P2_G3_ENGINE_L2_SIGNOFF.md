# P2 G3 / §12.3 签收记录（Engine L2 终态）

> **日期：** 2026-05-23  
> **状态：** 维护者签收（实施决议）  
> **SSOT 边界：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11.0、§12.3  
> **Spike：** [P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md)

## G3 — §11.0 迁移边界 ADR 签收

**决议：** 接受 **L2 终态** 划分（与当前代码一致，不再以「整包 `Engine` struct 进 core」为 §12.3 唯一标准）。

| 归属 | 位置 | 说明 |
|------|------|------|
| Turn 执行主逻辑 | `deepseek-core::engine::turn_loop` | `handle_deepseek_turn` + `TurnLoopHost` ✅ |
| 类型 / session / 策略 | `deepseek-core` | chat、approval、dispatch、tool_catalog 等 ✅ |
| **`Engine` struct + 字段** | **`deepseek-tui`** | LlmClient、MCP、LSP、SubAgent、spawn — **L2 壳** |
| HTTP / SSE | `runtime_api` | 生产 sidecar 面 ✅ |
| 持久化 / 广播 | `RuntimeThreadManager` | JSONL + broadcast ✅ |
| 工具实现 | `tui/src/tools/*` | P2 不整体搬迁 ✅ |

**不再作为 P2 阻塞项：**

- 将 `Engine` 整 struct 迁入 `deepseek-core`（可选远期；非 D10 解冻前提）

**仍作为 P2 / 后续项：**

- `core::Runtime::handle_thread(Message)` 真 turn 委托（app-server 路径；生产为 HTTP sidecar）
- `StateStore` vs JSONL 双持久化统一 — **backlog**

## §12.3 P2 完成线 — 诚实状态（签收日）

| # | 标准 | 状态 |
|---|------|------|
| 1 | turn_loop 在 core；`engine.rs` **< 300 行** | ✅ ~201 行；逻辑在 core，struct 留 tui（见上） |
| 2 | 契约测 + 多窗口抽样 | ✅ 自动化 + [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md) |
| 3 | sidecar = `deepseek-tui`；`/v1/*` 无破坏性变更 | ✅ |

**§12.3 签字结论：** **有条件达标** — 按 L2 终态定义可进入 **D10 解冻评审**；`handle_thread` 与审批 UI 接线为 **解冻前/后首 PR**，不阻塞架构签收。

## 关联交付（同批）

- **审批接线：** `approval_policy` → Composer `autoApprove`；`turn_lifecycle` 读 config `ApprovalMode`
- **G2 手测：** §2 审批 UI 可在接线后复测

## 签收

| 角色 | 姓名 | 日期 |
|------|------|------|
| 维护者 | （项目维护者） | 2026-05-23 |
