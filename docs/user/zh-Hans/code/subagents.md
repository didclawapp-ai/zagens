# 子代理面板

**子代理**是主 Agent 通过 `agent_spawn` / `delegate_to_agent` 派生的子任务 worker。CRAFT 流水线中的 Explore、Implementer、Review 等均在此查看实时状态；面板底部还可摘要 **CRAFT 黑板任务**。

> 仅 **代码模式** 显示。办公模式无子代理侧栏入口。

---

## 打开方式

| 入口 | 说明 |
|------|------|
| 侧栏 → **子代理** | 全高面板，适合长时间跟踪 |
| **审计网格** → 右下象限 | 与清单、Scratchpad、LHT 同屏（[界面导览](/zh-Hans/docs/ui-tour)） |

标题栏 **显示审计网格** 在已有 harness 数据时出现（代码会话）。

---

## 面板布局

### 顶部统计（三列）

| 列 | 计数对象 |
|----|----------|
| **运行中** | `status = spawned \| running` |
| **已完成** | `completed` |
| **已中断** | `interrupted`（含失败/取消等终端态） |

仅显示 id 形如 **`agent_*`** 的行（过滤父级 `call_*` 工具 id）。

### 子代理卡片

每张卡可点击展开：

| 元素 | 含义 |
|------|------|
| 状态圆点 | 运行中脉冲琥珀 / 完成绿 / 中断红 |
| **标题** | `nickname` 或类型名 + 可选类型徽章 |
| **agent_id** |  monospace 完整 id |
| **目标摘要** | spawn prompt / objective 前 200 字 |
| **progressStatus** | 运行中时的最新进度行（SSE） |
| **步数 done/max** | 相对 `max_steps`（默认上限 100） |
| **单步上限 Ns** | `step_timeout_ms` 换算秒数 |
| **长时间无进展** | `stuck_suspected` — 超过 step 超时 + 缓冲仍无 heartbeat |
| **工作包 task_id** | 关联 CRAFT 黑板 / Scratchpad run |
| 底部 | 工具调用数 · token 估算 · 耗时 |
| 展开区 | 完整 objective、工具调用列表、`resultSummary` |

状态通过 runtime **SSE** 与轮询刷新，无需手动刷新。

---

## 代理类型与工具面

spawn 时 `type` / `agent_type` 字段（详见 [CRAFT](/zh-Hans/docs/code/craft)）：

| 类型 | 别名示例 | 工具裁剪 |
|------|----------|----------|
| **explore** | exploration, explorer | **只读**：list/read/grep/glob/file_search/web/note |
| **review** | reviewer, code_review | **只读 + exec_shell**（跑验证命令，不改文件） |
| **implementer** | implement, builder | **继承主 Agent** 全部工具 |
| **verifier** | verify, tester | **继承主 Agent** 全部工具 |
| **auditor** | audit, fact_checker | 审计专用 prompt + 工具 |
| **plan** | planning | 规划向 |
| **general** | default, worker | **继承主 Agent** 全部工具 |
| **custom** | — | **必须**显式传 `allowed_tools` 列表 |

Review 的 `exec_shell` 用于 `cargo check`、`npm test` 等**验证**，Implementer 改代码前 runtime 可创建 **git stash** 快照（非致命）以便回滚。

---

## CRAFT 任务区

当工作区存在 `.zagens/blackboards/*.json` 时，面板底部显示 **「CRAFT 任务」** 列表（按 `task_id` 排序）：

| 字段 | 来源 |
|------|------|
| **task_id** | 黑板文件名 |
| **探索：是/—** | 是否已有 `explorer` 分区 |
| **实现轮次** | `implementer.rounds` 数组长度 |
| **审查** | `reviewer.verdict` — PASS / BLOCKER / MAJOR / FAIL |
| **验收** | `verifier.summary` 摘要（若有） |

用于快速判断 fix-loop 停在哪一步，无需打开 JSON 文件。

---

## 设置

**设置 → 系统 → 安全**：

| 设置 | 范围 | 默认 |
|------|------|------|
| **Sub-agents enabled** | 总开关 | 开 |
| **Max sub-agents** | 1–20 | 见 config |
| **Step timeout** | 120–1800 秒 | **600s** |

spawn 参数 **`step_timeout_ms`** 可覆盖单次任务。全库 Explore 建议 10–30 分钟量级。

严格环境：`config.toml` 中 `[features] subagents = false`。

见 [执行策略](/zh-Hans/docs/settings/exec-policy)。

---

## 与 Scratchpad / LHT 联动

- Scratchpad **审计**象限显示 **「子代理 N 活跃」** — 与面板运行中计数一致。
- **口述 spawn** 警告：聊天描述 spawn 但面板无 `agent_*` 时出现。
- LHT **宏观循环** spawn 的 CRAFT Review 同样出现在本面板与 CRAFT 任务区。

---

## 使用建议

1. 同一 **task_id** 下按 **Explore → Implementer → Review → Verifier** 顺序 spawn（见 [CRAFT](/zh-Hans/docs/code/craft)）。
2. 看到 **长时间无进展** 时，在对话中要求父 Agent cancel 或缩小 scope。
3. 审查回合未完成前，不要在同一宏观周期追加新功能需求。
4. 并发 Explore 仅限只读；Implementer 并发写同一文件由 runtime 所有权规则约束。

---

相关：[CRAFT 多代理](/zh-Hans/docs/code/craft) · [审计 Scratchpad](/zh-Hans/docs/code/audit-scratchpad) · [LHT 设置](/zh-Hans/docs/settings/lht)
