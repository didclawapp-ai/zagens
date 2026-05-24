# D10 桌面 Feature freeze — 解冻评审记录

> **日期：** 2026-05-24  
> **状态：** **已签收**（维护者 Jason，2026-05-24）— D10 桌面 Feature freeze **已解除**  
> **依据：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §10.6、§12.3；[P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)；[P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md)

---

## 1. 结论（实施侧）

**§12.3 P2 完成线** 按 **L2 终态** 定义已满足；**D10 桌面 GAP 冻结可解除**，允许按 [TUI_DS_PICK_GAP.md](../../desktop/TUI_DS_PICK_GAP.md) 推进 **阶段 F**（F3 收尾、F4 待 L1 API）。

生产约束不变：Agent turn 仍在 **`deepseek-tui` sidecar**；桌面不复制 `turn_loop`。

---

## 2. §12.3 门控核对

| # | 标准 | 证据（2026-05-24） |
|---|------|-------------------|
| 1 | Engine/turn loop 在 core；`engine.rs` < 300 行 | `handle_deepseek_turn` + `streaming_phase` / `tool_phase` / `capacity_policy` 在 `deepseek-core`；`crates/tui/src/core/engine.rs` **~192 行** |
| 2 | `runtime_threads` 经 core 跑 turn；契约 + 多窗口测 | `TurnEnginePort` + `ThreadMessageTurnPort`；`parallel_turns_on_two_threads_*`、A+.7 审批测；[G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md) |
| 3 | sidecar = `deepseek-tui`；`/v1/*` 无破坏性变更 | `sidecar_contract_full_lifecycle`（CI）；HTTP 路由未换 binary |

**PR6（2026-05-24）：** turn loop L2 阶段主体在 core — 见 [P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md)。

---

## 3. 阶段 F 已落地 / 进行中

| 波次 | 状态 | 备注 |
|------|------|------|
| F0 路由 | ✅ | `start_turn_applies_route_intent_routing_rule_to_model` |
| F1a Terminal | ✅ | `TerminalCard` + 增量 `tool.progress` |
| F1b Diff | ✅ | `DiffCard` + 运行中预览 |
| F2 导出 / 资源管理器 | ✅ | `export_*_json`、`open_in_shell` |
| F3 a11y | 🟡 | Skip link、landmarks、roving tablist、reduced-motion；**ModelParamsDialog** dialog 语义（本批） |
| F4 内联编辑 | ⏸ | 依赖 L1「改历史」API |

**解冻后优先：** F3 收尾（键盘/对话框 a11y）→ F4 前置 API 设计 → GAP 表 ◐ 项验证。

---

## 4. 签收

| 角色 | 姓名 | 日期 | 备注 |
|------|------|------|------|
| 维护者 | Jason | 2026-05-24 | 已核对 §2–§3；§5 抽样测试通过；正式解除 D10 freeze |

---

## 5. 验证命令（回归抽样）

```powershell
cargo test -p deepseek-core --lib capacity_policy
cargo test -p deepseek-tui --lib capacity_escalation
cargo test -p deepseek-tui --lib history_isomorphism
cargo test -p deepseek-tui --test protocol_recovery
cargo test -p deepseek-tui --lib parallel_pending_approvals
```
