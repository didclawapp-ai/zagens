# DeepSeek-TUI / Zagens 运行时演进实施路线图

> **版本：** v2.0-final（2026-05-21）· **代码对齐：** 2026-05-26（**D6 Phase B** — `deepseek-runtime` 单 crate；CLI + ratatui TUI 已删除）  
> **状态：** **定稿 + 门控已闭合** — §4.2 已签收；**§17** 实施后审核（**§17.5** 2026-05-25 二次对齐）  
> **门控：** §12.2 A+ · §12.3 P2 + G3 + PR6 · §12.4 F · §12.5 #1/#2 · **§12.1 #1 A1 live `ToolCell` 同构** **✅**；§12.5 #3 GAP **◐**（产品 polish，非门控）  
> **v1.6 要点：** 修正 §6.2 步骤 0.8 引用、§12.1 A5.1 门控表述、§1.4 定稿章节策略、`RUNTIME_BASELINE.md` 占位  
> **受众：** 维护者、Agent、桌面开发  
> **产品节奏：** Phase 1 harness **已验收** → **A+A+ 打底** → **P2 还技术债**（Engine→core）→ **解冻桌面 GAP**；壳运 **始终分离**。  
> **当前架构评估：** [`adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md`](./adr/ARCHITECTURE_ASSESSMENT_2026-05-25.md)（§1 **10/10 定型**；D6 Phase B ✅；M-series ✅）

---

## 目录

1. [目标与原则](#1-目标与原则)
2. [三层架构：底层 / 契约 / 壳（分离且融合）](#2-三层架构底层--契约--壳分离且融合)
3. [现状快照](#3-现状快照)
4. [战略决策（必读）](#4-战略决策必读)
5. [阶段总览（底层优先序）](#5-阶段总览底层优先序)
6. [阶段 0 — 对齐与治理（1–2 周）](#6-阶段-0--对齐与治理12-周)
7. [阶段 A — 底层夯实（4–10 周，主战场）](#7-阶段-a--底层夯实410-周主战场)
8. [阶段 A+ — 融合就绪（契约层，A 中后期）](#8-阶段-a--融合就绪契约层a-中后期)
9. [阶段 B — 差异化能力（底层达标后）](#9-阶段-b--差异化能力底层达标后)
10. [阶段 F — 桌面融合（P2 后解冻）](#10-阶段-f--桌面融合p2-完成后解冻d10)
11. [阶段 P2 — 还 Phase 2 技术债（主债）](#11-阶段-p2--还-desktop_implementation_plan-phase-2-技术债主债)
12. [里程碑与验收清单](#12-里程碑与验收清单)
13. [风险与不做清单](#13-风险与不做清单)
14. [Issue 拆分模板](#14-issue-拆分模板)
15. [关联文档](#15-关联文档)
16. [文档审核记录](#16-文档审核记录)
17. [Zagens 实施后审核（2026-05-22）](#17-ds-pick-实施后审核2026-05-22) · [§17.5 代码对齐（2026-05-24）](#175-代码对齐快照2026-05-24)  
    · **实施总结（2026-05-24）：** [adr/IMPLEMENTATION_SUMMARY_2026-05-24.md](./adr/IMPLEMENTATION_SUMMARY_2026-05-24.md)

> **执行顺序阅读指引（与章节编号无关）：** §7 A → §8 A+ → §11 P2 → §10 F → §9 B（见 §1.6）。正文 §9–§11 按 **文档结构** 编号（B→F→P2），**勿按 9→10→11 数字顺序执行**。`v2.0-final` 可选将正文重排为 §9 P2 → §10 F → §11 B（见 §1.4）。

---

## 1. 目标与原则

### 1.1 总目标

在 **不推翻现有 sidecar 架构** 的前提下：

1. **先把底层打牢固**（Engine、会话、容量、可观测、错误、测试）——这是全产品的地基。  
2. **再通过稳定契约与桌面对齐**（HTTP/SSE + 少量 Tauri IPC），实现 TUI 与 Zagens **同一底层、两种壳**。  
3. **长期保持分离：** 桌面壳（`crates/desktop`）与运行时（`deepseek-tui`）**进程与职责分离**；融合 = 消费契约，≠ 把 Engine 搬进 WebView 或 Tauri。  
4. **冻结** `app-server` 实验栈，避免在「未完成的第二运行时」上叠桌面或差异化功能。

### 1.2 八条原则

| # | 原则 |
|---|------|
| P1 | **底层优先：** 阶段 A 完成线未达标前，桌面只做 **维护/契约消费/冒烟**，不抢跑大型 UI 特性或新 runtime 语义 |
| P2 | **壳运分离：** Zagens = Tauri 壳 + `deepseek-tui` sidecar；Agent turn **只在** sidecar 内执行 |
| P3 | **单事实源：** A 阶段新能力进 `tui` 生产链；**P2 后** 新 Agent 逻辑进 `deepseek-core`，`tui` 仅 HTTP/TUI 适配 |
| P4 | **契约融合：** 桌面与底层通过 **版本化 API/事件子集** 融合，而非代码库合并 |
| P5 | **增量拆分：** 巨石模块按域拆分；每步 `cargo test`，底层变更后跑 **sidecar 契约测** |
| P6 | **差异化后置：** CRAFT、记忆地图、xterm/diff 等大项在 **A + A+** 之后系统推进 |
| P7 | **门控：** A+A+ 达标 → 启动 P2；[§12.3](#123-阶段-p2技术债完成线解除-d10-桌面-feature-freeze) 达标 → **解冻** 阶段 F |
| P8 | **诚实安全：** 非 macOS 沙箱须在壳层 UI/文档标明 |

### 1.3 「融合」与「合并」的区别

| 概念 | 本路线图含义 | 不做 |
|------|--------------|------|
| **融合** | 桌面消费已稳定的 `/v1/*`、SSE 子集、Tauri 命令；与 TUI 共享同一 sidecar 二进制与行为 | — |
| **合并** | — | WebView 内跑 Engine；desktop crate 依赖 `engine.rs`；取消 sidecar |

### 1.4 定稿政策

- 本路线图须 **至少两轮审核** 后方可标为 **定稿**（`v2.0-final`）；第三轮（v1.5）已处理量化基准、P2 复杂度；**v1.6** 修正引用与门控表述。
- **执行期：** v1.5 起可开 R-00x issue；重大范围变更（换 sidecar、取消 P2）须维护者 ADR。
- **`v2.0-final` 可选变更：** 维护者签收后，可将正文 §9–§11 **重编号** 为执行序（P2→F→B），并同步 TOC；**不** 改阶段代号（A/A+/P2/F/B）。当前 v1.6 以文首指引 + §1.6 门控链为执行 SSOT，**不阻塞** 开工。

### 1.5 术语对照表（避免 issue 命名冲突）

| 名称 | `DESKTOP_IMPLEMENTATION_PLAN.md` | 本路线图 | Issue 建议前缀 |
|------|----------------------------------|----------|----------------|
| Phase 1 | 路线 B / harness | Phase 1 收官 | `harness` |
| Phase 2 | Tauri 脚手架、基础对话（ largely 已完成） | — | `desktop-phase2` |
| Phase 3 | 工具可视化、ApprovalDialog 等 | — | `desktop-phase3` |
| **Phase 2（三级火箭）** | **Engine → `deepseek-core`** | **阶段 P2 / P2-debt** | **`P2-debt`** |
| Phase 3（三级火箭·原稿） | ~~换 app-server sidecar~~ | **已废止** | — |
| **P3′** | 库层统一、sidecar 不变 | §11.3 | `P3-prime` |
| 原「阶段 C」（v1.0–v1.1） | — | **= 本文阶段 P2**（非桌面 Phase 2） | 勿再开 `stage-C` |

### 1.6 统一门控链（SSOT）

```
Phase 1 ✅ harness
    → A 完成线（§12.1，仅 L1）
    → A+ 完成线（§12.2，仅 L2）     ← 可与 A 尾部重叠，但验收独立
    → P2 完成线（§12.3）           ← 编码以 §12.2 为准；A5 + 契约测为硬门槛
    → F 解冻（§12.4，桌面 GAP）
    → B（CRAFT / 记忆地图，§12.5）
```

**与 [UNDERLYING_ITERATION_REFERENCE.md](../tui/UNDERLYING_ITERATION_REFERENCE.md) 的差异：** UNDERLYING 允许打底 **1+2** 后开独有功能原型；本路线图 **更严** — CRAFT/桌面 GAP 高峰在 **P2 后**，以免 Engine 仍焊在 `tui` 时叠差异化。

---

## 2. 三层架构：底层 / 契约 / 壳（分离且融合）

```
┌─────────────────────────────────────────────────────────────┐
│  L3 壳（Presentation Shell）                                 │
│  ├─ Zagens: Tauri + React（工作区、布局、原生对话框）          │
│  └─ TUI: ratatui（终端交互）                                  │
│       只通过 L2 与底层对话；壳内无 LLM turn                    │
├─────────────────────────────────────────────────────────────┤
│  L2 契约（Fusion Contract）  ← 融合面，A+ 阶段重点打磨          │
│  ├─ HTTP/SSE: runtime_api /v1/* + event_schema_version      │
│  ├─ Tauri IPC: 文件/密钥/二进制读/sidecar 监督（Channel A）   │
│  └─ runtime_proxy: Bearer 不出 WebView                       │
├─────────────────────────────────────────────────────────────┤
│  L1 底层（Runtime Foundation）  ← 阶段 A 主战场 ★             │
│  deepseek-tui: Engine · turn_loop · session · compaction      │
│                · tools · client · runtime_threads             │
└─────────────────────────────────────────────────────────────┘
```

**实施顺序强制为：L1 夯实 → L2 稳定并文档化 → L3 按契约扩展 UI。**

---

## 3. 现状快照

> **2026-05-26 更新（D6 Phase B）：** 生产 sidecar 为 **`deepseek-runtime`**（`crates/runtime-server`）；~~`crates/cli`~~、~~`crates/tui`~~、~~ratatui TUI~~ **已删除**。下文 §3.1–§3.4 中仍写 `deepseek-tui` / `crates/tui` 的表格与路径为 **2026-05-25 历史快照**，以本节 §3.1 为准。

### 3.1 生产路径（Zagens + Headless HTTP）

```
用户
  ├─ Zagens WebView ──► Tauri (IPC + runtime_proxy) ──► deepseek-runtime (sidecar)
  └─ 脚本 / CI / Headless ──► HTTP + Bearer ──► deepseek-runtime

deepseek-runtime  (crates/runtime-server — lib: deepseek_runtime)
  ├─ runtime_api/        HTTP /v1/* + SSE（router.rs）
  ├─ runtime_threads/    线程/回合/广播/持久化
  ├─ core/engine/        ~130 LOC shim + platform_dispatch
  ├─ deepseek-core       Engine struct + op_loop + turn_loop
  ├─ client/             LLM SSE 解析
  ├─ tools/*             工具实现主体
  ├─ mcp/                MCP 池
  └─ deepseek-tools      workspace 类型/trait
```

> **架构附图：** [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) · **Phase B ADR：** [D6_PHASE_B_CLI_SUNSET.md](./adr/D6_PHASE_B_CLI_SUNSET.md)

### 3.1a 历史生产路径（2026-05-25 前，已废止）

```
用户
  ├─ Zagens WebView ──► … ──► deepseek-tui serve --http
  ├─ 终端 TUI ─────────► deepseek-tui (ratatui)
  └─ deepseek CLI ─────► delegate_to_tui ──► deepseek-tui
```

### 3.2 实验路径（仅 `deepseek app-server`）

```
deepseek app-server
  └─ crates/app-server → deepseek-core::Runtime
       └─ handle_thread(Message) → "queued" 占位（不跑完整 turn）
```

### 3.3 Workspace 库实际引用

| Crate | deepseek-tui | deepseek-desktop | deepseek CLI | 说明 |
|-------|:------------:|:----------------:|:------------:|------|
| config | ✅ | ✅ | ✅ | |
| secrets | ✅ | ✅ | ✅ | |
| tools (+protocol 类型) | ✅ | ❌ | 经 app-server | **实现**在 `tui/src/tools`；**类型**在 `deepseek-tools` → `protocol`。P2 默认 **不迁** 工具实现，仅迁 Engine/session（§11.0 ADR） |
| core | ✅ | ❌ | app-server + **tui 生产链** | **P2 L2 终态：** `turn_loop`/`session`/类型在 core；`Engine` struct 仍留 tui（[BACKLOG_ENGINE_STRUCT_IN_CORE.md](./adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md)） |
| topic-memory | ✅ | ✅（设置 + `TopicMemoryPanel`） | ❌ | **B2** — `crates/topic-memory` + `tui/topic_memory.rs`；`GET /v1/topic-memory` |
| state | ❌ | ❌ | thread 元数据 | 与 runtime_threads 并行 |
| tui-core | ❌ | ❌ | ❌ | **legacy** — 仅 snapshot 测；B3.2 定案不接入（见 `crates/tui-core/README.md`） |

### 3.4 代码体量快照（2026-05-25 对齐）

| 路径 | 约行数 | 路线图动作 | 实施状态 |
|------|--------|------------|----------|
| `crates/tui/src/runtime_api/mod.rs` | ~100 | **A4** 拆分 | **✅ 达标** — 域模块 + `tests.rs`；主文件保留 `run_http_server`、`ApiError`、CORS、`ThreadEventsQuery` |
| `crates/tui/src/runtime_threads/mod.rs` | ~275 | **A4.6** | **✅ 主文件达标** — 测试在 `tests.rs`（~2140） |
| `crates/tui/src/runtime_threads/manager.rs` | ~585 | **A4.6** | **✅ 已拆** — 逻辑在 `turn_lifecycle` / `persist` / `monitor` 等；主文件达标 |
| `crates/core/src/session.rs` | ~183 | **P2 PR2** | **✅** — `Session`/`SessionUsage` + `working_set`/`project_context`/`ApprovalMode` |
| `crates/tui/src/core/engine.rs` | ~192 | **P2 PR4/PR6** | **✅ 薄壳** — `turn_loop` 阶段在 core；`host_impl` + `tool_plans_exec` 在 tui |
| `crates/core/src/engine/turn_loop/streaming_phase.rs` | ~700 | **P2 PR6** | **✅** — 可选拆 `stream_poll` 子模块 |
| `crates/core/src/lib.rs` | ~1838 | **P2** 并入 Engine | **L2 终态** — `turn_loop` 阶段 + 类型在 core；整包 `Engine` struct / `Runtime` 职责仍 split（backlog ADR） |
| `crates/tui/src/lib.rs` | 有 | **A5.1** | **✅** lib target 已存在 |
| `crates/tui/src/main.rs` | ~350 | **B3.1** CLI 拆分 | **✅** — 命令在 `cli/commands/legacy.rs`；测试在 `cli/tests.rs` |
| `crates/topic-memory/src/lib.rs` | ~85+ | **B2** | **✅ MVP** — 图引擎 + k-hop 注入 + metrics；`TopicMemoryPanel` + `/v1/topic-memory`；B2.1/B2.5 |

**已有能力（避免重复立项）：** `tools/large_output_router.rs`（大 tool 输出外部化/路由）、`core/capacity*.rs`（容量估算）、`history_isomorphism`（user/assistant/thinking 同构测）。A1 live 同构等余项见 §17.5。

---

## 4. 战略决策（必读）

### 4.1 已采纳（2026-05，与 DESKTOP_IMPLEMENTATION_PLAN 一致）

| 决策 | 内容 |
|------|------|
| **D1** | Zagens **Phase 1 = 路线 B**：sidecar 为 `deepseek-tui`，HTTP 面为 `runtime_api`，**不** 使用 `app-server` |
| **D2** | 桌面鉴权使用 `runtime_token`；**不** 使用 `app-server` 的全开放 CORS |
| **D3** | 差异化顺序：**CRAFT → 话题记忆地图**（见 UNDERLYING_ITERATION_REFERENCE §2.2） |
| **D8 Phase 1 收官** | **Harness 验证已通过**（2026-05） | 路线 B 证明 `deepseek-tui` 可承载桌面；不再为「能否做桌面」扩功能 |
| **D9 P2 技术债** | **A+A+ 后必做**（非可选 C） | Engine 上移 `deepseek-core`（DESKTOP_IMPLEMENTATION_PLAN Phase 2） |
| **D10 桌面功能冻结** | **P2 完成前** | 不扩大 Zagens GAP；避免壳层特性固化后无法迁底层 |
| **D11 P3 修订** | **不换 app-server sidecar** | P3 = `runtime_api` 适配 `core::Runtime`；sidecar 仍为 `deepseek-tui` |

### 4.2 本路线图新增（维护者签收栏）

| 决策 | 推荐选项 | 说明 | 签收 |
|------|----------|------|------|
| **D4 app-server 政策** | **冻结** | 不删 crate；不新增 turn/API；**P3 不依赖** 切换至 app-server HTTP | ✅ 2026-05-21 维护者签收 |
| **D5** | **原路线图「阶段 C」已废止并并入 P2** | 原意 = v1.0 可选「Engine 上移 `deepseek-core`、统一 runtime」（现 §11）；**非** DESKTOP_PLAN 的 UI Phase 2。旧 issue 标 `stage-C` → `P2-debt` | ✅ 2026-05-21 维护者签收 |
| **D6 文档 SSOT** | 本文件 + API_DESIGN.md | 架构叙述以本文 & API_DESIGN 为准；对外 HTTP 事件以 **A+ compat 子集** 为准，非立即统一 `EventFrame` | ✅ 2026-05-21 维护者签收 |
| **D7 节奏** | **底层 + P2 优先** | 阶段 F（桌面 GAP）高峰在 **P2 完成线** 之后 | ✅ 2026-05-21 维护者签收 |
| **D9** | **P2 必做** | 见 §4.1；与 D10 freeze 绑定 | ✅ 2026-05-21 维护者签收 |

### 4.3 Phase 1 harness 验收（2026-05）

| 当初问题 | 现答案 |
|----------|--------|
| `deepseek-tui` 能否作桌面 harness？ | **能** — sidecar + `/v1/*` + 真 turn |
| 设计是否值得长期投入？ | **是** — 宜沉入 `core`，不宜再堆桌面专有 runtime 语义 |
| Phase 1 之后最忌什么？ | **继续扩 Zagens GAP**，导致 P2 迁移面不可逆增大 |

---

## 5. 阶段总览（底层优先序）

```mermaid
gantt
  title 底层优先 → 契约融合 → 桌面扩展
  dateFormat YYYY-MM-DD
  section L1 底层
  阶段0 治理              :p0, 2026-05-21, 14d
  A 底层夯实              :a, after p0, 56d
  section L2 契约
  A+ 融合就绪             :ap, after a, 21d
  note right of ap
    可与 A 尾部重叠
  end note
  section L1 P2债
  P2 Engine上移core       :p2, after ap, 56d
  section L3 壳
  F 桌面GAP解冻           :f, after p2, 42d
  B 差异化 CRAFT等        :b, after f, 56d
```

| 阶段 | 层级 | 周期（参考） | 产出 |
|------|------|-------------|------|
| **0** 治理 | — | 1–2 周 | SSOT、app-server 冻结、**桌面 Feature freeze 公告** |
| **A** 底层夯实 | **L1** | 4–10 周 | 会话/容量/可观测/错误/模块/测试 — **主投入** |
| **A+** 融合就绪 | **L2** | A 中后期 3–5 周 | HTTP 事件子集、sidecar 契约测 |
| **P2** 还技术债 | **L1→core** | A+ 后 6–10 周 | Engine/`turn_loop` 迁入 `deepseek-core`；`runtime_threads` 调 core |
| **F** 桌面融合 | **L3** | **P2 后** 4–8 周 | Zagens GAP（xterm、diff…）— **冻结至 P2 完成** |
| **B** 差异化（L1 部） | L1/L2 | **P2 后、F 前** | CRAFT 黑板/工具（runtime） |
| **B** 差异化（L3 部） | L3 | **F 后** | 记忆地图 UI、AgentPanel 等 |

**人力建议：** 直至 **§12.3 P2 完成线**：**80%+** 在 L1（A+A++P2）；桌面仅维护/冒烟（D10）。

**Phase 1 背景：** 桌面曾用路线 B **验证 harness**；验证已通过，故不再以「扩桌面功能」为主线，而以 **P2 避免债固化** 为主线。

---

## 6. 阶段 0 — 对齐与治理（1–2 周）

### 6.1 目标

消除「红框 = 桌面接口」「app-server = 主 HTTP」等误解；书面确认 **L1/L2/L3 分离 + 底层优先**。

### 6.2 实施步骤

| 步骤 | 任务 | 产出物 | 负责人建议 |
|------|------|--------|------------|
| 0.1 | 架构总览附图 | [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) | 文档 ✅ |
| 0.2 | 更新 `docs/tech/API_DESIGN.md` 文首：**唯一生产 HTTP = runtime_api** | 醒目 callout | 文档 ✅ |
| 0.3 | 更新 `docs/desktop/DESKTOP_IMPLEMENTATION_PLAN.md` §2.1 顶部：**Phase 1 已落地，勿新建 app-server 依赖** | 防回归 | 文档 ✅ |
| 0.4 | 更新 `README.md` 中 `app-server` 描述为 **Experimental — not used by Zagens** | 对外一致 | 文档 ✅ |
| 0.5 | 维护者确认 **D4**（冻结 `app-server`；**不**再开「阶段 C」— 已并入 **P2**）并记入 §4.2 签收栏 | 决策 | Tech lead |
| 0.6 | `project_rules.md` 增加可 grep 禁令：**禁止 app-server 扩展；禁止 WebView 内嵌 Engine** | 流程 | 工具链 ✅ |
| 0.7 | 签收 **D4/D7/D9** 与 §1.6 门控链；发布桌面 **Feature freeze** 公告 | 决策 | Tech lead |
| 0.8 | **PR 审查规则（freeze）：** 标签 `area:desktop` 的 PR 须说明是否符合 §10.6；超出 freeze 须标 `freeze-exception` + 维护者 ack | `CONTRIBUTING` 或 `project_rules.md` | **✅** `project_rules.md`（D10 解除后仍保留 ack 流程） |

### 6.3 验收

- [ ] 新成员能回答：底层在哪？融合面是哪一层？桌面为何不合并进 tui？
- [ ] 无开放 PR 把 Zagens 功能接到 `app-server` 或在 WebView 实现 turn
- [x] 运行时/桌面 **维护性** 修复可合并（例：模型输出重复、SSE 去重）— 不视为 GAP 扩张，见 §10.6

---

## 7. 阶段 A — 底层夯实（4–10 周，主战场）

**层级：L1。** 本阶段 **不依赖** 桌面新功能交付；TUI 与 sidecar 共用同一底层，TUI 可随 A 项一并验证。

对应 [UNDERLYING_ITERATION_REFERENCE.md](../tui/UNDERLYING_ITERATION_REFERENCE.md) 阶段 A 与 §3.1–3.8。

### 7.1 A1 — 会话与消息存储（优先级最高）

**问题：** 长跑会话 `api_messages`、工具结果、JSONL/SQLite 全量序列化导致内存与主线程阻塞；compaction 与 TUI transcript 可能分叉。

**现状（2026-05-25 代码对齐）：** 已有 `context_partition` 热/冷、`large_output_router` + `artifact_refs`、`history_isomorphism` 单测、`trim_oldest_messages_to_budget`（`VecDeque` 式批量 drain，非 `remove(0)`）、R-015 RSS 基线 + `-Gate`；[A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md) 已签收；HTTP **`events_since_async`**（`spawn_blocking`）已用于 `runtime_api/stream.rs` / `sessions.rs`。**A1 follow-up（2026-05-25 闭合）：** live `ToolCell` 与 `session.messages` 同构升级为 **生产级** — `App::check_live_history_isomorphism(site)` 在 release 也走 `tracing::warn!` + `history_isomorphism::drift_count` 计数；debug 保留 `debug_assert!`（详见 [A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status）。

| 步骤 | 实施内容 | 主要文件 | 状态 |
|------|----------|----------|------|
| A1.1 | 定义 **热窗口 / 冷区** 数据模型（近期消息 + pin vs 摘要/外部引用） | `core/session.rs`, `compaction.rs` | **🟡** `context_partition` MVP |
| A1.2 | 超大 tool output **外部化**：持久化 ref + 消息同构 | `large_output_router.rs`, `runtime_threads.rs`, `tools/*` | **🟡** ref 已有；live 同构未全 |
| A1.3 | 落盘 **节流 + spawn_blocking**；crash-safe checkpoint | `thread_store_sqlite.rs`, `session_store_sqlite.rs`, `manager.rs` | **✅** HTTP `events_since_async`；persist 路径已审计（[A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md)） |
| A1.4 | compaction/trim 后 **同构校验** | `compaction.rs`, `tui/history.rs`, `runtime_threads.rs` | **🟡** `history_isomorphism` 单测 + debug_assert |
| A1.5 | `trim_oldest_messages_to_budget`：`VecDeque` 或批量 `drain` | `context_recovery.rs` | **✅** |
| A1.6 | **长跑基准脚本** | `scripts/runtime-longrun-baseline.ps1` | **✅** RSS 已写入 [adr/RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md) + `-Gate`；§12.1 #1 维护者签收仍可选 |

**A1-MVP（可分阶段验收，避免 4–10 周全耗 A1）：**

| 子项 | 内容 | 验收 |
|------|------|------|
| A1-MVP.1 | **large_output_router** 产出 ref 与 session/JSONL **一致** + 消息内 ref | 单测 + 一轮长跑（R-015 脚本可选） |
| A1-MVP.2 | compaction **pin** 单测（与 A5.4 可合并） | pin 不被删 |
| A1-full | 热/冷窗口 + 落盘同构 + A1.6 全量基准 | §12.1 #1 |

**验收：**

- [x] A1-MVP 或 A1-full 至少达成其一后，A 项 #1 可标部分通过（**A1-MVP+** 已达成：ref 路由 + pin 测 + R-015 RSS）
- [x] compaction 后无「UI 有、引擎无」类 silent bug（`history_isomorphism` + live `check_live_history_isomorphism` — 2026-05-25 升级为生产级 warn + drift counter）

---

### 7.2 A2 — Turn 级可观测与事件模型（L1，为 L2 铺路）

**问题：** 排障依赖零散 `status` 文案；后续桌面融合需要稳定事件模型（完整子集在 A+ 冻结）。

**现状（2026-05-25）：** `TurnSummary`、`turn.completed` SSE、`RuntimeEventRecord` + tracing span 已落地；§12.1 #2 **✅** [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)（2026-05-25）。

| 步骤 | 实施内容 | 主要文件 | 状态 |
|------|----------|----------|------|
| A2.1 | 扩展 `EngineEvent` / `RuntimeEventRecord`：`turn_summary` | `core/events.rs`, `runtime_threads.rs` | **✅** |
| A2.2 | Turn 结束 emit 结构化事件；映射到 SSE compat 名 | `runtime_api/stream.rs` | **✅** |
| A2.3 | 内部文档草案：稳定事件子集 v1 | [A2_TURN_OBSERVABILITY_V1_DRAFT.md](./adr/A2_TURN_OBSERVABILITY_V1_DRAFT.md) | **🟡** 草稿 |
| A2.4 | （移至 A+）桌面 `client.ts` 对齐子集 | 见 §8 | **✅** |
| A2.5 | 结构化 tracing span：`turn_id`, `thread_id`, `step` | `logging.rs`, `turn_loop.rs` | **✅** |

**验收：**

- [x] 单条 turn 可从 **日志或内部 EngineEvent** 回答：几步、哪些工具、为何结束（SSE 子集对齐在 **A+**）— [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) 2026-05-25

---

### 7.3 A3 — 失败路径与重试分类

**现状：** 已有 `ErrorCategory`、`ErrorEnvelope`（`error_taxonomy.rs`）；**36** golden/边界单测在 `crates/core/src/error_taxonomy.rs`。

| 步骤 | 实施内容 | 主要文件 | 状态 |
|------|----------|----------|------|
| A3.1 | 30+ golden/边界单测 | `error_taxonomy.rs` | **✅** 36 tests |
| A3.2 | 扩展 `ErrorCategory` 并与 `ErrorEnvelope` 对齐 | `error_taxonomy.rs` | **✅** |
| A3.3 | `turn_loop` 重试仅对可重试类 | `core/engine/turn_loop.rs` | **✅** |
| A3.4 | TUI 与 HTTP 错误 payload 统一 | `runtime_api`, `tui` 错误 UI | **🟡** 核心已统一；边缘 UI 可再抛光 |

**验收：**

- [x] 网络断连与 reasoning 400 用户可见文案不同 — [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) 2026-05-25
- [x] 无「业务不可重试仍刷 3 次」的额度浪费（可测）— 36 golden + turn_loop 策略

---

### 7.4 A4 — `runtime_api` / `runtime_threads` 模块化

**目标：** 单文件 ~5k 行 → 按域拆分，降低 Zagens 新端点冲突。  
**优先级：** 建议 **先于** A1.5、大块 A2 修改同一文件（见 §3.4）；Issue 顺序见 §14.1（R-003 提前）。

| 步骤 | 建议模块布局 | 原文件 |
|------|--------------|--------|
| A4.1 | `runtime_api/mod.rs` + `router.rs`（`build_router`） | `runtime_api.rs` |
| A4.2 | `runtime_api/auth.rs`（token 中间件）— **先拆** | 同上 |
| A4.3 | `runtime_api/stream.rs`（`/v1/stream` SSE） | 同上 |
| A4.4 | `runtime_api/threads.rs`（CRUD、turns、approval） | 同上 |
| A4.5 | `runtime_api/mcp.rs`, `tasks.rs`, `automations.rs`, … | 同上 |
| A4.6 | `runtime_threads/{manager, persist, events}.rs` | `runtime_threads.rs` |

**原则：** 每 PR 拆 1–2 模块，**零行为变更**；每步 `cargo test -p deepseek-tui`。

**验收：**

- [x] `runtime_api` 根 `mod.rs` < 800 行；**每个**子模块 < 1000 行（code-organization 软上限）
- [x] 全量测试通过；Zagens 冒烟（建 thread、发消息、停）— 契约测 `sidecar_contract_full_lifecycle`（CI）

---

### 7.5 A5 — 可测试性与最小回放（**P2 编码硬门槛**）

> **已完成（G2）：** `crates/tui` 已有 **`lib.rs`** + A5.5 回放 fixture。**P2 编码** 门槛见 §12.2（A5.1 + A5.5 + A+.4 已签收）。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| A5.0 | **0.5d 审计：** `capacity.rs` 现有 `#[ignore]` 测、mock LLM 注入点；决定复用 vs 新建 | 笔记 → PR 描述 |
| A5.1 | `deepseek-tui` 增加 **`lib.rs`** + `[[bin]]` 依赖 lib | `crates/tui/Cargo.toml`, `src/lib.rs` |
| A5.2 | `Engine` 使用 `Arc<dyn LlmClient>`（或等价注入点） | `core/engine.rs`, `llm_client` |
| A5.3 | 取消 `#[ignore]` 的 mock LLM 集成测 | `core/engine/tests.rs` |
| A5.4 | compaction **pin 属性测试**（关键消息不可删） | `compaction.rs` tests |
| A5.5 | **事件序列录制/回放** fixture（10–20 步 turn） | `crates/tui/tests/` |

**验收：**

- [ ] CI 新增/启用集成测不 flaky
- [ ] 重构 `turn_loop` 前至少有 1 条回放回归

---

### 7.6 A6 — 安全诚实化（可与 A1 并行）

| 步骤 | 实施内容 |
|------|----------|
| A6.1 | 文档：沙箱能力矩阵（macOS / Linux / Windows） |
| A6.2 | TUI + Zagens 设置页：非 macOS 显示「降级模式」 |
| A6.3 | （可选 backlog）Linux Landlock / Windows 限制实现立项 |

**阶段 A / P2 期间桌面仅允许（D10 Feature freeze）：**

- Sidecar 监督、多窗口、已有 API 的 **bugfix**（不新增 runtime 语义）
- 契约冒烟（建 thread / SSE / stop / 审批）
- **运行时行为修复**（如模型输出重复、SSE/流式去重、审批路由）— **允许**，不视为 GAP
- **禁止：** xterm、diff2html、新 `/v1` 路由、大改版 `App.tsx`、CRAFT 桌面大屏
- **不受 freeze 约束：** P0 安全漏洞、sidecar 崩溃、**数据丢失**类 bugfix（见 §10.6）

---

## 8. 阶段 A+ — 融合就绪（契约层，A 中后期）

**层级：L2。** 在 A1–A3 有初版后并行推进；**A+ 完成线（§12.2）达标后** 方可启动 **P2 编码**（见 §1.6）。**不是** 阶段 F 的前置（F 在 P2 后）。

### 8.1 目标

让 Zagens 与任意 L3 壳 **只依赖文档化契约** 即可安全迭代 UI，而不触 Engine  internals。

### 8.2 实施步骤

| 步骤 | 任务 | 产出 |
|------|------|------|
| A+.1 | `API_DESIGN.md` 正式发布 **稳定事件子集 v1** + `event_schema_version` | 契约 SSOT |
| A+.2 | `map_compat_stream_event` 与 v1 子集 **100% 对齐** 并加契约单测 | `runtime_api.rs` |
| A+.3 | Zagens `client.ts` / `streamNormalize.ts` **仅解析 v1**；未知事件忽略 | `web-ui` |
| A+.4 | **Sidecar 契约测**（CI 可选）：随机端口、`DEEPSEEK_RUNTIME_TOKEN` 注入、固定 3 步 `health → create thread → stream → stop`、超时与进程清理 | `scripts/` 或 `crates/tui/tests/` |
| A+.4b | `RuntimeEventRecord` 与 `event_schema_version` **代码落地**（非仅文档） | `runtime_threads.rs`, `API_DESIGN.md` |
| A+.5 | `runtime_proxy` 路径覆盖 `/v1/*` 与 `/health` 回归 | `desktop` |
| A+.6 | 变更流程：**破坏性事件** 必须 bump schema version + CHANGELOG | 流程 |
| A+.7 | **审批契约回归：** `approval.required` SSE + `POST .../resolve-approval` 与桌面多窗口路由；单测覆盖「挂起→resolve→继续 turn」（非同步 auto-approve 误杀） | `runtime_api.rs`, `runtime_threads.rs`, `multi-window-plan` |

> **说明：** HTTP 审批端点已存在（`resolve-approval`）；A+.7 重点是 **语义一致** 与 Zagens 多窗，而非新造 API。对外事件仍以 **compat 子集 + schema version** 为准（D6）。

### 8.3 验收（§12.2 — 启动 P2 编码的门槛）

- [x] v1 事件子集与实现一致；契约测 CI 绿（G2 2026-05-23）
- [x] 桌面在 **不修改** `tui/src/core/engine.rs` 内部逻辑的前提下可完成一轮完整对话冒烟

---

## 9. 阶段 B — 差异化能力（分轨启动，非整阶段一把锁）

**比 UNDERLYING §2.1 更严**（见 §1.6）：UNDERLYING 允许打底 1+2 后开独有功能原型；本路线图对 **桌面 GAP** 与 **CRAFT 壳 UI** 更晚放行。

| 子轨 | 何时启动 | 内容 |
|------|----------|------|
| **B-L1** | **§12.3 P2 完成后**（不必等 F） | CRAFT：角色工具、黑板、turn_loop 闭环（`core` / `tools`） |
| **B-L2** | B-L1 中、随契约 bump | `craft.*` 事件进 API_DESIGN |
| **B-L3** | **§12.4 F 完成后** | Zagens `AgentPanel`、记忆地图 UI 等 **L3** |

**顺序：** B-L1 runtime → B-L2 契约 →（并行 F 桌面 GAP）→ B-L3 壳 UI。

### 9.1 B1 — CRAFT（多智能体可靠性）

详见 [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md)（含 **CRAFT 缩写 SSOT**：Closed-loop / Review / Agent / Fix-loop / Traceable → B1.1–B1.4）。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| B1.1 | **角色级工具白名单** 在 Engine 注册前裁剪 | `core/engine.rs`, `tools/subagent/` |
| B1.2 | 黑板 schema（findings / observed / blockers / verdict） | 新模块 `craft_board.rs` 或 `scratchpad` 扩展 |
| B1.3 | `agent_spawn` 注入黑板快照 | `tools/subagent/mod.rs` |
| B1.4 | Verifier 结构化输出 → Implementer 闭环 | `turn_loop.rs`, prompts |
| B1.5 | HTTP 事件暴露 `craft.*` 子集（**bump 契约 v1.1**） | `runtime_api`, `API_DESIGN.md` |
| B1.6 | （**B-L3**，F 后）Zagens `AgentPanel` 对接 `craft.*` — **B1.5 合并后** | `web-ui` |

**验收：**

- [x] explore/review 角色无法调用写盘工具（自动化测试）
- [x] 一次失败验收能程序化触发修复轮（手测 2026-05-24，G2 §10）

---

### 9.2 B2 — 话题记忆地图（Topic Memory Graph）

> **代码对齐（2026-05-24 实施）：** B2.1/B2.5/B-L3 ✅ — [B2_INJECTION_ARBITRATION.md](./adr/B2_INJECTION_ARBITRATION.md)、`TopicMemoryPanel`、`scripts/topic-memory-eval.ps1`。Runtime：`deepseek-topic-memory` + `tui/topic_memory.rs`。

| 步骤 | 实施内容 | 状态 |
|------|----------|------|
| B2.1 | **注入仲裁规则** 文档化（工具结果 > 黑板 > 图摘要） | **✅** [B2_INJECTION_ARBITRATION.md](./adr/B2_INJECTION_ARBITRATION.md) |
| B2.2 | 选型：Rust 对等实现（已定案） | **✅** `crates/topic-memory/` |
| B2.3 | 按当前 turn **k-hop 子图** 检索注入 | **✅** `retrieve_for_query` + `inject_interval` |
| B2.4 | 隐私：图路径 `~/.deepseek/topic-memory/`；opt-in 默认关 | **✅** |
| B2.5 | 轻量指标：`TopicMemoryMetrics` 落盘 + 重复澄清/完成率评测 | **✅** `eval_report` + `scripts/topic-memory-eval.ps1` |
| B2.6 | **B-L3** Zagens 记忆地图可视化面板 | **✅** `TopicMemoryPanel` + `GET /v1/topic-memory` |

---

### 9.3 B3 — TUI 体验（非阻塞）

| 步骤 | 内容 | 状态 |
|------|------|------|
| B3.1 | `main.rs` CLI 拆分到 `cli/` 子模块 | **✅** — `main.rs` ~350 行；`cli/tests.rs` |
| B3.2 | `tui-core` 评估：**接入** 或 **删除** | **✅ 定案 legacy** — 保留 snapshot 测，不接入生产链 |
| B3.3 | 背压策略：broadcast 满时 delta 合并策略 | **✅** — `event_coalesce.rs` + SSE `Lagged` 回补 |

---

## 10. 阶段 F — 桌面融合（**P2 完成后解冻**，D10）

**层级：L3。** 在 **A+A+ + P2** 完成后，才恢复 Zagens GAP 扩张；此前仅维护 + 冒烟（§10.6）。

### 10.1 与「并行轨道」的差异（v1.1 调整）

| v1.0 | v1.1（当前） |
|------|--------------|
| 桌面 GAP 与 A 长期并行 | **F 高峰在 P2 之后**；A 期间仅维护 + 冒烟 |
| 桌面可抢先做 xterm/diff | xterm/diff/导出等 **排在 F**，底层稳定后再做 |

### 10.2 依赖关系

```
A ──► A+ ──► P2 ──► F 解冻
              │
              ├──► B-L1 CRAFT（runtime，可与 F 并行）
              │
F 完成 ────────┴──► B-L3（AgentPanel、记忆地图 UI）
```

### 10.3 实施步骤（TUI_DS_PICK_GAP）

| 波次 | 任务 | 说明 |
|------|------|------|
| **F0** | **验证** `RoutingPanel` → `start_turn` 全链路透传 | 可能已部分完成（见 `TUI_DS_PICK_GAP.md`）；以验收为主，非大开发 |
| **F1a** | xterm.js Shell 实时输出 | ✅ `TerminalCard` + 增量 `tool.progress` |
| **F1b** | diff2html（edit_file / apply_patch） | ✅ `DiffCard`；运行中 diff 预览 |
| **F2** | 导出会话 JSON、资源管理器打开工作区 | ✅ `export_*_json` + `open_in_shell` |
| **F3** | 快捷键 / a11y | ✅ Skip link、Escape 停生成、focus-visible、aria 区域标签、`prefers-reduced-motion`；**G2 §8 手测已签**（2026-05-24） |
| **F4** | 内联编辑历史消息 | **✅** `POST .../edit-last-turn` + `MessageBubble`（2026-05-24）；TUI 多消息回退仍强于桌面 |

### 10.4 壳层 **禁止**（与 P2 一致）

- Spawn `app-server`；WebView 内实现 turn；`desktop` 直接依赖 `engine.rs`
- 为赶进度在 Tauri 内复制 `turn_loop` 逻辑

### 10.5 融合完成线

- [x] GAP 表 F0–F3 关闭或记入暂缓（F3 §8 + **§12.4 #2** 手测签收 2026-05-24）
- [x] 同一 `deepseek-tui` 二进制：TUI 与 Zagens 跑 **同一** stop/审批/长跑语义（抽样）— [G2 §9](./adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md)、[CODE_REVIEW_2026-05-24.md](../../deliverables/CODE_REVIEW_2026-05-24.md)

### 10.6 桌面 GAP 冻结（还债窗口，D9）

**自 2026-05-21 起至 §12.3 P2 完成线达标（2026-05-24 代码侧已达标，见 D10 解冻记录）：**

| 允许 | 禁止 |
|------|------|
| Sidecar 监督、崩溃恢复、多窗口 bugfix | 新 `/v1` 路由（除 P2 必需且评审） |
| 契约冒烟、与 A+ 对齐的 client 修复 | xterm、diff2html、导出 JSON、新面板 |
| 安全/依赖 patch；**数据丢失**类 bugfix | 内联编辑历史、定时自动化 UI 等新语义 |
| **运行时/流式 bugfix**（重复输出、SSE 去重、sidecar 契约冒烟修复） | 在 WebView 复制 Engine / turn 逻辑 |
| 文案/i18n/样式 | 大改版 `App.tsx` 布局/新面板 |

**理由：** Phase 1 已证明 harness 可行；每多一项桌面 **新语义/GAP**，都在 **加大 P2 迁移面**，最终可能 **无法** 把 Engine 收出 `tui`。

---

## 11. 阶段 P2 — 还 DESKTOP_IMPLEMENTATION_PLAN「三级火箭」Phase 2 技术债（主债）

> **命名：** 本文 **阶段 P2** = PLAN 中 **Engine → `deepseek-core`**。≠ PLAN 文档里的「Phase 2: Tauri 脚手架」。Issue 请用 **`P2-debt`** 前缀。  
> **原路线图 v1.0「阶段 C」** = 可选「Engine 上移 core / 废弃 app-server 实验栈」，**已废止并并入本节 P2**（非 DESKTOP_IMPLEMENTATION_PLAN 的 UI Phase 2）。

**触发：** Phase 1 harness 已验收（§4.3）。**禁止**用「继续堆桌面功能」代替 P2。

**目标：** `Engine` / `turn_loop` / `session` 沉入 **`deepseek-core`**；`runtime_api` + `RuntimeThreadManager` 为 **适配层**；sidecar 仍为 `deepseek-tui`，`/v1/*` **无破坏性变更**。

**不做：** 桌面 sidecar 换 `app-server`；为桌面单独维护第二套 turn 管道。

### 11.0 迁移边界 ADR（P2.0 — 开编码前须签收，对应 R-013）

| 归属 | P2 迁入 `deepseek-core` | 留在 `deepseek-tui` | P2 后 / 后续 |
|------|-------------------------|---------------------|--------------|
| **Engine / turn_loop** | ✅ 主迁对象 | 薄 re-export 即可 | — |
| **session / compaction 核心** | ✅ 与 Engine 同迁 | TUI 展示适配 | — |
| **runtime_api / HTTP** | 类型依赖 | ✅ 路由与 SSE 映射 | — |
| **RuntimeThreadManager** | 调用 core | ✅ 持久化 JSONL、广播 | — |
| **工具实现** | 仅 trait 边界 | ✅ `tui/src/tools/*` 主体 | 可选：注册层下沉 `deepseek-tools` |
| **MCP 池** | — | ✅ `tui/src/mcp.rs` | — |
| **双持久化** | — | — | **Backlog：** `StateStore`（CLI/app-server）vs `runtime_threads` JSONL 统一策略，**不在 P2 首 PR** |
| **Crate 边界** | **定案：扩展 `deepseek-core`** | 不新建 `runtime` crate，除非 ADR 修订 | — |
| **PR0 Spike (R-013)** | ✅ 已完成 (2026-05-22) | 见 [adr/P2_MIGRATION_SPIKE.md](adr/P2_MIGRATION_SPIKE.md) | trait 边界、EngineConfig 落点、ThreadManager→core 草图已确认可行 |
| **PR1 类型平移** | ✅ (2026-05-23) | `deepseek-core` 子模块 + tui re-export | `chat`/`models`/`turn`/`compaction`/`capacity`/`workshop`/`LlmClient` 等；§12.3 以 PR6 + G3 为准 |

**`core::Runtime::handle_thread`：** 迁移后作为与 `RuntimeThreadManager` **共享的 core 入口**（委托真实 turn），用于消除 `queued` 占位漂移；**禁止**为桌面再开一条独立 Message 管道（与 §13.2 一致）。

**P2 职责划分（`deepseek-core` 已 ~1700 行 `Runtime`，非空接收）：**

```
┌─────────────────────────────────────────────────────────────┐
│  deepseek-core（P2 后 L1 主逻辑）                             │
│  · Engine + turn_loop + session/compaction 核心              │
│  · Turn 执行、工具编排回调（trait → tui 工具注册表）            │
│  · 与 app-server 共用的 handle_thread 真 turn（可选渐进）       │
├─────────────────────────────────────────────────────────────┤
│  deepseek-tui 适配层                                         │
│  · RuntimeThreadManager：thread 生命周期、JSONL、broadcast    │
│  · runtime_api：HTTP/SSE、map_compat_stream_event、鉴权        │
│  · tui/src/tools/*、mcp.rs：工具实现（P2 不整体搬迁）          │
└─────────────────────────────────────────────────────────────┘
```

**合并风险：** `StateStore`（app-server）与 `runtime_threads` JSONL 仍 **backlog**；P2 PR 禁止顺带统一持久化。

### 11.1 前置门（§1.6）

| 门 | 要求 |
|----|------|
| G1 | §12.1 A 完成线（仅 L1） |
| G2 | §12.2 A+ 完成线 + **A5.1 + A5.5 + A+.4** |
| G3 | §11.0 ADR 签收（R-013） |
| G4 | 桌面 GAP 冻结（§10.6） |

### 11.2 绞杀者迁移切片（禁止单 PR 搬完全部 Engine）

| PR | 内容 | 验收 |
|----|------|------|
| PR0 | **Spike（≤3d）：** trait 边界 + `EngineConfig` 落点 + ThreadManager→core 调用草图；更新 §11.0 ADR | 设计笔记；无行为变更 |
| PR1 | 类型/配置/`EngineConfig` 平移；`deepseek-tui` 依赖 `deepseek-core` | `cargo test -p deepseek-core` |
| PR2 | `turn_loop` + `session` 迁入 core；回放测绿 | A5.5 fixture |
| PR3 | `RuntimeThreadManager::start_turn` 委托 core；契约测绿 | A+.4 |
| PR4 | 删/薄 `tui/src/core/engine.rs`（目标 **< 300 行** 包装） | §12.3 #1 |
| PR5 | `handle_thread(Message)` 委托；多窗口并行 turn 回归 | `multi-window-plan` 冒烟 |

### 11.3 Phase 3′（库层统一，非换 sidecar）

| 步骤 | 内容 |
|------|------|
| P3′.1 | **保留** `deepseek-tui serve --http` 与 `runtime_api` 为唯一生产 HTTP |
| P3′.2 | `app-server` 标 deprecated；**不扩展**；stdio 实验可委托 core |
| P3′.3 | README / PLAN：废止「桌面改 app-server binary」 |

### 11.4 与 §12.3 关系

本节实施步骤的验收以 **§12.3** 为准；达标后解除 §10.6 桌面 Feature freeze。

---

## 12. 里程碑与验收清单

### 12.1 阶段 A「底层完成线」（仅 L1）

> **2026-05-25 对齐：** **#1/#2/#3/#4/#5 ✅** — A1 live `ToolCell` 同构（#1）随 follow-up 升级为生产级（`check_live_history_isomorphism` + `drift_count`），A2/A3 已签收 [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)。

历史门控（保留作上下文）：未达标时不得将 A+ 标为完成、不得启动 P2 编码。**当前 §12.1 全部 5 项 ✅**；后续门控（A+/P2/G3/D10/F/B §12.5 #1/#2）均已闭合。

| # | 标准 | 验证方式 | 状态 |
|---|------|----------|------|
| 1 | 长跑内存/阻塞可接受 + live transcript 同构 | §12.6 / A1.6 + `App::check_live_history_isomorphism` | **✅** A1-MVP+（R-015 RSS + `-Gate`）+ A1 follow-up 生产级 drift counter（[A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status，2026-05-25） |
| 2 | Turn 结构化可观测（**内部** EngineEvent / 日志） | A2 抽查 | **✅** [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) 2026-05-25 |
| 3 | 错误分类用户可见差异 | A3 单测 | **✅** 同上；A3.4 边缘 UI → backlog |
| 4 | `runtime_api` 模块化 + 测试网 | CI；子模块 < 1000 行 | **✅** |
| 5 | A5.4 pin 测（**推荐**）；A5.1 lib **非** A 硬项 | **P2 编码硬门槛** 见 §12.2 G2 | **✅**（G2 已签收 A5.1 + A5.5 + A+.4） |

**达标后：** **仅** 启动/加速 **A+**（可与 A 尾部重叠）。**不** 启动 P2 编码、**不** 启动 F 高峰。

### 12.2 阶段 A+「融合就绪线」（仅 L2）

未达标：**不得** 启动 P2 编码。

| # | 标准 |
|---|------|
| 1 | `API_DESIGN.md` 稳定事件子集 v1 + **`event_schema_version` 代码字段** |
| 2 | Sidecar 契约测 CI 绿（§8.2 A+.4） |
| 3 | 桌面冒烟：不改 `engine.rs` 逻辑即可全链路对话 |
| 4 | 审批：A+.7 回归通过（挂起→resolve→继续） |

**达标后：** 启动 **P2** 编码（§11，含 PR0 spike）。仍 **不** 启动 F 高峰。

**P2 完成至 F 完成期间：** 契约测 **每周** CI 不得红（§13.1 升级硬门禁）。

### 12.3 阶段 P2「技术债完成线」（解除 D10 桌面 Feature freeze）

| # | 标准 |
|---|------|
| 1 | Engine/turn_loop 位于 `deepseek-core`；`tui/src/core/engine.rs` **< 300 行**（薄包装） |
| 2 | `runtime_threads` 通过 core 跑 turn；契约测 + 多窗口抽样冒烟 |
| 3 | sidecar 仍为 `deepseek-tui`；`/v1/*` 无破坏性变更 |

**达标后：** 解冻 **F**；可启动 **B-L1**（CRAFT runtime）。**B-L3** 与 **§12.5** 全量 B 验收在 **F 后**。

### 12.4 阶段 F「桌面融合线」

| # | 标准 |
|---|------|
| 1 | TUI_DS_PICK_GAP 的 F0–F2 完成或明确暂缓 |
| 2 | TUI 与 Zagens 对同一 thread 的 stop/审批/长跑行为一致（抽样） |

### 12.5 阶段 B 完成线

> **2026-05-25 对齐：** #1 CRAFT ✅；#2 记忆地图 **✅ MVP**（与 §9.2 一致）；#3 GAP 表 **◐**（产品 polish，非门控）。

| # | 标准 | 状态 |
|---|------|------|
| 1 | CRAFT 角色工具 + 黑板 + 一轮闭环验收 | **✅** G2 §10（2026-05-24） |
| 2 | 记忆地图注入可开关、可评测 | **✅ MVP** — k-hop 注入 + 设置开关；[B2_INJECTION_ARBITRATION.md](./adr/B2_INJECTION_ARBITRATION.md)；`topic-memory-eval.ps1`；`TopicMemoryPanel` + `/v1/topic-memory` |
| 3 | Zagens GAP 表 P0–P2 项关闭或明确暂缓 | **◐** 定时自动化 UI 暂缓；TUI 斜杠深度、子代理联动 polish 等 |

### 12.6 量化指标（写入 CI/脚本；首版基准见 R-015）

| 门槛 | 验收规则 |
|------|----------|
| **A1.6 长跑** | 场景：N=50 轮，至少 1 次 tool 输出 ≥ 1MB。**首版数值** 已写入 [adr/RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md)（RSS 中位数 **29 MB** @ `8b1538a`）。**A1 过渡门禁：** `-Gate` 不得比基线劣化 >10% RSS。 |
| **A+.4 契约测** | `GET /health` → `POST /v1/threads` → `POST .../messages`（SSE，含 `turn.completed` 或 `done`）→ `POST .../stop`；随机端口；超时 120s；进程清理 |
| **P2 薄包装** | `tui/src/core/engine.rs` < 300 行 |
| **A4 模块** | `runtime_api` 根 < 800 行；每个子模块 < 1000 行 |

### 12.7 发布节奏建议

| 类型 | 建议 |
|------|------|
| workspace `deepseek-tui` | 随 A 项合并发 patch |
| Zagens SemVer | 独立版本；A2/B1 可视用户面发 minor |
| Breaking HTTP 事件 | 仅 bump `event_schema_version`，旧客户端忽略未知字段 |

---

## 13. 风险与不做清单

### 13.1 风险登记

| 风险 | 影响 | 缓解 |
|------|------|------|
| 双轨继续开发 | 返工、协议分裂 | D4 冻结 app-server；PR 审查 |
| 桌面抢跑底层 | 契约反复撕扯 | P1 + 阶段 F 门控 |
| 无测试重构 turn_loop | 回归 | A5 先于大改 |
| CRAFT + 记忆地图同时注入 | 上下文污染 | B2.1 仲裁规则 |
| 非 macOS 沙箱误解 | 安全事故 | A6 诚实 UX |
| P2 前扩桌面 GAP | Engine 焊死 tui、无法抽取 | D10 冻结 + §10.6 |
| 误做原 PLAN P3 换 sidecar | 鉴权/契约全毁 | D11：仅库层统一 |
| 「融合」误解为合并代码库 | 架构腐化 | §1.3 + P2 |
| **P2 大爆炸单 PR** | 全量回归难查 | §11.2 绞杀者切片 |
| **多窗口 + P2** | `RuntimeThreadManager` 回归 | PR5 多窗口冒烟 |
| **双持久化未规划** | StateStore vs JSONL 分裂 | §11.0 backlog |
| **80% L1 无桌面反馈** | 契约漂移 | A+ 后至 F 完成：**每周** 契约测 CI 绿（§12.2） |
| **文档版本漂移** | 执行争议 | 关联文档随路线图同步（v1.6+ 统一引用 v1.6） |
| **P2 并入非空 core** | 职责冲突、双 Runtime | §11.0 职责图 + PR0 spike；禁止 PR 顺带统一 StateStore |

### 13.2 明确不做（当前周期）

- 为桌面 **单独** 维护第二套 turn 管道（`handle_thread` 仅作 core 共享入口，见 §11.0）
- Zagens 双 sidecar；WebView 内嵌 Engine
- **P2 完成前** 冲刺 xterm/diff/新面板/CRAFT 桌面大屏
- 无测试的一刀切重写 `ui.rs` / `main.rs`
- 未定义仲裁前全量注入记忆地图
- 对外承诺 Linux/Windows 与 macOS 同等沙箱（未实现前）

---

## 14. Issue 拆分模板

复制到 GitHub Issue：

```markdown
## 摘要
[一句话]

## 阶段
- [ ] 0 治理  [ ] A 底层  [ ] A+ 契约  [ ] P2 技术债  [ ] F 桌面（解冻后）  [ ] B 差异化

## 路线图步骤
例如：A2.3 — HTTP 稳定事件子集文档

## 涉及文件
- crates/tui/src/...

## 验收标准
- [ ] cargo test -p deepseek-tui
- [ ] （若 HTTP）docs/tech/API_DESIGN.md 已更新
- [ ] （若桌面）Zagens 冒烟：建会话 / 发消息 / SSE

## 依赖
- 阻塞：#xxx
- 不阻塞 app-server / core Runtime
```

### 14.1 建议首批 Issue 列表（底层优先）

| ID | 标题 | 阶段 | 层级 | 备注 |
|----|------|------|------|------|
| R-001 | 文档：RUNTIME_ARCHITECTURE + 底层/壳分离共识 | 0 | — | ✅ |
| R-002 | `project_rules.md`：禁止 app-server 扩展 + 禁止 WebView Engine | 0 | — | ✅ |
| R-008 | 术语对照表 + 桌面 Feature freeze 公告 + [§6.2 步骤 0.8](#62-实施步骤) PR 规则 | 0 | — | **✅** §1.5 + `project_rules.md` + D10 解冻 ADR |
| **R-003** | **A4.1–A4.3：** 拆分 runtime_api router + auth + stream | **A** | L1 | **优先** |
| R-004 | A2.1–A2.2：turn_summary 事件（L1） | A | L1 | **✅** + [A2_A3_SIGNOFF](./adr/A2_A3_SIGNOFF.md) |
| R-005 | A1.5：消息历史 VecDeque / drain | A | L1 |
| R-006 | A5.0–A5.1：审计 + deepseek-tui lib target | A | L1 |
| R-007 | A3.1–A3.2：error_taxonomy golden tests + 分类扩展 | A | L1 | **✅** + [A2_A3_SIGNOFF](./adr/A2_A3_SIGNOFF.md) |
| R-009 | A+.1–A+.4b：契约 v1 + sidecar 契约测 | A+ | L2 |
| R-014 | A+.7：审批挂起→resolve 契约回归 + 多窗口 | A+ | L2 |
| R-015 | A1.6 / §12.6：长跑基准脚本 + `RUNTIME_BASELINE.md` 首值 | A | L1 |
| R-013 | **P2.0 ADR + PR0 spike**（§11.0、§11.2） | P2 | — | 可与 R-006 并行 |
| R-012 | P2 PR1–PR5：绞杀者迁移 | P2 | 阻塞 R-006、R-009、R-013 签收 |
| R-010 | F0：**验证** Routing 全链路透传 | F | L3（P2 后） |
| R-011 | F1a/F1b：xterm + diff2html | F | L3（P2 后） |

> **建议开工顺序：** R-003 → R-006/R-015 → R-007/R-004 → R-009/R-014 → R-013 → R-012。  
> **R-007（旧）桌面路由** 已改为 **F0**，避免底层未稳时改 UI 语义。

---

## 15. 关联文档

| 文档 | 关系 |
|------|------|
| [UNDERLYING_ITERATION_REFERENCE.md](../tui/UNDERLYING_ITERATION_REFERENCE.md) | 阶段 A/B 技术方向明细 |
| [DESKTOP_IMPLEMENTATION_PLAN.md](../desktop/DESKTOP_IMPLEMENTATION_PLAN.md) | 桌面 Phase 1 选型（路线 B） |
| [RUNTIME_ARCHITECTURE.md](./RUNTIME_ARCHITECTURE.md) | 三层模型、crate 依赖、双持久化/双通道附图 |
| [OPENCODE_AGENT_CORE_BENCHMARK.md](./OPENCODE_AGENT_CORE_BENCHMARK.md) | OpenCode agent 核心对标、P0–P4 借鉴路线图 |
| [API_DESIGN.md](./API_DESIGN.md) | HTTP/SSE + Tauri IPC 契约 SSOT |
| [adr/RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md) | A1.6 长跑基准数值（R-015 填入） |
| [TUI_DS_PICK_GAP.md](../desktop/TUI_DS_PICK_GAP.md) | 桌面差距排期 |
| [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md) | B1 CRAFT |
| [CODE_REVIEW_2025-05-11.md](../CODE_REVIEW_2025-05-11.md) | 历史审核与 Step 5–6 |
| [multi-window-plan.md](../desktop/multi-window-plan.md) | 多窗口已交付；并行 turn 假设 |

---

## 16. 文档审核记录

| 轮次 | 日期 | 结论 | 处理 |
|------|------|------|------|
| **一** | 2026-05-21 | 战略正确；§8b 重复、门控矛盾、缺 P2 切片 | → **v1.3** |
| **二** | 2026-05-21（Zagens） | C1/C2 **已在 v1.3 修复**（无 §8b）；M1 P2′、M2 AGENTS、M3 阶段 C、L1 F1、L3 B 时序 | → **v1.4** |
| **三** | 2026-05-21 | 代码快照 §3.4；A1/A3/A5 对齐现状；§11.0 职责图+PR0；§12.6 基准流程；freeze 与维护性修复；R-003/014/015；§6.2 步骤 0.8 | → **v1.5** |
| **四** | 2026-05-21 | 定稿前抛光：§6.2.0.8 引用统一、§12.1 #5 与 G2 门控一致、§1.4 v2.0 重排策略、`RUNTIME_BASELINE.md` 占位 | → **v1.6** |
| **五** | 2026-05-22（实施后） | 代码≠路线图完成：A4/P2 局部；D10 仍有效；修正 CHANGELOG「含 router.rs」表述 | → **§17** |

**定稿：** 2026-05-21 维护者签收 §4.2（D4–D7、D9）→ **v2.0-final**。R-015 首版基准仍 **并行**（填 [RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md)）。**实施状态** 见 **§17**（2026-05-22）。

---

## 17. Zagens 实施后审核（2026-05-22）

> **结论（2026-05-25）：** **门控链已闭合**（Phase 1 → A+ → P2 → D10 → F → B §12.5 #1/#2）。**§12.1 全 5 项 ✅** — A2/A3 签收 [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)、A1 live ToolCell 同构生产级化（2026-05-25 follow-up）。**§12.5 #3** 与 GAP 表为产品 polish。  
> **归档总结：** [IMPLEMENTATION_SUMMARY_2026-05-24.md](./adr/IMPLEMENTATION_SUMMARY_2026-05-24.md)（§2–§5 随本文同步）

### 17.1 里程碑对照

| 阶段 | 路线图要求 | 审核结论（2026-05-24 对齐） |
|------|------------|------------------------------|
| **0 治理** | 文档 SSOT、D4–D9、freeze 规则 | **✅**（D10 已解除；0.8 → `project_rules.md`） |
| **A L1** | A1–A5、A4 模块化 | **✅** — #1/#2/#3 ✅（A1 live 同构生产级化 2026-05-25）；A4/A5/A6 ✅ |
| **A+ L2** | 契约 v1、sidecar 契约测、审批回归 | **✅ 自动化**（G2 2026-05-23） |
| **P2** | Engine→core、engine.rs <300 行 | **✅ L2 终态 + PR6** — `engine.rs` ~193 行；整 struct 留 tui（backlog） |
| **F** | P2 后 GAP F0–F3 + 行为抽样 | **✅** F0–F4 代码 + §12.4 手测（2026-05-24） |
| **B** | CRAFT + 记忆地图 + GAP | **🟡** — **#1/#2 ✅**（B-L1 + B2 MVP）；**#3 GAP ◐** |

### 17.2 已交付（可勾选 issue）

| ID | 交付物 | 证据 |
|----|--------|------|
| R-001 | `RUNTIME_ARCHITECTURE.md` | 存在且引用 v2.0-final |
| R-003（部分） | `runtime_api` → `mod` + `auth` + `stream` + `router` + `threads` + `sessions` + `tasks` | 目录 `crates/tui/src/runtime_api/` |
| R-009（部分） | A+.4 sidecar 契约测 + CI 显式跑 | `sidecar_contract_full_lifecycle`、`.github/workflows/ci.yml` |
| R-006（部分） | `deepseek-tui` lib target | `crates/tui/src/lib.rs` |
| R-013 | P2 PR0 spike | `docs/tech/adr/P2_MIGRATION_SPIKE.md` |
| R-015（部分） | 长跑脚本 + ADR dry-run p99 + `-Gate` + 1MB fixture | `scripts/runtime-longrun-baseline.ps1`（`-DryRun` / `-Gate`）、`adr/RUNTIME_BASELINE.md`（RSS @ `ab4c3c4`） |
| — | P2 PR6 turn loop L2 | `streaming_phase` / `tool_phase` / `tool_parser` / `capacity_policy` → core；tui `tool_plans_exec` + `host_impl/` — [P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md](./adr/P2_PR6_TURN_LOOP_L2_MIGRATION_PLAN.md) |
| — | P2 PR2 局部 | `deepseek-core::{session,working_set,project_context,approval,cycle::CycleBriefing,engine}` + tui re-export |
| — | P2 PR1 类型/`LlmClient` 入 core | `crates/core/src/{chat,models,turn,...}` + tui re-export |
| — | Zagens 生产路径 | Phase 1 harness、v0.4.3 流式去重、多窗口（CHANGELOG） |
| — | B-L1 CRAFT | 黑板 API、`craft.*` SSE、fix-loop 提示、AgentPanel — [craft-implementation-issues.md](../../craft-implementation-issues.md)；手测 [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./adr/G2_PR5_MANUAL_SMOKE_CHECKLIST.md) §10（2026-05-24） |
| — | B2 记忆地图 MVP | [B2_INJECTION_ARBITRATION.md](./adr/B2_INJECTION_ARBITRATION.md)；`TopicMemoryPanel`；`scripts/topic-memory-eval.ps1`；`GET /v1/topic-memory` |
| R-004 / R-007 | A2 turn_summary + A3 error_taxonomy | [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) §12.1 #2/#3（2026-05-25） |

### 17.3 阻塞与债务（2026-05-25 代码对齐）

**§12.5 / 路线图余项 — 仍需写代码或签收：**

| 优先 | 项 | 状态 |
|------|-----|------|
| **P1** | **A1** live `ToolCell` 与 `session.messages` 全同构 | **✅ 闭合（2026-05-25）** — `App::check_live_history_isomorphism` + `history_isomorphism::record_drift` / `drift_count`；[A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status |
| **P2** | **B3** `main.rs` 进一步瘦身（`cli/` 已拆；`main.rs` ~350 行仍含大量 `mod` 声明） | **🟡** 可选 polish |
| **P2** | **GAP 表** 定时自动化 UI、TUI `/` 深度、通知触发等 | **◐** 见 [TUI_DS_PICK_GAP.md](../desktop/TUI_DS_PICK_GAP.md) |
| **backlog** | 整包 `Engine` → core；StateStore/JSONL 统一；`streaming_phase` 子模块；A6.3 Landlock/Windows 沙箱 | 见 `docs/tech/adr/BACKLOG_*.md` |

**已完成（以代码为准）：** A4/A5/A6；**A2/A3 §12.1 #2/#3** [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)；A1.3 `events_since_async`；A1.6 R-015；P2 PR0–PR6 + G3；D10 解冻；F0–F4；B-L1 CRAFT；B2 MVP；B3.3；G2 §11。

### 17.4 维护者签收待办

- [x] 维护者签收 §17 审核结论（**门控已闭合**；G2/G3 2026-05-23；2026-05-25 文档对齐）
- [x] §12.1 **#2/#3** A2/A3 — [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)（2026-05-25）
- [x] §11.0 ADR **G3** 正式签收 — [P2_G3_ENGINE_L2_SIGNOFF.md](./adr/P2_G3_ENGINE_L2_SIGNOFF.md)
- [x] D10 解冻评审（实施记录）→ [P2_D10_UNFREEZE_RECORD.md](./adr/P2_D10_UNFREEZE_RECORD.md)；阶段 F F0–F3 已落地，F3 §8 手测已签（2026-05-24）
- [x] 维护者正式签收 D10 解除 freeze — [P2_D10_UNFREEZE_RECORD.md](./adr/P2_D10_UNFREEZE_RECORD.md) §4（Jason，2026-05-24）

### 17.5 代码对齐快照（2026-05-25）

对照仓库源码的 **门控 vs 余项** 一览（详细命令见 [IMPLEMENTATION_SUMMARY_2026-05-24.md](./adr/IMPLEMENTATION_SUMMARY_2026-05-24.md) §6）。

| 门控 / 阶段 | 代码事实 | 路线图状态 |
|-------------|----------|------------|
| Phase 1 harness | sidecar + `/v1/*` + 真 turn | **✅** |
| A+ §12.2 | `sidecar_contract_full_lifecycle` in CI；`event_schema_version` | **✅** |
| P2 §12.3 / M-series | `handle_deepseek_turn` + phases in core；`Engine` struct + op loop in core（tui ~130 行 newtype shim） | **✅**（M8 2026-05-26） |
| D10 / F §12.4 | F0–F4 落地；G2 §8–§9 手测 | **✅** |
| B-L1 §12.5 #1 | 黑板、`craft.*`、角色白名单、fix-loop | **✅** |
| B2 §12.5 #2 | `topic-memory` + 注入 + `TopicMemoryPanel` + B2.1/B2.5 | **✅ MVP** |
| A §12.1 #2/#3 | A2 可观测 + A3 错误分类 | **✅** [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) |
| A §12.1 全量 | #1 live ToolCell 同构 — `check_live_history_isomorphism` 升级生产级 | **✅**（2026-05-25 闭合） |
| B3 | `cli/` + `event_coalesce`；`tui-core` legacy 定案 | **✅** / **🟡** 可选 `main.rs` 再瘦身 |
| GAP 表 §12.5 #3 | 定时自动化 UI ⏸；TUI `/` 深度 ◐ | **◐ 产品债** |

**仍未实施或待签收（按优先序）：**

1. **B3 可选** — `main.rs` 进一步瘦身（`cli/commands/legacy.rs` 已承载命令体）
2. **Backlog ADR** — ~~整包 `Engine`→core~~ **✅ M8 2026-05-26** [BACKLOG_ENGINE_STRUCT_IN_CORE.md](./adr/BACKLOG_ENGINE_STRUCT_IN_CORE.md)；StateStore/JSONL 统一；可选 `streaming_phase` 拆子模块；A6.3 Landlock/Windows 沙箱实现
3. **GAP（非门控）** — 定时自动化 UI；通知前端触发；TUI 斜杠命令对标；高级线程操作

**已闭合（勿重复立项）：** **A2/A3** [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md)；**A1 follow-up（live `ToolCell` 同构生产级，2026-05-25）** [A1_PERSIST_BLOCKING_AUDIT.md](./adr/A1_PERSIST_BLOCKING_AUDIT.md) §Status；B2.1/B-L3/B2.5；A1.3 `events_since_async`；B3.3；G2 §11。

---

## 修订记录

| 日期 | 版本 | 说明 |
|------|------|------|
| 2026-05-21 | v1.0 | 初版：基于架构评估与现有 PLAN/REFERENCE 合并为可执行路线图 |
| 2026-05-21 | v1.1 | 底层优先 + L1/L2/L3 分离；新增 A+ 契约层、阶段 F 桌面融合（后置） |
| 2026-05-21 | v1.2 | Phase 1 harness 验收；P2 提升为主债；桌面 GAP 冻结 |
| 2026-05-21 | v1.3 | 第一轮审核修订：删 §8b、门控 A→A+→P2→F、§12 去重、P2 ADR/绞杀者表 |
| 2026-05-21 | v1.4 | 第二轮（Zagens）审核：确认 §8b 已删；统一 **P2** 命名；0.5/ D5 阶段 C 释义；B 分 B-L1/B-L3；F1a/F1b；执行顺序指引 |
| 2026-05-21 | v1.5 | 第三轮审核：§3.4 体量快照；A1/A3/A5 与代码对齐；P2 core↔ThreadManager；§12.6+R-015 基准；A+.7 审批；freeze PR 规则；Issue 优先级 |
| 2026-05-21 | v1.6 | 定稿前抛光：步骤 0.8 引用、§12.1 A5.1 门控、§1.4 章节重排策略、`adr/RUNTIME_BASELINE.md` 占位 |
| 2026-05-21 | v2.0-final | 维护者签收 §4.2（D4–D7、D9）；路线图定稿，可按 §14.1 开工 |
| 2026-05-24 | v2.0-final+align | §17.5 代码对齐；§3.3–3.4 / §7 / §9–11 / §12 状态刷新；F4/B2/B3 与代码一致 |
| 2026-05-24 | — | §9.1 B1 验收勾选；§17.1 B-L1 CRAFT 手测签收（G2 §10） |
| 2026-05-22 | v2.0-final+audit | §17 Zagens 实施后审核；§3.4 体量快照更新；D10 freeze 仍有效 |
| 2026-05-25 | v2.0-final+align2 | §12.1/§12.5/§17 与代码二次对齐；B2/B-L3/A1.3/events_since 状态统一；文首门控表述更新 |
| 2026-05-25 | — | [A2_A3_SIGNOFF.md](./adr/A2_A3_SIGNOFF.md) §12.1 #2/#3 维护者签收 |
