# Anthropic Harness 与 Claude Managed Agents（行业对照）

> **状态：** 备忘（2026-05-27）  
> **用途：** 记录 Anthropic 官方 Harness 叙事与托管产品动态，并与本仓库 [`Agent+Harness组合式编程方案.md`](./Agent+Harness组合式编程方案.md)、[`HARNESS_INTEGRATION_PROPOSAL.md`](./HARNESS_INTEGRATION_PROPOSAL.md) 对照。  
> **非官方摘要：** 以下链接与日期来自公开报道与 Anthropic Engineering 文章；产品细节以 [Anthropic 文档](https://docs.anthropic.com/) 为准。

---

## 1. 时间线（2026）

| 时间 | 事件 | 来源类型 |
|------|------|----------|
| **2026-04-08 前后** | **Claude Managed Agents** 公测（public beta）：托管 Agent 运行时、沙箱、长会话、多 Agent（部分为 research preview） | 产品报道、开发者指南 |
| **2026-04 起** | API 需 `managed-agents-2026-04-01` beta header；核心概念 **agents / environments / sessions / events** | 官方文档、技术博客 |
| **2026-05（Code with Claude）** | 在 Managed Agents 上追加 **Dreaming**（跨会话记忆整理，research preview）、**Outcomes**（成功标准）、**多 Agent 编排** 更广可用 | [Ars Technica](https://arstechnica.com/ai/2026/05/anthropics-claude-can-now-dream-sort-of/)、[MindStudio 报道](https://www.mindstudio.ai/blog/anthropic-dev-day-managed-agent-features-dreaming-outcomes/) |
| **持续** | Engineering Blog 系列：**长程 Harness**、**brain vs hands 解耦**、**eval harness vs agent harness** | 见 §2 官方文章 |

**商业信号：** 除模型 token 外，Managed Agents 按 **会话活跃时间** 计费（媒体报道约 **$0.08/运行时小时** + 常规模型费用），定位接近「云主机跑 Agent」而非纯 API。

---

## 2. Anthropic 官方 Harness 论述（推荐阅读顺序）

| 主题 | 文章 | 要点 |
|------|------|------|
| 托管产品与 meta-harness | [Scaling Managed Agents: Decoupling the brain from the hands](https://www.anthropic.com/engineering/managed-agents) | **Session / Harness / Sandbox** 分离；Harness 编码的「模型做不到」假设会随模型过时；Managed Agents 是 **可换 controller** 的 meta-harness（Claude Code 等均可跑在上面） |
| 长程多上下文 | [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents) | **Initializer agent** + **Coding agent**；`init.sh`、`claude-progress.txt`、git 交接；增量进度 + 结构化 artifact |
| 长程应用开发 | [Harness design for long-running application development](https://www.anthropic.com/engineering/harness-design-long-running-apps) | Planner / Generator / Evaluator；sprint contract；Playwright 式验收 |
| 并行 Agent 团队 | [Building a C compiler with a team of parallel Claudes](https://www.anthropic.com/engineering/building-c-compiler) | 测试 oracle、并行 merge、Harness 环境设计决定能否无人值守推进 |
| 评测 vs 运行时 | [Demystifying evals for AI agents](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents) | **Evaluation harness**（跑任务、打分）≠ **Agent harness**（工具编排、执行）；评的是 **harness + model** 一体 |

**术语（Anthropic 口径）：**

- **Agent harness / scaffold**：让模型能「行动」的系统——处理输入、编排工具、返回结果（Claude Code 是灵活 harness 之一）。
- **Managed Agents**：在 Anthropic 基础设施上运行的 **预构建、可配置** harness，面向数分钟到数小时的长程任务。

---

## 3. Claude Managed Agents 产品能力（公测口径）

媒体报道与教程归纳的四块能力（与中文科技稿「重磅 Harness」叙事一致）：

1. **生产级 Agent**：沙箱执行、身份验证、工具执行、检查点由平台承担。  
2. **长运行会话**：断连后状态与输出持久化，可恢复。  
3. **多 Agent 协调**：主 Agent 派生子 Agent 并行子任务再汇总（编排能力在 beta / preview 阶段演进）。  
4. **可信治理**：作用域权限、身份、执行追踪内置。

**2026-05 增量（Managed Agents 平台）：**

- **Dreaming**：在会话间隙整理事件流，提炼高信号记忆（团队模式、反复错误、工作流偏好）；可选自动或人工审核记忆变更（research preview，非全量开放）。  
- **Outcomes**：为任务定义可验证的成功标准。  
- **多 Agent orchestration**：从 research preview 走向更广可用。

**架构原则（工程文）：** 「大脑」（推理/编排）与「手」（沙箱执行）解耦；事件可在进入模型上下文前由 harness 转换；便于独立演进 session 日志、执行环境与安全边界。

---

## 4. 三个 Harness 设计模式（产品/engineering 共识）

中文报道与 Anthropic 工程叙述常归纳的三条（与 LangChain **Agent = Model + Harness** 公式同源）：

### 4.1 使用 Claude 已精通的通用工具

- SWE-bench 等场景强调 **bash + 文本编辑器** 为基座；Skills、程序化工具调用、内存工具多叠在此之上。  
- **哲学：** 少造任务专用工具，多给通用杠杆，让模型组合模式。

### 4.2 让 Claude 自主编排（代码即编排）

- 传统：每个工具结果都进上下文 → 慢、贵。  
- 用 **代码执行**（bash 脚本等）串联工具，仅最终输出进窗口；BrowseComp 等基准上 reported 显著提升（如 Opus 4.6 + 自过滤工具输出）。  
- **Skills：** YAML 前言进上下文，正文按需 `read` 展开，避免 system prompt 膨胀。

### 4.3 在边界处使用专用工具

- bash 只提交 **命令字符串**；专用工具提供 **类型化参数** → 可拦截、审批、审计、UI 模态框。  
- 不可逆操作（外部 API、写盘）适合确认；写工具可做 read 后过期检查。  
- 边界应随模型能力 **重评**（例如「第二 Claude 审 bash 安全性」可减少专用工具数量）。

---

## 5. 与 Zagens「组合式 Harness」对照

| 维度 | Anthropic Managed Agents | 本仓库组合式方案 + 归并提案 |
|------|--------------------------|-----------------------------|
| **公式** | Model + Harness（托管） | Model + Harness（本地 runtime + 桌面壳） |
| **Harness 形态** | 云 meta-harness，可换 controller | **按任务类型装配** policy/审查模块（§3.2–3.3）；装配器 → D13 Capability Manifest |
| **记忆/状态** | 平台 session + Dreaming 整理 | 黑板三身份、scratchpad、compaction/seam、thread 事件（§3 映射表） |
| **长程** | 托管持久化 + 检查点 | Context Reset、cycle、目标重注入、行为监控（部分已存在，部分 derived view） |
| **编排** | 强调 bash/代码管道、少过上下文 | 可吸收为 **工具层策略**；不推翻「审查管线 + 专用边界工具」 |
| **进化** | 靠模型与 harness 假设重评 | 方案中的 **进化引擎** → 提案延期 v1.0+ |
| **商业化** | 按活跃小时 + token | 桌面/侧载 sidecar，用户自有 API Key 与数据驻留 |

**结论（维护者备忘）：**

- Anthropic 证明 **Harness 可产品化、可计费**；本仓库证明 **Harness 可开源参考实现 + 可验收长程任务**（audit scratchpad、事件回放等）。  
- **不应对齐为「上云替代本地」**；应对齐的是 **问题定义**（长程、信任、边界）与 **可插拔模块** 思路。  
- 详见 [`HARNESS_INTEGRATION_PROPOSAL.md`](./HARNESS_INTEGRATION_PROPOSAL.md) §3 名词映射与 Phase 路线。

**本仓库下一形态预测（v1.3）：** 组合式（阶段五）之后为 **自适应主动 Harness**（阶段六）——Harness 据运行时证据调节 Capability Manifest 并主动干预，人只签策略包；与 Anthropic「假设重评」/ Dreaming 同向，但强调本地可审计 manifest。见 [`Agent+Harness组合式编程方案.md`](./Agent+Harness组合式编程方案.md) §3.4。

---

## 6. 竞品与平行动向（简表）

| 厂商 | 动向 | 备注 |
|------|------|------|
| **Anthropic** | Managed Agents + Harness 工程文 | 本文 §1–§4 |
| **OpenAI** | Agents SDK（2026-04 报道）：sandbox、manifest、MCP/skills/`AGENTS.md` | 同向「托管运行时 + 本地 harness 并存」 |
| **LangChain 等** | Agent = Model + Harness 公式传播 | 生态话术，非单一产品 |

---

## 7. 本仓库应跟踪的落地项（非 Anthropic 功能抄送）

从行业动态反推 **Zagens / runtime** 优先级（已写入归并提案，此处摘要）：

1. **工具结果是否必经 turn 上下文** — 评估 shell/脚本编排，对齐 §4.2。  
2. **Skills 渐进展开** — 对照 `load_skill` 与 prompt-architecture 中的 catalog 注入。  
3. **Harness 可观测** — thread 事件回放、`runtime_thread_id`、HarnessPanel（Phase 2）。  
4. **组合式装配器** — D13，勿在冻结期新开 Engine 字段轨。

---

## 8. 参考链接

**Anthropic Engineering**

- https://www.anthropic.com/engineering/managed-agents  
- https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents  
- https://www.anthropic.com/engineering/harness-design-long-running-apps  
- https://www.anthropic.com/engineering/building-c-compiler  
- https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents  

**媒体报道（2026-05，Managed Agents / Dreaming）**

- https://arstechnica.com/ai/2026/05/anthropics-claude-can-now-dream-sort-of/  
- https://www.mindstudio.ai/blog/anthropic-dev-day-managed-agent-features-dreaming-outcomes/  
- https://blakecrosby.com/blog/managed-agents-vs-local-ai-agent-harnesses  

**本仓库**

- [`README.md`](./README.md) — 文档集索引  
- [`Agent+Harness组合式编程方案.md`](./Agent+Harness组合式编程方案.md)  
- [`HARNESS_INTEGRATION_PROPOSAL.md`](./HARNESS_INTEGRATION_PROPOSAL.md)  
- [`../desktop/HARNESS.md`](../desktop/HARNESS.md) — Zagens 栈位与 JD 映射  

---

> **维护：** Anthropic 发布重大 Harness/Managed Agents 变更时，更新 §1 时间线与 §4–§5；若与本方案 §3 装配规则冲突，先改归并提案映射表，再改远景方案正文。
