# Zagens 开发笔记

零散想法、后续方向与非正式排期；需要落地时再拆 issue / 写入 IMPLEMENTATION_STEPS。

**图例：** ✅ 已落地 · 🔶 部分 / 雏形 · ⬜ 规划中（未做或未产品化）

**Harness 总览：** [HARNESS.md](HARNESS.md) — DeepSeek 社招 JD 映射、本仓库栈位、会话恢复案例、「与官方关系」备忘。

**版本 / 发布：** [VERSIONING.md](VERSIONING.md) — Zagens 独立 SemVer；当前对外版本 **`0.7.0`**（历史标签可能带 `-preview.n`）。

**本地 Lint / 工具链：** [LOCAL_DEV_VERIFY.md](../LOCAL_DEV_VERIFY.md) — Rust 1.96 钉死、`verify-lint`、git hooks 与 CI 对应关系。

---

## 2026-05-30 — 抗幻觉工程哲学：Harness 是「把人类工程方法翻译给模型」（设计对话整理）

**背景：** 维护者复盘 Zagens 的两个底层决策——**为什么押 DeepSeek V4 单模型** + **为什么 harness（CRAFT / LHT / verify）长成现在这样**。核心矛盾：MVP 阶段最「破防」的不是 V4 能力上限，而是它的**幻觉率**；死磕压制幻觉的过程，事后看正是在重新发现软件工程方法论。本节整理结论，供路线图与设计评审引用；**非立即工程承诺**。

**一句话结论：** 不要等更聪明的模型，**先把工程学补给现在的模型**——harness 的本质不是「教模型别幻觉」（prompt 写「不要幻觉」无效），而是**一点点抽掉它幻觉的空间**。这套方法不会随模型迭代过期。

### 1. 单模型（V4）路线 — 取舍签收

| 理由 | 结论 | 备注 |
|------|------|------|
| V4 能力够用 | **成立** | agent 场景关键不是榜单排名，是「在 harness 约束下行为是否稳定」。中上但可预测 + 强 harness > 更聪明但调不动 |
| 调用量前三（生态确定性） | **成立** | 价值在 API 稳定性 / 长期可用 / 社区踩坑经验，而非「流行」。对独立开发者是降长尾运维风险 |
| 多模型 prompt 工程发散 | **成立（最硬）** | 多模型适配会被迫退化到所有模型的**公约数**，谁都不是最优。单模型可对 V4 thinking 行为做极致耦合调优（如 `max_tokens` 把 reasoning+answer 合并计费的坑） |
| 「未来 agent 都是单一模型最优」 | **部分成立 — 需拆一层** | 对的是「单模型深度适配 > 多模型浅适配」；要警惕的是「绑死某个具体版本」。**把模型耦合点收进一个适配层**（prompt 措辞 + provider 行为细节），harness 控制逻辑保持模型无关 → 享受极致调优，又能换 V5 不重写 |

**设计戒律：** 对外承诺「为 V4 优化」，对内保留「换模型不重写」的弹性——耦合点关进一个房间（runtime / prompt 分离现状已接近此形态）。

### 2. 抗幻觉四件事（设计骨架）

agent 的幻觉 ≠「答错」，而是**「污染状态」**：模型幻觉一个文件路径 / 函数签名后会**基于错误事实继续规划、改代码、勾清单**，错误被后续几十步放大成废墟，且全程语气笃定。所以抗幻觉要在它「还是计划里一句话」时抓，而不是「已写进 20 个文件」后。

| # | 手段 | 含义 | 落地 |
|---|------|------|------|
| 1 | **工具够好用（不给逃逸动机）** | 工具不好用→模型自己造工具 / 写脚本，其输出 harness 管不到（不可观测、不可验证、不可回灌）→ 幻觉从这里漏进来。模型绕过工具是**工具链 ROI 体检信号**：治本是把工具做好，不是 prompt 禁止 | 🔶 `write_file`/`grep_files`/`apply_patch` 编码安全 + 原子写加固持续进行 |
| 2 | **输入端逼 grounding** | 幻觉的上游常是「没看全就推断」。逼模型先交代读了什么 | ✅ Explorer `## Coverage Report`；read/grep 优先于作答 |
| 3 | **输出端逼证据（证据义务）** | 结论必须落到硬事实，不许「看起来没问题」 | ✅ `[verify:]` 闸门 + false-green 守卫；CRAFT Reviewer 跑编译（C1） |
| 4 | **终审交给不会幻觉的裁判** | 最好的 judge 是编译器 / 测试 / grep 真实命中 / 文件是否真存在——它们永不幻觉 | 🔶 C1 让编译器当终审已做；S3「程序化校验你真读了吗」⬜ |

### 3. CRAFT 的出处：人类工程项目实施流程

CRAFT（Explorer / Implementer / Reviewer + 黑板）不是发明，是**重新发现一门被验证上百年的学问**。工程学（土木 / 航空 / 外科 / 制造）的内核从来不是「让个体不犯错」，而是**「假设每个执行者都会犯错，系统照样交付可靠结果」**。这给了一个现成的**指南针**：harness 该长什么样，直接问「人类工程靠什么机制防人犯错」，再映射过来。

| 人类工程机制 | Zagens harness | 落地 |
|------|------|------|
| code review | CRAFT Reviewer | ✅ |
| CI / 编译门禁 | `[verify:]` 闸门 | ✅ |
| 调研 / 勘察阶段 | Explorer + 覆盖率报告 | ✅ |
| 工程文档 / wiki | 黑板 blackboard | ✅ |
| 事后复盘 postmortem | cycle 简报 / rounds 历史 | 🔶 cycle ✅；rounds 见 craft-v2 C3 |
| **设计评审前置（design review）** | plan / 任务图阶段加「方案审」闸门（现 CRAFT 重事后审代码） | ⬜ 金矿①：错误设计实现得再漂亮也白干，趁它还是一句话时抓 |
| **可追溯矩阵（traceability）** | `目标 ↔ 实现 ↔ 验证` 三者强绑（现已绑「实现↔验证」，差锚回「目标」） | ⬜ 金矿②：堵死 DEMO3「验收项分解时悄悄变形」 |

### 4. 类比的边界（单模型最该上心）

人类工程大量机制**隐性依赖「人会为后果负责、卡住会主动求助、有长期记忆」**——模型这三样全没有。人类不确定会停下来问，模型不确定会**自信地幻觉一个答案继续走**。

**戒律：** 凡是人类工程靠「责任心 / 主动暴露不确定性」兜底的地方，**不能照搬，必须用硬约束替代那份责任心**。
- 已在做的翻译：Explorer `Confidence: low 就解释还需读什么` —— 强制模型做一件**人类会自然做、模型永不主动做的事：承认「我没把握」**。这是补「模型没有责任心」缺口最聪明的一笔。
- 单模型自审的**共谋盲区**：模型审自己输出时倾向确认而非证伪（和被审对象共享同一套幻觉）。解法**不是引第二个模型**（拖回多模型复杂度），而是让不会幻觉的工具当终审 → 故 **S3（程序化校验）优先级应高于 S1（Dual Judge 双模型）**。

### 相关文档

- 抗幻觉落点：[`../craft-v2-improvements.md`](../craft-v2-improvements.md)（C1-C3 / S1-S3）、[`../agent-reliability-craft-plan.md`](../agent-reliability-craft-plan.md)
- LHT verify 闸门 / false-green：[`../harness/LONG_HORIZON_CODE_TASKS.md`](../harness/LONG_HORIZON_CODE_TASKS.md)（§4.x、DEMO3 实证）
- 单模型 / 引擎分离方向：本文件 §2026-05-24 战略备忘、[`../tech/RUNTIME_EVOLUTION_ROADMAP.md`](../tech/RUNTIME_EVOLUTION_ROADMAP.md)

---

## 2026-05-24 — 产品战略方向备忘（架构对话整理）

**背景：** 维护者与 Agent 就 Zagens 长期方向进行战略对话——担心 Agent 领域认识尚浅、业界许多能力仍在摸索，**开发方向是否走偏**。本文档整理对话结论，供后续评审与路线图引用；**非立即执行的工程承诺**。

**一句话结论：** 方向判断 **大体正确**——Zagens 应押 **Desktop 壳 + 本地 sidecar Harness + 长程任务（CRAFT）**；TUI/CLI **已退出**（D6 Phase B 2026-05-26）；sidecar 三层架构 **不必推翻**。

### 1. 核心战略判断（签收备忘）

| 判断 | 结论 | 置信度 | 说明 |
|------|------|--------|------|
| Harness 终态需要富交互壳 | **成立** | 高 | 审批、CRAFT 黑板、diff、Harness 预置、记忆图——终端 ratatui 难以产品化 |
| **Desktop-only 作为用户产品** | **成立** | 高 | 与 Cursor / Claude Desktop / Windsurf 主战场一致；资源应停止 TUI parity |
| TUI/CLI 完全消失 | **已发生（D6 Phase B）** | 高 | 2026-05-26 删除 `crates/cli`、`crates/tui`；headless 直接用 **`deepseek-runtime`** HTTP |
| sidecar 壳运分离 | **继续** | 高 | Agent turn 只在 sidecar；Desktop 是 L3，不是引擎 |
| 本地可配置 Harness vs 云端托管 | **差异化成立** | 中-高 | Anthropic Managed Agents = 云端 harness；Zagens = 本地控制权（见 [§Harness 组件化](#2026-05-24--harness-组件化从硬编码到可组合-agent-执行结构)） |
| 长程任务可达 | **已有实证** | 高 | **CRAFT 手测 ~35 分钟**（2026-05-24）；B-L1 runtime + AgentPanel 已签收 |

**产品表述（对内）：** Zagens = **本地长程 Agent Harness 的 Desktop 控制台**——不是「聊天窗口」，而是能跑多角色 CRAFT、目标数小时、可跨天接力的任务运行时。

### 2. 架构评估摘要

当前 **L1/L2/L3** 分层（见 [RUNTIME_EVOLUTION_ROADMAP.md §2](../tech/RUNTIME_EVOLUTION_ROADMAP.md)）经对话复核 **仍然正确**：

```
L3  Zagens（Tauri + React）— 唯一用户产品壳
L2  HTTP/SSE + Tauri IPC + runtime_proxy（Bearer 不出 WebView）
L1  deepseek-runtime sidecar + deepseek-core（Engine / turn_loop / tools）
```

**不必做：** 合并 sidecar 进 Tauri、换 app-server 为生产 binary、在 WebView 内跑 Engine。

**值得加强：** P2 收尾（Engine struct 边界）、Harness 预置 MVP、长任务 Desktop UX、Handoff Report。

### 3. 业界对照（2025–2026，非对标）

业界在 Agent 形态上 **并未收敛**，但有几条可观察趋势——用于 **校准方向**，不是复制竞品：

| 趋势 | 代表 | 对 Zagens 的含义 |
|------|------|-------------------|
| CLI → Desktop 编排 | Anthropic：Claude Code 从 CLI 走向 Desktop 管多 agent | 长任务 / 多 agent **可视化编排** 在 Desktop，不在 TUI |
| IDE/Desktop 主战场 | Cursor、Windsurf、Claude Desktop Code tab | 日常 + agent 可视化 = IDE/Desktop；与「desktop-only 产品」同向 |
| CLI 仍占生态位 | Claude Code CLI、Codex CLI、headless SDK | **power user / CI / 自动化**；Zagens 不必抢，保留 dev 入口即可 |
| 云端 Managed Harness | Anthropic Managed Agents（2026.04 beta） | 卖「省心」；Zagens 卖 **本地控制权 + 可组合 Harness** |
| Harness 术语结晶 | session / harness / sandbox 全行业共用 | 本仓库 Harness 组件化路线与业界词汇对齐 |

**重要 nuance：** CLI 常被说「适合深度推理」——**不是因为终端更聪明**，而是 historically 绑定了 lean harness、少 UI 打断、长 autonomous run。**同一 sidecar + 同一模型在 Desktop 推理深度可等价**；Desktop 还多 steer、diff、AgentPanel。Zagens 应用 **trust 预置 + 批量审批** 在长任务上复现 CLI 的「少打断」，而非保留 ratatui 产品。

**仍不确定（业界也在摸索）：** 最优 Harness 工具集大小、多 agent 并行写冲突策略、跨天记忆 vs compaction 分工、Managed vs Local 长期份额——**保持 sidecar 可配、Desktop 可观测**，便于随业界迭代调参，而非押死单一路径。

### 4. TUI / CLI 终态（D6 Phase B ✅ 2026-05-26）

| 层级 | 终态 | 状态 |
|------|------|------|
| **用户产品 L3** | 仅 Zagens Desktop | ✅ |
| **Runtime sidecar** | **`deepseek-runtime`**（`crates/runtime-server`） | ✅ 生产 binary |
| **CLI / ratatui TUI** | 已删除 | ✅ `crates/cli`、`crates/tui` 不存在 |
| **Headless / CI** | `deepseek-runtime` + HTTP Bearer | ✅ 唯一 dev 入口 |

**已签收 ADR：**

| 决策 | 内容 | 状态 |
|------|------|------|
| **D12 Desktop-only 产品** | Zagens 为唯一用户产品壳 | ✅ |
| **D13 Sidecar 语义** | sidecar = **`deepseek-runtime`** HTTP runtime | ✅ Phase B |
| **D14 CLI 定位** | ~~CLI dev 工具~~ → **已移除** | ✅ Phase B |

### 5. 长程任务：35 分钟 CRAFT 与下一关

**实测：** CRAFT 已在 Zagens 上连续运行 **约 35 分钟**（2026-05-24），与 B-L1 手测签收一致。说明 **L1 墙钟 + L2 多角色交接** 已过关。

长程任务分三层（验收用，避免只盯墙钟）：

| 层 | 含义 | 状态 |
|----|------|------|
| **L1 墙钟** | 单次 session 连续跑多久不崩 | **✅ ~35 min 实证** |
| **L2 认知** | CRAFT 黑板、fix-loop、子代理闭环 | **✅ B-L1 已签收** |
| **L3 产品** | 用户能盯、steer、恢复、跨天续 | **🟡 主缺口** |

**L3 优先 backlog（与 [§Handoff](#2026-05-21--会话线程结项汇总报告handoff-report--规划中)、[§主动性北极星](#2026-05-18--agent-方向与主动性北极星) 对齐）：**

| 优先级 | 项 | 说明 |
|--------|-----|------|
| **P0** | Sidecar 长跑友好化 | busy-timeout 宽容、SSE 断线重连；见 [SIDECAR_SUPERVISOR_HARDENING_PLAN.md](SIDECAR_SUPERVISOR_HARDENING_PLAN.md) |
| **P1** | Harness 预置「长任务」 | 如 `craft-audit`：trust + 按 audit-repo 分档 `step_timeout_ms` + 显式长 `agent_wait`；避免模型踩默认短 timeout |
| **P1** | AgentPanel 长任务 UX | 运行时长、blocked 原因、子代理树；最小化后分级通知 |
| **P2** | Handoff Report MVP | 手动「生成本轮摘要」→ `~/.deepseek/handoffs/`；compaction 服务模型，handoff 服务人与跨天 |
| **P2** | 35 min 基准化 | 纳入 `runtime-longrun-baseline` / 回归门，从「一次手测」变产品指标 |
| **P3** | Resume 性能 | 超长 thread 冷启动（350+ 消息 fsync 问题） |

**与 §2026-05-18「长程任务」轴线的关系：** 该节表中「行业级长程产品化 ⬜」仍成立；**35 min CRAFT** 将「持久线程 + CRAFT + 桌面 persist」从 🔶 推向 **可演示、可回归** 的产品能力，下一步是 **L3 产品化 + Handoff**，而非回到 TUI。

### 6. 方向「对」与「仍须小心」的清单

**继续投入（方向正确）：**

- Desktop 壳 + sidecar runtime（不改）
- CRAFT / 子代理拓扑 / 黑板（长任务核心）
- Harness 组件化 + 预置组合（产品化控制权）
- execpolicy、审批 UI、工作区预览、embedded PTY
- Handoff Report、入座 briefing（长任务 + 主动性）

**停止或降优先级：**

- TUI ↔ Desktop 功能 parity 追平
- 把 ratatui 当第二产品壳迭代
- 与 Claude Code CLI 抢 terminal power user 市场（除非明确扩品类）
- 换 app-server 或合并 Engine 进 WebView

**保持观望、小步验证：**

- 记忆地图 L3 可视化（B2）
- Managed Agents 类云端 harness 是否侵蚀本地市场
- Zagens 对外命名（见 [§Harness 组件化 · 命名决定](#2026-05-24--harness-组件化从硬编码到可组合-agent-执行结构)）——最终确定为 Zagens（拉丁文 agens = 行动者）

### 7. 建议演进顺序（战略层，覆盖原 §2026-05-18 部分排期）

在 P2 ✅、D10 解冻、B-L1 CRAFT ✅ 前提下：

1. **签收 D12–D14**（或等价表述写入 [RUNTIME_EVOLUTION_ROADMAP.md §4](../tech/RUNTIME_EVOLUTION_ROADMAP.md)）— 冻结 TUI 产品化
2. **P0 sidecar 长跑** + **P1 长任务 Harness 预置** — 保住 35 min+ 稳定性
3. **AgentPanel / 长任务 UX** — Desktop-only 差异化
4. **Handoff P1 手动结项** — 跨天长程
5. **Harness 设置页预置组合** — 组件化第一步（见 Harness §7）
6. TUI ratatui **freeze**；CLI 缩面 — 无用户可见时间表

### 相关文档

| 文档 | 关系 |
|------|------|
| [RUNTIME_EVOLUTION_ROADMAP.md](../tech/RUNTIME_EVOLUTION_ROADMAP.md) | L1/L2/L3、门控链；待补 D12–D14 |
| [IMPLEMENTATION_SUMMARY_2026-05-24.md](../tech/adr/IMPLEMENTATION_SUMMARY_2026-05-24.md) | B-L1 CRAFT 签收、P2 状态 |
| [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md) | CRAFT 运行时与调度事实 |
| [API_DESIGN.md](../tech/API_DESIGN.md) | 双通道 API、sidecar 契约 |
| [SIDECAR_SUPERVISOR_HARDENING_PLAN.md](SIDECAR_SUPERVISOR_HARDENING_PLAN.md) | 长跑 sidecar 稳定性 |
| [TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md) | **停止 parity 追平**后改为 desktop-only backlog 参考 |

---

## 2026-05-24 — Harness 组件化：从硬编码到可组合 Agent 执行结构

**背景：** 与 Agent（Zagens desktop session）进行了一整天架构对话。核心产出：harness 的进化线、跨领域泛化、组件化作为硬编码膨胀的解决方案。

### 关键结论

#### 1. 演化线

| 阶段 | 形态 | 本质 |
|------|------|------|
| Skill | SKILL.md prompt 注入 | 模型被「建议」怎么做 |
| Harness | prompt + 代码强制执行结构 | 模型被「确保」怎么做 |
| Harness 组件化 | 预置模块组合 + 参数化 | 用户可组合执行结构 |

#### 2. 数学模型：余代数统一

Agent+harness 架构统一于余代数 `X → F(X)`。LLM 是退化余代数 `X → O`（只有观测无状态转移），直线输出、不自主。主动性在 harness 的循环结构里（`handle_deepseek_turn` 的 `loop {}`），不在模型权重里。Harness 用非退化余代数围绕退化 LLM，构造非退化系统。

- 审批 = 监督控制自动机
- CRAFT = 拜占庭容错降级为不可靠节点检测
- Scratchpad = 事件溯源 + Lamport 因果序
- Compaction Pin = Bell-LaPadula 不可降级信息流
- Execpolicy = 安全自动机 DFA + 属性文法
- Sidecar = 能力安全模型（不可传递性）

互模拟保证：模块替换前后可观测行为不变。换 LLM 不破坏安全性质。

#### 3. Harness 的 7 个组件

| 组件 | 当前硬编码位置 | 可配置维度 |
|------|--------------|-----------|
| 审批策略 | `approval.rs` | auto / on-request / untrusted / never |
| 分发策略 | `dispatch.rs:should_parallelize_tool_batch` | naive / file-aware |
| 执行策略 | `execpolicy/` | read-only / workspace-only / full |
| 压缩策略 | `compaction` | trim-oldest / pin-protected / summary-first |
| 子代理拓扑 | `subagent/mod.rs` + `host_impl.rs` | off / craft / pb-bootstrap / teaching |
| LSP 集成 | `lsp_hooks.rs` | off / post-edit / full |
| 容量控制 | `capacity.rs` | off / warn-only / enforce |

#### 4. 组合规则

组件间有顺序依赖（审批→分发→执行→子代理），不能自由排列。需定义：
- **合法组合：** 依赖图约束
- **推荐组合（预置）：** `safe-default` / `pb-bootstrap` / `code-review` / `teaching` / `yolo`
- **废弃组合：** 语义矛盾组合产生警告（如 approval=never + dispatch=naive 合法但冗余）

#### 5. 跨领域泛化

同一套组件覆盖编程、教学、法律、医疗、金融审计。区别只在参数值和拓扑形状（CRAFT vs 交叉验证 vs 鉴别诊断 vs 并行审计）。Harness 是 **Agent 执行结构的可移植标准**，不仅服务于编程领域。

#### 6. 业界对照（Claude Code 2026）

Anthropic Managed Agents（2026.04 公测）使用完全相同的术语：session（只追加日志）、harness（调用模型+路由工具的循环）、sandbox（隔离执行环境）。

- Anthropic：**云端托管** harness（纵向），用户上传 session 定义
- Zagens：**本地可配置** harness（横向），用户控制执行结构
- Skills 在两边同时出现（Claude Code Skills 课程 + Zagens SKILL.md），同一季度结晶

#### 7. 实现路径（最小第一步）

不改 trait 定义、不改 turn_loop 执行流。在已有硬编码分支上加一层**配置驱动路由**：

1. `HarnessConfig` 结构体（`crates/config/src/lib.rs`）
2. `start_turn` 加载配置 → 注入 `TurnContext`
3. 各组件 match 分支读配置，替换硬编码常量
4. Zagens 设置面板 Harness 页（预置组合选择器 + 7 个组件下拉框）

### 产品定位

**预设组合是产品，配置面板是高级功能。** 90% 用户只碰预置，harness 配置暴露给 power user。与 Anthropic 的分叉：Anthropic 卖省心，Zagens 卖控制权。

### 与路线图关系

- 组件化依赖 P2 L2 终态 trait 边界（已达标）
- 不改变 A+ 契约（SSE 事件子集不变）
- 设置面板 Harness 页可作为 F 阶段项目
- CRAFT 从 prompt 约定升级为 SubAgentTopology 组件实现

**非阻塞：** 不改变任何门控依赖，可独立推进。

### 相关文档

| 文档 | 关系 |
|------|------|
| [RUNTIME_EVOLUTION_ROADMAP.md](../tech/RUNTIME_EVOLUTION_ROADMAP.md) | P2 L2 终态、门控链 |
| [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md) | CRAFT 作为 SubAgentTopology 实现 |
| [HARNESS.md](HARNESS.md) | Harness 定位与栈位 |

### 命名决定

**Zagens。** 经讨论，DS Pick → Pickcode / CodePick → AgentPick → Zagens。

| 候选 | 绑定 | 问题 |
|------|------|------|
| DS Pick | DeepSeek 生态 | 用户不知道 Pick 什么 |
| Pickcode / CodePick | 编程领域 | 跨领域是包袱 |
| AgentPick | Agent 范式 | .com 被域名投资者占据；agent 前缀已泛滥 |
| **Zagens** | 不绑任何东西 | 自创词，空白画布 |

**词源：** `agens` 是拉丁文 — `agere`（做、行动、驱动）的现在分词，意为「正在行动者」「驱动力」。Z + agens = 与众不同的驱动力。

和驾驭器的对应：

| 拉丁文 agens | 驾驭器的职责 |
|-------------|------------|
| 正在行动的那个 | turn_loop — 不是一次性的，是持续的 |
| 驱动者 | 不是模型在驱动过程，是驾驭器在驱动模型 |
| 行动的主体 | 主动性不在 LLM，在驾驭器 |

Z（科技品牌经典前缀，Zoom / Zendesk / 最后一个字母 = 与众不同）+ agens（拉丁文行动者）= 独立词源，不依附于 agent 的行业疲劳，不绑定任何现有词汇的语义包。

Zagens 已作为桌面产品名存在于代码仓库中（README、project_rules）。现在提升为项目级名称。

内部架构层叫**导引层**。中文不面向用户。

---

## 2026-05-21 — 会话/线程「结项汇总报告」（Handoff Report）— ⬜ 规划中

**背景（产品类比）：** 人类项目结束会写**总结报告**；以后查问题先看报告，而不是从原始邮件/会议记录从头翻。IDE Agent（如 Cursor）在长对话里会对**旧轮次做摘要压缩**，相当于机器侧的「报告」。Zagens **目前没有**与之对等、**用户可检索**的「结项汇总」机制。

### 现状：有压缩，无「报告」

| 能力 | 状态 | 说明 |
|------|------|------|
| **上下文压缩（compaction）** | ✅ | `compaction.rs`：token 超阈值时摘要旧消息、合并进 system prompt；`ThreadContextSnapshot` 供 UI 环 |
| **工具结果截断 / 摘要行** | ✅ | `compact_tool_result_for_context` 等；保护单轮 context，**不是**跨会话 handoff |
| **周期 `<carry_forward>`** | 🔶 | `cycle_manager`：轮次/周期内的 carry，服务于**同 thread 长跑**，≠ 用户打开的「上次干了啥」一页纸 |
| **审计 scratchpad `REPORT.md`** | 🔶 | 仅 **audit-repo** 等技能场景；`.deepseek/scratchpad/<run>/REPORT.md`，非通用聊天结项 |
| **会话 / 线程持久化** | ✅ | SQLite 消息体 + `GET …/events` 全量事件；恢复时 **replay**，不是先读摘要 |
| **桌面 `persistThreadSession`** | ✅ | turn 完成或周期 checkpoint 落盘；**无**结构化「结论 / 未决 / 下次入口」字段 |
| **通用 Handoff Report（结项汇总）** | ⬜ | **未做**：无 `thread.handoff.md`、无侧栏「上次摘要」、无新对话自动 `@` 上一份报告 |

**缺口：** 用户关掉窗口或隔天继续时，只能依赖**完整历史 replay** 或自己翻聊天记录；模型侧 compaction 摘要**不产品化**（用户看不见、不能编辑、不能当下一任务的固定上下文）。长任务（多轮审计、大功能开发）与「主动性 / 入座 briefing」北极星（见 [§2026-05-18](#2026-05-18--agent-方向与主动性北极星)）都更需要**可验收的一页结项**，而不是更长的事件流。

### 若迭代 — 草案方向（非承诺，待评审）

1. **触发：** `turn.completed` / 用户点「生成本轮摘要」/ 上下文 > N% 时建议生成（可关）。
2. **产物（示例路径）：** `~/.deepseek/handoffs/<thread_id>.md` 或 session 级 `handoff.json`（schema 待定），字段建议：目标、已完成、未决、关键路径/commit、**禁止编造**（仅锚工具输出 / scratchpad verified）。
3. **消费：** 新 thread / resume session 时 UI 提供「先读 handoff」；Composer 可选注入 `<thread_handoff>`（字数上限）；与 `<user_memory>`、CRAFT 黑板、audit scratchpad **分工**（见 [audit-scratchpad-design.md §2.1](audit-scratchpad-design.md) 事实/推理分离）。
4. **与 compaction 关系：** compaction 继续服务**模型 context**；handoff 服务**人与跨天接力**——可复用同一次 LLM 摘要调用，但存储与展示分离，避免「压缩了但用户找不到」。
5. **桌面：** 侧栏或会话卡片显示「上次摘要 · 3 行」；设置里「结项时自动写报告」默认关。
6. **Runtime API（候选）：** `POST /v1/threads/{id}/handoff` 生成、`GET` 读取；或 piggyback `persist-session` 扩展字段。

| 优先级 | 项 | 备注 |
|--------|-----|------|
| P0 | 写清 schema + 与 scratchpad / CRAFT 边界 | 避免三套「总结」互相打架 |
| P1 | 手动「生成结项摘要」+ 文件落盘 | 最小可用，无自动触发 |
| P2 | resume / 新会话注入 + UI 预览 | 对齐 Cursor「先看摘要再干活」体验 |
| P3 | 与入座 briefing、记忆图谱联动 | 依赖 [§2026-05-18](#2026-05-18--agent-方向与主动性北极星) 聚合层 |

**参考（仓库内）：** [prompt-architecture.md](../prompt-architecture.md) compaction 流 · [RUNTIME_EVOLUTION_ROADMAP.md](../tech/RUNTIME_EVOLUTION_ROADMAP.md) 会话/容量章节 · 流式重复修复案例（过程在聊天里，**结论在 CHANGELOG `[0.4.3]` + commit**）即「应写进仓库的报告」范式。

**决策备忘：** 短期 **不立项实现**；先在本节与 [DESKTOP_IMPLEMENTATION_PLAN.md](DESKTOP_IMPLEMENTATION_PLAN.md) 跟踪。若要做，优先 **P1 手动结项** 验证用户是否真的用，再考虑自动触发。

---

## 2026-05-20 — Harness 定位文档

| 项 | 状态 | 说明 |
|----|------|------|
| [HARNESS.md](HARNESS.md) | ✅ | JD → Zagens 模块表；三门工程；§7 战略备忘（非商业建议） |
| 与 scratchpad 交叉链接 | ✅ | [audit-scratchpad-design.md](audit-scratchpad-design.md) §2 链到 HARNESS §1–2 |

---

## 2026-05-21 — 工作台「目录」Tab

| 项 | 状态 | 说明 |
|----|------|------|
| [workspace-directory-plan.md](workspace-directory-plan.md) | 🔶 | 目录 Tab：A/B/C1/D 已落地；§10 跟踪 C2/C3、B4 等 |

---

## 2026-05-21 — 真多窗口（Cursor 式）— 已结案

| 项 | 状态 | 说明 |
|----|------|------|
| [multi-window-plan.md](multi-window-plan.md) | ✅ 结案 | M1–M4 + M6 已交付；T1–T2、T4 手测通过；**M5 延后**（几何记忆、资源管理器打开等 §7.5 backlog） |

---

## 2026-05-18 — Agent 方向与「主动性」北极星

### 产品北极星（入座 briefing）

**主动性**在此处的定义（非「更聪明的单轮回答」）：

> 用户回到电脑前时，Agent **主动汇报**（离开期间/后台发生的事）、**主动问今天要做什么**，并优先通过 **语音** 交流，少依赖先打开 Composer 打字。

可收成产品句：**入座 briefing** —— 像副驾接管开场白，而不是等用户发起对话。

最小闭环（后续实现时参照）：

1. **触发**：显式「我回来了」/ 空闲 N 分钟后首次键鼠 / 可配置（注意隐私，避免偷拍式监控）。
2. **汇总**：只读聚合——未完成后台 task、当前 thread 断点、可选记忆图 Top-K、CRAFT/open loops（仅已验证状态，禁止编造）。
3. **播报**：短稿 TTS（30～60 秒级）+ 一句带选项的开工问句（「继续 A 还是新任务 B？」）。
4. **接入**：STT 或快捷键确认 → 进入对应 TaskType / 线程 / 工作区。

**默认可关、可推迟、可仅文字**；Code / Office 分场景（语音偏意图路由，执行仍走现有工具面）。

| 能力块 | 状态 | 说明 |
|--------|------|------|
| 入座 briefing 任务与 API | ⬜ | 建议落点：desktop + `runtime_api`（如 `POST /v1/briefing` 或 session resume hook） |
| 在场检测 / 触发器 | ⬜ | 桌面 Tauri 侧；Windows 需单独设计 |
| TTS / STT 语音栈 | ⬜ | 含打断、静音、「勿播报」 |
| 汇报稿模板与幻觉约束 | 🔶 | 幻觉 patch、task 状态机可复用；尚无 briefing 专用聚合层 |
| 周期内上下文 briefing | 🔶 | `cycle_manager` 的 `<carry_forward>` 为**轮次压缩**用，≠ 入座汇报 |

---

### 五条战略轴线（与北极星的关系）

与官方 / 大厂招聘**不做对标**；个人项目尺子：**本地、可维护、少胡说、能值守**。

#### 1. 长程任务（大厂也在酝酿的方向）

跨轮、可中断、可恢复、可审计；Agent 记得「自己要干什么」。

| 项 | 状态 | 落点 / 备注 |
|----|------|-------------|
| 持久线程 / turn / item（HTTP API） | ✅ | `runtime_threads`、`/v1/threads`、事件流 |
| 后台 task（enqueue / cancel） | ✅ | `task_manager`、`/v1/tasks`；桌面「任务与技能」面板 |
| 会话持久化（SQLite WAL） | ✅ | `session_store_sqlite.rs`、`SessionManager`；由 JSON 单文件改为 SQLite，支持从旧 JSON 自动迁移 |
| Runtime 线程 / turn / item / 事件（SQLite） | ✅ | `thread_store_sqlite.rs`、`RuntimeThreadStore`；增量写入，含 `list_incomplete_turns` 等恢复语义 |
| 桌面 persist / 流式 checkpoint | ✅ | `App.tsx` + `persist-session` API；与 runtime 共用 SQLite，见 [2026-05-09](#2026-05-09--会话持久化与崩溃恢复--已解决sqlite) |
| Steer / 进行中回合控制 | ✅ | runtime steer API |
| 工作区快照 | ✅ | `snapshots` 配置与 side-git |
| 行业级「长程任务」产品化（日程、多 Agent 编排） | ⬜ | 非当前目标 |

#### 2. 场景分裂（借鉴 Claude Code：通用运行时 + 专精面具）

| 项 | 状态 | 落点 / 备注 |
|----|------|-------------|
| TaskType：`Office` / `Code` | ✅ | `task_type.rs`、overlay prompt、工具面裁剪 |
| 切换 TaskType = 新 session（KV 前缀稳定） | ✅ | 见 [task-type-prompt-architecture.md](../task-type-prompt-architecture.md) |
| Office：办公工具 + web + `load_skill` | ✅ | `tool_setup` / `with_office_surface` |
| Office 默认工作区、桌面 UI 隐藏项 | ✅ | Composer / RightPanel / Sidebar |
| Desktop Composer 切换与路由展示 | ✅ | `App.tsx`、`Composer.tsx` 等 |

#### 3. 可靠性：约束 + 可验证（含「第三方检验」）

高可行性输出场景引入 **第二进程 / 第二模型审核**（类似人类第三方检验）：主 Agent 产出 → 只读审核 → 结构化 verdict（pass / fix-list）→ 未通过则主 Agent 必须修。

| 项 | 状态 | 落点 / 备注 |
|----|------|-------------|
| Prompt 幻觉防控 V4（能力声明 / 架构描述 / 子代理输出） | ✅ | [prompt-hallucination-patch.md](../prompt-hallucination-patch.md)、`prompts/base.md` |
| 工具并行策略、子代理权限等「先查代码」清单 | ✅ | 文档 + `dispatch.rs`、`subagent` |
| 子代理 `review` 角色（工具面裁剪） | 🔶 | 已有 review 类子代理思路；**非**独立审核回合与强制门禁 |
| 高风险输出强制「审核子回合」（patch、定稿、对外文档等） | ⬜ | 今日方向；触发条件与 verdict 协议待设计 |
| 双模型并行「评委」式对话 | ⬜ | 非目标；要可编程、窄工具面 |

#### 4. 记忆：图谱 + 与 CRAFT / 用户记忆分工

| 层 | 状态 | 职责 |
|----|------|------|
| `<user_memory>` / `memory.md` | ✅ | 用户级持久偏好，`#` 快录、`remember` 工具 |
| Capacity memory（干预 JSONL） | ✅ | `capacity_memory.rs` |
| CRAFT 黑板（任务内结构化交接） | 🔶 | [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md) 持续推进 |
| **记忆图谱**（Topic Memory Graph） | 🔶 | 库与路线见 [UNDERLYING_ITERATION_REFERENCE.md §2.2–2.3](../tui/UNDERLYING_ITERATION_REFERENCE.md)（[topic-memory-graph](https://github.com/didclawapp-ai/topic-memory-graph)）；**未**系统化接入 runtime prompt |
| 图谱接地（路径 / commit / task_id） | ⬜ | 文档已列方向 |
| 图谱触发式注入（非每轮全图） | ⬜ | 服务「主动性」与入座 briefing |
| 用户「忘掉这条」降权/删边 | ⬜ | |

**定序（仍有效）：** 先 CRAFT 闭环，再记忆地图系统化接入，并约定与黑板、`<user_memory>` 的注入顺序与字数上限。

#### 5. 技能与生态

| 项 | 状态 | 落点 / 备注 |
|----|------|-------------|
| 技能扫描、`load_skill`、SKILL.md | ✅ | `skills/`、`SkillRegistry` |
| TUI `/skill install`（网络） | ✅ | `skills/install.rs` |
| Desktop：新建 / 本地导入 / 网络安装 API | ✅ | `POST /v1/skills`、`/import`、`/install`；AutomationPanel「添加技能」 |
| 技能与 Office / Code 提示词 catalog | ✅ | `prompts.rs`、`includes_skills_catalog` |

---

### 主动性的三层（验收用，避免空泛）

| 层 | 含义 | 状态 |
|----|------|------|
| **任务级** | 长程任务内拆步、记 open loops、blocked 换策略 | 🔶 task + CRAFT |
| **注意力级** | 根据索引/图谱决定先读什么、提醒未决决策 | 🔶 符号索引 ✅；图谱 ⬜ |
| **质量级** | 高风险产出前自发第三方审核 | ⬜ |

北极星 **入座 briefing** 主要覆盖 **任务级 + 注意力级** 的「开场」；**质量级** 靠审核进程。

---

### 建议演进顺序（个人排期，可调整）

1. **审核子回合最小闭环**（Code：multi-file patch 前强制 review）— 验证「可行性输出」。
2. **入座 briefing**（先文字稿 + 按钮触发，再 TTS，再 STT）。
3. **记忆图谱** 触发式注入 + 接地（与 briefing 共用聚合层）。

---

### 相关文档

| 文档 | 内容 |
|------|------|
| [task-type-prompt-architecture.md](../task-type-prompt-architecture.md) | TaskType MVP（✅） |
| [prompt-hallucination-patch.md](../prompt-hallucination-patch.md) | 幻觉防控 V4（✅） |
| [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md) | CRAFT、子代理、并行策略 |
| [tui/回归测试.md](../tui/回归测试.md) | 幻觉防控 R1–R8 题库（可复用于 Claude 对照） |
| [audit-scratchpad-design.md](audit-scratchpad-design.md) | 审计工作记忆；**Phase A ✅**（pick-rules §7、base.md、`audit-repo` skill） |
| [audit-scratchpad-test.md](audit-scratchpad-test.md) | Phase A 试跑记录（2026-05-19 冒烟 + 续审 ✅） |
| [tui/UNDERLYING_ITERATION_REFERENCE.md](../tui/UNDERLYING_ITERATION_REFERENCE.md) | CRAFT → 记忆地图定序 |
| [TOOLS_PRINCIPLES.md](../tech/TOOLS_PRINCIPLES.md) | 工具设计原则 |
| [API_DESIGN.md](../tech/API_DESIGN.md) | HTTP API |
| [TUI_DS_PICK_GAP.md](TUI_DS_PICK_GAP.md) | 桌面与 TUI 能力差距 |

---

## 2026-05-19 — 审计工作记忆（Audit Scratchpad）

**问题：** 全库审查时长回合内，reasoning 有 UI 缓存，但不足以当「工作记忆」；易出现早先检查项遗漏、后半程提前收口。

**方向：** 结构化外存（`inventory.json` + `notes.jsonl`），P0–P3 与 Auditor；与 CRAFT 黑板互补。

| 阶段 | 状态 | 落点 |
|------|------|------|
| **Phase A** | ✅ | `.deepseek/pick-rules.md` §7 · `base.md` · skill `audit-repo`；试跑见下 |
| **Phase B** | ✅ | [audit-scratchpad-design.md §6](desktop/audit-scratchpad-design.md) — 工具、注入、提醒、桌面进度、TTL |
| **Phase C** | ✅ C0–C3 | [§6.12](desktop/audit-scratchpad-design.md#612-phase-c--与-craft--auditor-深集成-排队)：compact、coverage gate、Auditor←scratchpad、blackboard 镜像 |

**试跑（2026-05-19）：** Phase A：`skills` + 续审 + 多区 `tui/src`（14 area）；Phase B：`2026-05-19-phase-b-smoke`（工具/门禁/续审/合成）**✅** → [audit-scratchpad-test.md](desktop/audit-scratchpad-test.md)。

---

## 2026-05-18 — Claude 对照实验（Zagens runtime × 外来模型）

**目的：** 在 **同一套 Zagens 运行时**（`dispatch.rs`、子代理环、幻觉 patch、TaskType）下挂 Claude 等模型，观察行为差异——**不是**复刻 Claude Code 产品，也不与 Anthropic 整链产品对标。

### 和 Claude Code 的本质差异

| 维度 | Claude Code（+ Claude 模型） | Zagens（+ 任意兼容 API 模型） |
|------|------------------------------|--------------------------------|
| 链条 | 客户端、系统提示、工具 schema、调度、模型对齐 **共设计** | 自研 runtime + prompt；**模型可换、宪法不变** |
| 并行叙事 | 宽泛规则：「无依赖则并行」，易让模型推断 Edit 可并行 | **`should_parallelize_tool_batch`**：整批须 `read_only && supports_parallel` |
| 推论 vs 事实 | 常先按原则回答，再读代码修正 | [prompt-hallucination-patch.md](../prompt-hallucination-patch.md) 要求能力/架构陈述 **先查代码** |
| 外来模型 | 同族模型，摩擦小 | 例如 DeepSeek V4 在 CC 外壳里 = **指令层 + 模型层 + 执行层** 拼装缝 |

**代码事实（与模型无关，换 Claude 也不变）：**

| 问题 | Zagens 结论 | 落点 |
|------|--------------|------|
| 主 agent 同轮并行 `edit_file`？ | **否** | `dispatch.rs` → `should_parallelize_tool_batch`；写工具非 `read_only` |
| 主 agent 同轮并行多文件 `read_file`？ | **是**（批内全只读时） | `turn_loop.rs` → `FuturesUnordered` |
| 子代理同 step 多 `read_file` 并行？ | **否** | `subagent/mod.rs` 串行 `for` + `await`，**不经过** `should_parallelize_tool_batch` |
| 子代理并行写？ | explore/review 裁剪；implementer 可写但串行 | `build_allowed_tools` + 同上循环 |

实测记录：在 Claude Code 里用 DeepSeek V4 问并行问题，模型曾按 CC **通用并行规则** 推论；读 Zagens 源码后与上表一致。见 [回归测试 R1、R6](../tui/回归测试.md)。

### 在 Zagens 里接 Claude（当前可行路径）

| 项 | 状态 | 说明 |
|----|------|------|
| 独立 `ApiProvider::Anthropic`（Messages API） | ⬜ | 未实现；需单独适配 tool 块格式 |
| **OpenRouter / Novita 等** OpenAI-compat 网关 | ✅ | `config.toml` + `/provider openrouter` |
| 桌面 Composer 模型下拉 | 🔶 | 仅 `deepseek-v4-pro` / `deepseek-v4-flash`；测 Claude 优先 **TUI** 或改 config 默认模型 |
| `context_window_for_model("claude…")` | ✅ | 约 200K（`models.rs`） |

**推荐步骤（TUI / sidecar 共用 `~/.deepseek/config.toml`）：**

```toml
provider = "openrouter"

[providers.openrouter]
api_key = "YOUR_OPENROUTER_KEY"
base_url = "https://openrouter.ai/api/v1"
model = "anthropic/claude-sonnet-4"   # 以 OpenRouter 模型列表为准
```

1. 保存配置，启动 TUI 或 Zagens（sidecar 读同一 config）。
2. TUI：`/provider openrouter`（或依赖上方 `provider =` 默认）。
3. **新开 session / thread**，TaskType 选 **Code**（与日常开发对照一致）。
4. 用 [回归测试](../tui/回归测试.md) **R1、R6** 等提问，要求 **引用 `dispatch.rs` / `subagent/mod.rs` 行号** 再结论。
5. 记录：是否仍胡说并行写、是否遵守 Capability Claims Rule、工具调用是否稳定。

**测什么 / 不测什么：**

- **测：** 在 Zagens prompt + 调度下，Claude 是否更少架构幻觉、工具链是否可靠、与 V4 的主观差异。
- **不测：** Claude Code 客户端体验；Anthropic 原生 extended thinking / prompt cache；「Claude Code 里换模型」的等价体验。

### 后续（可选）

- Desktop：Composer 支持自定义 `model` 字符串或 provider 切换（透传 thread API）。
- 第一方 Anthropic provider + 回归子集自动化。
- 子代理只读批并行：在 `subagent/mod.rs` 复用 `should_parallelize_tool_batch`（性能项，非哲学项）。

---

## 2026-05-09 — 会话持久化与崩溃恢复 — ✅ 已解决（SQLite）

**原问题：** 桌面端依赖 `~/.deepseek/sessions/*.json` 等在 **turn 完成** 后才落盘；进程异常退出、WebView 重载或回合未完成时，UI 与磁盘快照易脱节（上文不可见、侧栏历史不全）。

**现状（✅）：** 引入 **SQLite（WAL）** 作为主存储，会话与 runtime 线程状态均改为事务级增量写入，崩溃后重开可恢复到库内最后一笔一致状态。

| 范围 | 状态 | 落点 |
|------|------|------|
| 会话列表与消息体（SQLite WAL） | ✅ | `session_store_sqlite.rs` + `session_manager.rs`（`open_sqlite_session_db`，空库时从旧 JSON 迁移） |
| Runtime threads / turns / items / events | ✅ | `thread_store_sqlite.rs` + `runtime_threads.rs`（`open_sqlite_thread_db`；含未完成 turn 查询等） |
| **桌面 Zagens 对接** | ✅ | Web UI `persistThreadSession` → `POST /v1/threads/{id}/persist-session`（`runtime_api.rs` → `SessionManager::save_session` 写 SQLite）；流式生成中每 **18s** 周期 checkpoint（`App.tsx` `SESSION_CHECKPOINT_MS`）；`turn.completed` 等节点同样 persist；侧栏会话列表走 runtime 读 SQLite |
| 原「流式 checkpoint / JSONL 回填 UI」专项 | ✅ | Runtime SQLite + 桌面周期 persist 覆盖，不再单列后续块 |

**备注：** 旧 JSON 文件仍可作为迁移来源；新安装默认走 SQLite。桌面与 sidecar 共用同一 runtime 会话库，崩溃/重载后重连即可从库内恢复。若仍有边角（仅 UI 内存态未刷新的极端场景），按具体复现再开 issue。

---

*（有新条目时按日期追加在本文件顶部，或独立 dated 节。2026-05-24 含「产品战略方向备忘」与「Harness 组件化」；2026-05-18 含「主动性北极星」与「Claude 对照实验」。）*
