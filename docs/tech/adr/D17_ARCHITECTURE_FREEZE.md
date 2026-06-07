# D17 — 系统架构收尾与冻结（Architecture Freeze v1）

> **类型：** 架构签收 ADR（收尾 / 纪律化，非新功能主线）  
> **状态：** Accepted（2026-05-27）  
> **前置（已 Landed）：** D6 · D7 · D8 · D15 · D16 Checkpoint · [ARCHITECTURE_ASSESSMENT §1 = 10/10](./ARCHITECTURE_ASSESSMENT_2026-05-25.md)  
> **SSOT 架构图：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **不含：** Harness 引擎独立化（见 [docs/harness/](../../harness/README.md) 远景，**不在本 ADR 执行范围**）

---

## 0. TL;DR — 执行到哪里截止

| 问题 | 答案 |
|------|------|
| 架构还要大改吗？ | **不要。** 本 ADR 宣告 **Architecture Freeze v1**，重构主线关闭。 |
| D16 还要继续做吗？ | **不要继续 E1 深拆。** D16 以 **Checkpoint** 结案（见 §3）。 |
| 本 ADR 要执行什么？ | **仅 §5 可选收尾 PR**（文档已 Landed；F1/F2 按需、非阻塞）。 |
| 本 ADR 明确不执行什么？ | **§4 整表** — 含 E1 阶段 2、E4、D11–D14、Harness 分离等。 |
| 定型后主线的？ | **产品功能**，遵守 [Assessment §7.1](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) 红线。 |

**截止线（一句话）：** Assessment 10/10 + D15 legacy 清除 + D16 高 ROI 项 Landed + 三 crate 骨架（api / orchestrator / adapters）= **架构已定**；`runtime-server` 仍 ~10 万行、`client.ts`/`commands.rs` 仍单体 = **接受，不再作为架构 KPI**。

---

## 1. 背景

2026-05-26 前已完成：

- **D6：** `deepseek-runtime` sidecar，无 ratatui / CLI/TUI  
- **D7：** Thread/Event SSOT，Session 为投影  
- **D8：** OpenAPI + TS 生成 + contract CI  
- **D15：** 删除 `deepseek-state`、`core::Runtime`  
- **D16（部分）：** SubAgent / App hooks / OpenAPI CI / 三 crate 骨架  

维护者在 D16 E1 深拆中途暂停，评估认为 **继续整包迁移投入产出比低**。本 ADR 将「架构定型」从事实落实为 **团队可执行的截止规则**，避免无限重构。

---

## 2. 冻结后的目标架构（最终形态）

```text
┌─────────────────────────────────────────────────────────────┐
│  L3 — Zagens（唯一用户产品）                                  │
│  crates/desktop + web-ui                                     │
│  Tauri · sidecar 监督 · runtime_proxy · PTY · 密钥/文件      │
└──────────────────────────┬──────────────────────────────────┘
                           │ L2：localhost /v1/* + Bearer（Rust 注入）
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  L2 — deepseek-runtime（sidecar 二进制 + HTTP 网关）          │
│  crates/runtime-server                                       │
│  runtime_api/* · runtime_serve · tools/* · task_manager …    │
└──────────────┬──────────────────────────────┬─────────────────┘
               ▼                              ▼
    runtime-orchestrator              runtime-adapters
    （Thread/Turn 编排）               （MCP · persist · tool host 端口）
               └──────────────┬───────────────┘
                              ▼
                    deepseek-core（Engine · turn_loop · hosts）

持久化 SSOT（默认；可用 `DEEPSEEK_RUNTIME_DIR` / `DEEPSEEK_TASKS_DIR` 等覆盖，见 orchestrator/task 代码）：
  ~/.deepseek/tasks/runtime/runtime.db  ← Thread / Turn / Event
  ~/.deepseek/sessions/sessions.db      ← Session 投影（runtime_thread_id）
```

**Turn 唯一生产路径（不可改）：**

```text
HTTP handler
  → RuntimeThreadManager::start_turn （sidecar 薄委派）
  → runtime-orchestrator::turn_lifecycle::start_turn
  → TurnEnginePort::start_turn （deepseek-core：`EngineHandle` → Op::SendMessage）
  → runtime-server：EnginePlatformExt::dispatch_op → handle_send_message（宿主 glue）
  → deepseek-core::handle_deepseek_turn （回合 / streaming / tool 规划与结果）
```

换言之：**编排入口**始终在 `RuntimeThreadManager::start_turn`；**宿主 L2（sidecar `Engine`）不可绕开**，再进入 core 的 `handle_deepseek_turn`。不得新增「直连 core 回合」第二条生产路径。

---

## 3. D16 结案状态（执行截止明细）

### 3.1 ✅ 已执行完毕（Landed — 不再追加同类工作）

| 子项 | 截止状态 | 验收锚点 |
|------|----------|----------|
| **E2** SubAgent 拆模块 | **Landed** | `crates/runtime-server/src/tools/subagent/mod.rs` ~82 行（实现）；`deepseek-core` 侧 `subagent` 仅类型/re-export（薄层）；108 个 SubAgent 相关单元测试 |
| **E3** App.tsx hooks | **Landed** | `App.tsx` ~772 行；`AppShell` + hooks |
| **E5** OpenAPI contract CI | **Landed** | `scripts/check-openapi-contract.{ps1,sh}`；CI Ubuntu：`export-runtime-openapi` + `generate:api-types` diff gate；`sidecar_contract_*` |
| **E1-a 阶段 1** adapters 骨架 | **Landed** | `runtime-adapters`：MCP、persist、network_gate、tool host 端口 |
| **E1-b 阶段 1–5** orchestrator 核心 | **Landed** | `runtime-orchestrator`：manager、monitor、engine_load、task_port |
| **E1-c 阶段 1–6** runtime-api wire | **Landed** | auth/health/cors、task wire、OpenAPI SSOT |
| **E1-d 阶段 1–2** DI 组装 | **Landed** | `runtime_serve/http.rs`；`run_http_server` re-export |
| **E1 模块内拆分** | **Landed** | shell/file/web_run/skills/task_manager 子目录化 |

### 3.2 ⏸ 执行到此为止（Checkpoint — 明确不再做）

| 子项 | 原 D16 目标 | **截止决定** | 恢复条件 |
|------|-------------|--------------|----------|
| **E1 阶段 2** | `tools/` 整包迁 `runtime-adapters` | **暂停** | 单独 ADR：`ToolExecutionHost` + 无循环依赖 |
| **E1-b 阶段 6+** | `task_manager/` 整包迁 orchestrator | **暂停** | 同上或量化 PR 冲突瓶颈 |
| **E1-d 终态** | `runtime-server/lib.rs` <500 行 DI 壳 | **放弃为 KPI** | 不作为 Freeze 验收 |
| **E4** | `client.ts` 按 domain 拆分 | **Won't Do (v1)** | API 客户端变更成为主要瓶颈时单独立项 |

### 3.3 D16 整体状态

**Closed (Checkpoint)** — 详见 [D16_PHASE_E_MAINTAINABILITY.md](./D16_PHASE_E_MAINTAINABILITY.md) §5。

---

## 4. 本 ADR 及后续 — 明确不执行清单

以下工作 **不属于 Architecture Freeze v1**，**不得**以「完成 D16 / 架构收尾」为由启动：

| # | 主题 | 说明 |
|---|------|------|
| N1 | D16 E1 阶段 2（tools 整包迁移） | ToolContext ↔ adapters 循环依赖未解 |
| N2 | D16 E4（client.ts 拆分） | v1 接受 ~1470 行单体 |
| N3 | `runtime-server` 瘦至 <500 行 | 接受 ~10 万行 sidecar lib |
| N4 | `desktop/commands.rs` 拆分 | D1 已签收不拆（~1.5k） |
| N5 | D11 Prometheus metrics | P2 backlog |
| N6 | D12 每 workspace 一 sidecar | 需单独 ADR |
| N7 | D13 Capability Manifest | Harness 组合式装配，远景 |
| N8 | D14 MCP 池稳定性 | P2 backlog |
| N9 | Harness 引擎独立 repo/crate | 见 docs/harness，**非本 ADR** |
| N10 | Desktop 内嵌 runtime（去 HTTP hop） | v0.7+ 再评 |
| N11 | 重写 `/v1/*` 或 Engine turn 路径 | Assessment §7.1 禁止 |
| N12 | 恢复 CLI/TUI / `deepseek-state` / `core::Runtime` | D15/D6 已删除 |

**新会话默认假设：** 架构已冻结；若 PR 触碰 N1–N12，须 architecture owner 显式签收新 ADR。

---

## 5. D17 可选收尾 PR（非阻塞，执行上限）

本 ADR **文档签收即视为 D17 Landed**。下列 PR **可选**，**全部跳过也不影响 Freeze 宣告**：

| ID | 内容 | 执行上限 | 跳过后果 |
|----|------|----------|----------|
| **F0** | 本文 + 关联文档更新 | **已完成（本 commit）** | — |
| **F1** | stale `deepseek-tui` 生产注释清理 | 仅 `core/engine/*`、`subagent`、`sandbox`、`config` 注释；**不改行为** | ✅ Landed（2026-05-27） |
| **F2** | `scripts/check-architecture-freeze.{ps1,sh}` | 可选：串联 **`cargo test` 命中** `architecture_invariants`、`architecture_boundary`、以及你本机常用的 OpenAPI 检查脚本；**不新增 gate 规则**（本 workspace 主线 **无 ratatui 依赖**，勿再写「ratatui tree」类检查） | ✅ Landed（2026-05-27） |
| **F3** | 工作区 untracked 清扫 | 仅 D16 无关文件 | 无 |

**F1/F2 若在 2026-06-30 前未合并，视为放弃，不再追踪。**

---

## 6. 架构不变量（PR 审查 — 与 D15/D16 一致）

合并任何触及 `desktop` / `runtime-server` / `core` 边界的 PR 前必查：

| # | 规则 |
|---|------|
| I1 | `desktop` **不得** path-depend `runtime-server` / `deepseek-core`（**代码审查必看** `Cargo.toml`） |
| I2 | WebView **不得** 持有 runtime Bearer；经 `runtime_proxy` |
| I3 | 新 `/v1/*` **必须** OpenAPI + TS 再生 + contract test |
| I4 | **不得** 给 `Engine` struct 加字段 |
| I5 | **不得** 引入第三套持久化 SSOT |
| I6 | Turn **必须** 经 `RuntimeThreadManager::start_turn` |
| I7 | sidecar **`deepseek-runtime` / `runtime-server` 依赖图不得引入 ratatui**（当前 workspace 主线无该项；新增依赖须人工审查 manifest） |
| I8 | 新 `.rs`/`.tsx` >1000 行 → **优先同 crate 内拆模块**；**不**为此新建 crate |

已有 CI / 脚本护栏：

- `crates/runtime-server/tests/architecture_invariants.rs`（D15：`deepseek_state` / legacy port 等）
- `crates/desktop/tests/architecture_boundary.rs`（**自动化**：禁止 `../runtime-server`、`../core`、`deepseek-core`、禁止 `deepseek-tui` **库** path-dep）
- OpenAPI：`scripts/check-openapi-contract.{ps1,sh}`；**CI**（`ci.yml` Ubuntu）另对 `zagens-runtime-v1.openapi.json` 与 `runtime-api.ts` 做 `git diff` gate（D8 / D16 E5）

---

## 7. 新功能默认落点（冻结后）

| 需求类型 | 落点 |
|----------|------|
| HTTP / wire 类型 | `runtime-api` |
| Thread/Turn 生命周期 | `runtime-orchestrator` |
| MCP / persist / 网络策略 | `runtime-adapters` |
| Tool 实现 / LLM / prompts | `runtime-server`（宿主） |
| Engine / turn 逻辑 | `deepseek-core` + host trait |
| 桌面 UI / OS 集成 | `desktop` + `web-ui` |

---

## 8. 新人阅读顺序

1. [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
2. **本文（D17）** — 截止线与禁区  
3. [ARCHITECTURE_ASSESSMENT §7.1](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) — PR 红线  
4. [API_DESIGN.md](../API_DESIGN.md) + [OpenAPI JSON](../openapi/zagens-runtime-v1.openapi.json)  
5. [PERSISTENCE.md](../PERSISTENCE.md)  

**不必再读：** D6 实施细节（`doc_Private/`）、D16 E1 阶段 2 计划、SESSION_HANDOFF_D16（已归档至 `doc_Private/`）。

---

## 9. 完成定义（D17 DoD）

| # | 验收项 | 状态 |
|---|--------|------|
| 1 | 本文 Accepted | ✅ |
| 2 | D16 标记 Closed (Checkpoint) | ✅ |
| 3 | RUNTIME_ARCHITECTURE §10 更新 | ✅ |
| 4 | CHANGELOG 记录 Freeze v1 | ✅ |
| 5 | Assessment §0 指向 D17 | ✅ |
| 6 | F1/F2 可选 PR | ✅ Landed（2026-05-27） |

**宣告语（对内/对外）：**

> Zagens 系统架构 **Architecture Freeze v1**（2026-05-27）已生效。Assessment 10/10；Sidecar + OpenAPI 为稳定 ABI；重构主线关闭；产品功能可正常迭代。

---

## 10. 相关文档

| 文档 | 关系 |
|------|------|
| [D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md) | legacy 清除（已 Landed） |
| [D16_PHASE_E_MAINTAINABILITY.md](./D16_PHASE_E_MAINTAINABILITY.md) | 维护性拆分（Closed Checkpoint） |
| [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) | 10/10 判定 + §7.1 红线 |
| SESSION_HANDOFF_D16_PHASE_E | 归档于 `doc_Private/docs/tech/adr/` |

---

*维护：若 Architecture Freeze v2 需启动（如 Harness 独立、Desktop 内嵌 runtime），须新 ADR 显式解冻并修订 §4。*
