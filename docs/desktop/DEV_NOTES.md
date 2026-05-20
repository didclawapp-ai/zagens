# DS Pick 开发笔记

零散想法、后续方向与非正式排期；需要落地时再拆 issue / 写入 IMPLEMENTATION_STEPS。

**图例：** ✅ 已落地 · 🔶 部分 / 雏形 · ⬜ 规划中（未做或未产品化）

**Harness 总览：** [HARNESS.md](HARNESS.md) — DeepSeek 社招 JD 映射、本仓库栈位、会话恢复案例、「与官方关系」备忘。

---

## 2026-05-20 — Harness 定位文档

| 项 | 状态 | 说明 |
|----|------|------|
| [HARNESS.md](HARNESS.md) | ✅ | JD → DS Pick 模块表；三门工程；§7 战略备忘（非商业建议） |
| 与 scratchpad 交叉链接 | ✅ | [audit-scratchpad-design.md](audit-scratchpad-design.md) §2 链到 HARNESS §1–2 |

---

## 2026-05-21 — 工作台「目录」Tab

| 项 | 状态 | 说明 |
|----|------|------|
| [workspace-directory-plan.md](workspace-directory-plan.md) | ⬜ | 目录 Tab UI/功能分阶段方案；§0 总览表 + §10 任务清单供实施打勾 |

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

## 2026-05-18 — Claude 对照实验（DS Pick runtime × 外来模型）

**目的：** 在 **同一套 DS Pick 运行时**（`dispatch.rs`、子代理环、幻觉 patch、TaskType）下挂 Claude 等模型，观察行为差异——**不是**复刻 Claude Code 产品，也不与 Anthropic 整链产品对标。

### 和 Claude Code 的本质差异

| 维度 | Claude Code（+ Claude 模型） | DS Pick（+ 任意兼容 API 模型） |
|------|------------------------------|--------------------------------|
| 链条 | 客户端、系统提示、工具 schema、调度、模型对齐 **共设计** | 自研 runtime + prompt；**模型可换、宪法不变** |
| 并行叙事 | 宽泛规则：「无依赖则并行」，易让模型推断 Edit 可并行 | **`should_parallelize_tool_batch`**：整批须 `read_only && supports_parallel` |
| 推论 vs 事实 | 常先按原则回答，再读代码修正 | [prompt-hallucination-patch.md](../prompt-hallucination-patch.md) 要求能力/架构陈述 **先查代码** |
| 外来模型 | 同族模型，摩擦小 | 例如 DeepSeek V4 在 CC 外壳里 = **指令层 + 模型层 + 执行层** 拼装缝 |

**代码事实（与模型无关，换 Claude 也不变）：**

| 问题 | DS Pick 结论 | 落点 |
|------|--------------|------|
| 主 agent 同轮并行 `edit_file`？ | **否** | `dispatch.rs` → `should_parallelize_tool_batch`；写工具非 `read_only` |
| 主 agent 同轮并行多文件 `read_file`？ | **是**（批内全只读时） | `turn_loop.rs` → `FuturesUnordered` |
| 子代理同 step 多 `read_file` 并行？ | **否** | `subagent/mod.rs` 串行 `for` + `await`，**不经过** `should_parallelize_tool_batch` |
| 子代理并行写？ | explore/review 裁剪；implementer 可写但串行 | `build_allowed_tools` + 同上循环 |

实测记录：在 Claude Code 里用 DeepSeek V4 问并行问题，模型曾按 CC **通用并行规则** 推论；读 DS Pick 源码后与上表一致。见 [回归测试 R1、R6](../tui/回归测试.md)。

### 在 DS Pick 里接 Claude（当前可行路径）

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

1. 保存配置，启动 TUI 或 DS Pick（sidecar 读同一 config）。
2. TUI：`/provider openrouter`（或依赖上方 `provider =` 默认）。
3. **新开 session / thread**，TaskType 选 **Code**（与日常开发对照一致）。
4. 用 [回归测试](../tui/回归测试.md) **R1、R6** 等提问，要求 **引用 `dispatch.rs` / `subagent/mod.rs` 行号** 再结论。
5. 记录：是否仍胡说并行写、是否遵守 Capability Claims Rule、工具调用是否稳定。

**测什么 / 不测什么：**

- **测：** 在 DS Pick prompt + 调度下，Claude 是否更少架构幻觉、工具链是否可靠、与 V4 的主观差异。
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
| **桌面 DS Pick 对接** | ✅ | Web UI `persistThreadSession` → `POST /v1/threads/{id}/persist-session`（`runtime_api.rs` → `SessionManager::save_session` 写 SQLite）；流式生成中每 **18s** 周期 checkpoint（`App.tsx` `SESSION_CHECKPOINT_MS`）；`turn.completed` 等节点同样 persist；侧栏会话列表走 runtime 读 SQLite |
| 原「流式 checkpoint / JSONL 回填 UI」专项 | ✅ | Runtime SQLite + 桌面周期 persist 覆盖，不再单列后续块 |

**备注：** 旧 JSON 文件仍可作为迁移来源；新安装默认走 SQLite。桌面与 sidecar 共用同一 runtime 会话库，崩溃/重载后重连即可从库内恢复。若仍有边角（仅 UI 内存态未刷新的极端场景），按具体复现再开 issue。

---

*（有新条目时按日期追加在本文件顶部，或独立 dated 节；2026-05-18 含「主动性北极星」与「Claude 对照实验」两节。）*
