# Harness 定位与 DS Pick 映射

> **状态：** 设计备忘（2026-05-20）  
> **来源：** DeepSeek 社招「Agent Harness」产品经理 / 研发工程师 JD（2026-05-15、2026-05-18，公开信息）；与本仓库试跑、会话恢复修复、audit scratchpad 讨论对齐。  
> **相关：** [audit-scratchpad-design.md](audit-scratchpad-design.md) §2（契约、产品本质）、[DEV_NOTES.md](DEV_NOTES.md)、[TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md)、[../tech/API_DESIGN.md](../tech/API_DESIGN.md)。

---

## 1. 外部信号：他们在招什么

DeepSeek Harness 团队公开表述的核心公式：

**Model + Harness = Agent**

| 概念 | 含义（JD 口径） |
|------|----------------|
| **Model** | 前沿模型能力（推理、工具选择、长上下文） |
| **Harness** | 模型之外、把能力变成**可用 Agent 产品**的一切：循环、工具、记忆、子代理、UI、指标、与训练团队的共进化 |
| **产品载体** | **DeepSeek 桌面端 Agent**（与 Claude Code、Cursor、Codex 等工作台型产品同赛道） |

JD 中反复出现、且与实现强相关的 Harness 模块：

| 模块 | 典型职责 |
|------|----------|
| **Agent Loop** | 多轮：用户输入 → 推理 → 工具 → 观察 → 再推理 |
| **Tool Use** | 工具 schema、执行、审批、输出回灌 |
| **Reasoning** | 可展示的推理链（与最终答案解耦） |
| **Planning** | 任务分解、清单、长程进度 |
| **Skills** | 可加载的领域规程（如 `audit-repo`） |
| **MCP** | 外部工具/数据源协议 |
| **Memory** | 跨轮外存（scratchpad、会话、blackboard） |
| **Subagent / Multi-Agent** | 派生子代理、并行、结果 join |

三门工程（JD 用语）：

| 学科 | 在本仓库中的落点 |
|------|------------------|
| **Prompt Engineering** | `base.md`、mode 栈、工具 `description`、skills |
| **Context Engineering** | turn 压缩、scratchpad 注入、`<scratchpad_summary>`、附件 |
| **Harness Engineering** | runtime thread、事件持久化、SSE 契约、UI 回放、门禁（C1/E2） |

---

## 2. 本仓库在 Harness 栈中的位置

```
┌─────────────────────────────────────────────────────────┐
│  DS Pick (Tauri + web-ui)          ← 桌面壳、Composer、│
│  思维链/工具卡片、scratchpad 横条、子代理面板            │
├─────────────────────────────────────────────────────────┤
│  runtime_api (HTTP/SSE)            ← Agent Loop 对外面 │
├─────────────────────────────────────────────────────────┤
│  runtime_threads + engine          ← turn/item/event、  │
│  工具执行、子代理、scratchpad store                     │
├─────────────────────────────────────────────────────────┤
│  deepseek CLI / TUI                ← 同 Harness，另一 UI │
└─────────────────────────────────────────────────────────┘
                          ▲
                          │ API
                          ▼
                    DeepSeek Model
```

**结论：** 本 monorepo 的主体不是「再训一个模型」，而是 **Harness 参考实现 + 桌面产品（DS Pick）**。与 JD 中「桌面端 Agent 全链路研发」高度同构。

---

## 3. JD 关键词 → 代码与文档索引

| JD 关键词 | DS Pick / runtime 落点 | 说明 |
|-----------|------------------------|------|
| Agent Loop | `POST /v1/stream`、`stream_turn`、`monitor_turn` | 单轮从 `turn.started` 到 `turn.completed` |
| Tool Use | `crates/tui/src/tools/*`、Web **工具调用** 卡片 | `item.started` / `item.completed`（`tool_call`） |
| Reasoning | `item.delta`（`kind: thinking`）、Reasoning UI 块 | 无 `item.completed` 收口；靠事件流累积 |
| Planning | audit scratchpad、`checklist_*`、`task_*` | 全库审计见 [audit-scratchpad-design.md](audit-scratchpad-design.md) |
| Skills | `crates/tui/assets/skills/`、`load_skill` | 如 `audit-repo` |
| MCP | 桌面 MCP 集成路径 | 与 TUI 共用配置面 |
| Memory | scratchpad 文件、blackboard、`SessionManager`、thread 事件库 | reasoning ≠ 可靠工作记忆（设计 §1.2） |
| Subagent | `agent_spawn` / `agent_result` / `agent_list` | 与 **Task**（`task_create`）区分见 design §7.1、§14 |
| 指标 / 可验收 | scratchpad inventory、C1 覆盖率、横条 | 「Agent 是否真帮人」的工程代理指标 |
| Phase D 可视化 | Inventory 面板、违约高亮、双轨进度（规划） | 见 [audit-scratchpad-design.md §6.13](audit-scratchpad-design.md#613-phase-d--审计过程可视化路线图-未实现)、[test §L8](audit-scratchpad-test.md#l8--phase-d-审计过程可视化规划) |
| E5 工具挡位 | `task_create` defer+block；`agent_spawn` eager（有 `scratchpad_run_id`） | 避免 L7b「嘴上说 sub-agent、脚用 Task」 |

---

## 4. 案例：重启后会话「只剩终稿」

**现象：** 同一会话在运行中可见 **Reasoning** + **write_office** 等工具块；关闭 DS Pick 再打开后只剩 assistant 正文（如 PPT 摘要表）。

**Harness 根因（非模型问题）：**

| 层 | 问题 |
|----|------|
| 会话 JSON | `export_thread_for_session_persist` 仅导出 text/thinking 块，**不含**工具卡片 |
| 恢复路径 | `resume-thread` 曾每次 **新建空 thread** 再 seed 文本；对空 thread 做 `replay_only` → 无工具/思维链事件 |
| UI 缓存 | 内存 `sessionUiCache` 在进程退出后清空 |

**修复方向（已落地或进行中）：**

1. 会话元数据 **`runtime_thread_id`**：`persist-session` 写入；`resume-thread` **复用**仍有事件的 thread。  
2. Web UI：**localStorage** 镜像工具/思维链快照（兜底）。  
3. 权威来源仍是 **thread 事件回放**（`rebuildMessagesFromThreadEvents`）。

这是典型的 **Harness Engineering** 问题：模型已正确调用工具，但 **可观测状态未与产品语义绑定**。

---

## 5. 与 audit scratchpad「契约」的关系

[audit-scratchpad-design.md §2.1](audit-scratchpad-design.md#21-人机契约契约现象) 用 **Model + Harness 契约** 描述三方对齐：

| 角色 | 比拟 |
|------|------|
| 模型 | 驾驶员（叙事、规划） |
| Harness | 车辆 + 交规（工具语义、门禁、持久化） |
| 用户 | 车主（目标与验收） |

JD 的 **Model + Harness = Agent** 与 §2.1 **同构**：产品价值在 **Harness 可执行、可违约发现**，不在模型独白是否流畅。

L7b 反例（Task 与 Sub-agent 混用、未 join）属于 **Harness 语义不清 + 无机械验收**；会话恢复丢失 UI 属于 **Harness 持久化断裂**。二者都应通过 **引擎与存储** 修复，而非仅改 prompt。

---

## 6. 路线图对照（非官方）

| JD / 行业方向 | 本仓库状态 | 参考 |
|---------------|------------|------|
| 桌面 Agent 全链路 | DS Pick v0.3.x；TUI 仍更完整 | [TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md) |
| 长程任务 + 可恢复 | thread + session SQLite + scratchpad | [audit-scratchpad-design.md](audit-scratchpad-design.md)、[DEV_NOTES.md](DEV_NOTES.md) |
| Sub-agent 产品化 | `agent_*` + 面板；与 Task 边界在收紧 | design §14、[TOOLS_PRINCIPLES.md](../tech/TOOLS_PRINCIPLES.md) §3.7.1 |
| Harness 指标 | inventory / verified / 横条（审计场景） | [audit-scratchpad-test.md](audit-scratchpad-test.md) |
| 与模型共进化 | 试跑反馈 → prompt/tool/skill（社区侧） | 无官方训练管道 |

---

## 7. 与 DeepSeek 的关系（「卖给 DeepSeek」？）

> **免责声明：** 本仓库 README 标明为 **非官方社区项目**，与 DeepSeek Inc. 无隶属关系。本节仅为维护者战略备忘，不构成商业建议。

### 7.1 对齐度

| 维度 | 说明 |
|------|------|
| 方向 | JD 明确招 **桌面端 Agent Harness**；本仓库 **已是** Harness 形态（runtime + DS Pick + TUI/CLI） |
| 资产 | 可复用：thread 事件模型、工具栈、scratchpad/子代理设计、桌面壳、试跑方法论 |
| 差距 | 官方品牌、模型侧深度适配、规模测试、合规与发布渠道；社区 fork 通常不自带这些 |

### 7.2 「出售」的现实路径（由易到难）

| 路径 | 可行性 | 备注 |
|------|--------|------|
| **开源协作 / PR** | 高 | 按对方开源策略贡献 runtime 或文档；无「卖」、有影响力 |
| **招聘 / 加盟 Harness 团队** | 中 | JD 即信号；带本仓库作品与 L7/L7b 复盘面试 |
| **商业授权 / 白标** | 低～中 | 需清晰 IP（许可证）、维护承诺；fork 许可证须与对方政策一致 |
| **整体收购** | 低 | 通常收购团队 + 产品牵引 + 合规；单仓库 unless 有显著 DAU/收入 较难 |

### 7.3 若希望「被看见」而非「被收购」

更务实的 Harness 叙事：

1. **公开可复现的长程审计试跑**（[audit-scratchpad-test.md](audit-scratchpad-test.md)）——证明 Harness 可验收。  
2. **桌面会话可观测性**（思维链 + 工具 + 事件回放）——与 JD 中 Reasoning / Tool Use 产品要求一致。  
3. **文档化 Task vs Sub-agent、契约、三门工程**——降低官方团队重复踩坑成本。

「卖给 DeepSeek」在多数情况下应理解为：**把 Harness 能力讲清楚，并找到官方产品或团队的对接面**；本仓库更适合作为 **参考实现 / 人才作品集 / 上游讨论基础**，而非待价而沽的闭源 SKU。

### 7.4 许可证提醒

对外洽谈前须核对根目录 **LICENSE** 与依赖协议；任何「合并进官方产品线」都涉及版权与专利尽职调查，需法务参与。

---

## 8. 维护

| 变更类型 | 动作 |
|----------|------|
| 新增 Harness 模块（如统一 Memory API） | 更新 §3 映射表 |
| 官方 JD / 产品表述变化 | 更新 §1 并注明日期 |
| 会话/事件持久化行为变更 | 更新 §4 |
| 战略讨论结论 | 更新 §7，保持与 README「非官方」声明一致 |

**Changelog：** 见根目录 [CHANGELOG.md](../../CHANGELOG.md) `[Unreleased]` → Docs。
