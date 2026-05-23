# DeepSeek-TUI / DS Pick 运行时演进实施路线图

> **版本：** v2.0-final（2026-05-21）  
> **状态：** **定稿 + 实施中** — §4.2 已签收；**§17** 为 DS Pick 实施后审核快照（2026-05-22）  
> **门控：** **未** 达 §12.3 P2 完成线 → D10 桌面 Feature freeze **仍有效**  
> **v1.6 要点：** 修正 §6.2 步骤 0.8 引用、§12.1 A5.1 门控表述、§1.4 定稿章节策略、`RUNTIME_BASELINE.md` 占位  
> **受众：** 维护者、Agent、桌面/TUI 开发  
> **产品节奏：** Phase 1 harness **已验收** → **A+A+ 打底** → **P2 还技术债**（Engine→core）→ **解冻桌面 GAP**；壳运 **始终分离**，**不**换 app-server sidecar。

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
17. [DS Pick 实施后审核（2026-05-22）](#17-ds-pick-实施后审核2026-05-22)

> **执行顺序阅读指引（与章节编号无关）：** §7 A → §8 A+ → §11 P2 → §10 F → §9 B（见 §1.6）。正文 §9–§11 按 **文档结构** 编号（B→F→P2），**勿按 9→10→11 数字顺序执行**。`v2.0-final` 可选将正文重排为 §9 P2 → §10 F → §11 B（见 §1.4）。

---

## 1. 目标与原则

### 1.1 总目标

在 **不推翻现有 sidecar 架构** 的前提下：

1. **先把底层打牢固**（Engine、会话、容量、可观测、错误、测试）——这是全产品的地基。  
2. **再通过稳定契约与桌面对齐**（HTTP/SSE + 少量 Tauri IPC），实现 TUI 与 DS Pick **同一底层、两种壳**。  
3. **长期保持分离：** 桌面壳（`crates/desktop`）与运行时（`deepseek-tui`）**进程与职责分离**；融合 = 消费契约，≠ 把 Engine 搬进 WebView 或 Tauri。  
4. **冻结** `app-server` 实验栈，避免在「未完成的第二运行时」上叠桌面或差异化功能。

### 1.2 八条原则

| # | 原则 |
|---|------|
| P1 | **底层优先：** 阶段 A 完成线未达标前，桌面只做 **维护/契约消费/冒烟**，不抢跑大型 UI 特性或新 runtime 语义 |
| P2 | **壳运分离：** DS Pick = Tauri 壳 + `deepseek-tui` sidecar；Agent turn **只在** sidecar 内执行 |
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
│  ├─ DS Pick: Tauri + React（工作区、布局、原生对话框）          │
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

### 3.1 生产路径（TUI + DS Pick + 绝大多数 CLI）

```
用户
  ├─ DS Pick WebView ──► Tauri (config/secrets/proxy) ──► deepseek-tui serve --http
  ├─ 终端 TUI ─────────► deepseek-tui (ratatui)
  └─ deepseek run/exec/serve/... ──► delegate_to_tui ──► deepseek-tui

deepseek-tui
  ├─ runtime_api.rs      HTTP /v1/* + SSE
  ├─ runtime_threads.rs  线程/回合/广播/持久化
  ├─ core/engine.rs      Agent turn（真 LLM + 工具）
  ├─ client/chat.rs      SSE 解析
  ├─ tui/src/tools/*     工具实现主体
  ├─ mcp.rs              MCP 池（实现层）
  └─ deepseek-tools      workspace 类型/trait（见 §3.3）
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
| core | ❌ | ❌ | 仅 app-server | **非产品主链**；**P2 后** 为 L1 主库 |
| state | ❌ | ❌ | thread 元数据 | 与 runtime_threads 并行 |
| tui-core | ❌ | ❌ | ❌ | 未接入 |

### 3.4 代码体量快照（2026-05-22 审核更新）

| 路径 | 约行数 | 路线图动作 | 实施状态 |
|------|--------|------------|----------|
| `crates/tui/src/runtime_api/mod.rs` | ~505 | **A4** 拆分 | **✅ 达标** — 域模块 + `tests.rs`；主文件保留 `run_http_server`、`ApiError`、CORS、`ThreadEventsQuery` |
| `crates/tui/src/runtime_threads/mod.rs` | ~275 | **A4.6** | **✅ 主文件达标** — 测试在 `tests.rs`（~2140）；`manager.rs` 待再切 |
| `crates/tui/src/runtime_threads/manager.rs` | ~2860 | **A4.6** | **待拆** — 超 code-org 软上限；按域再切 |
| `crates/core/src/session.rs` | ~183 | **P2 PR2** | **✅** — `Session`/`SessionUsage` + `working_set`/`project_context`/`ApprovalMode` |
| `crates/tui/src/core/engine.rs` | ~2174 | **P2 PR2** | **部分** — `turn_loop` 仍在此；待 Engine→core（PR3–PR4） |
| `crates/tui/src/core/engine.rs` | ~2174 | **P2** 薄包装目标 <300 行 | **未达标** — Engine 仍在 tui |
| `crates/core/src/lib.rs` | ~1710 | **P2** 并入 Engine | **部分** — 共享类型已入 core 子模块；`Runtime`/`Engine` 未迁 |
| `crates/tui/src/lib.rs` | 有 | **A5.1** | **✅** lib target 已存在 |

**已有能力（避免重复立项）：** `tools/large_output_router.rs`（大 tool 输出外部化/路由）、`core/capacity*.rs`（容量估算）。A1/A5 以 **扩展、落盘一致性与测试** 为主，见 §7.1、§7.5。

---

## 4. 战略决策（必读）

### 4.1 已采纳（2026-05，与 DESKTOP_IMPLEMENTATION_PLAN 一致）

| 决策 | 内容 |
|------|------|
| **D1** | DS Pick **Phase 1 = 路线 B**：sidecar 为 `deepseek-tui`，HTTP 面为 `runtime_api`，**不** 使用 `app-server` |
| **D2** | 桌面鉴权使用 `runtime_token`；**不** 使用 `app-server` 的全开放 CORS |
| **D3** | 差异化顺序：**CRAFT → 话题记忆地图**（见 UNDERLYING_ITERATION_REFERENCE §2.2） |
| **D8 Phase 1 收官** | **Harness 验证已通过**（2026-05） | 路线 B 证明 `deepseek-tui` 可承载桌面；不再为「能否做桌面」扩功能 |
| **D9 P2 技术债** | **A+A+ 后必做**（非可选 C） | Engine 上移 `deepseek-core`（DESKTOP_IMPLEMENTATION_PLAN Phase 2） |
| **D10 桌面功能冻结** | **P2 完成前** | 不扩大 DS Pick GAP；避免壳层特性固化后无法迁底层 |
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
| Phase 1 之后最忌什么？ | **继续扩 DS Pick GAP**，导致 P2 迁移面不可逆增大 |

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
| **F** 桌面融合 | **L3** | **P2 后** 4–8 周 | DS Pick GAP（xterm、diff…）— **冻结至 P2 完成** |
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
| 0.4 | 更新 `README.md` 中 `app-server` 描述为 **Experimental — not used by DS Pick** | 对外一致 | 文档 ✅ |
| 0.5 | 维护者确认 **D4**（冻结 `app-server`；**不**再开「阶段 C」— 已并入 **P2**）并记入 §4.2 签收栏 | 决策 | Tech lead |
| 0.6 | `project_rules.md` 增加可 grep 禁令：**禁止 app-server 扩展；禁止 WebView 内嵌 Engine** | 流程 | 工具链 ✅ |
| 0.7 | 签收 **D4/D7/D9** 与 §1.6 门控链；发布桌面 **Feature freeze** 公告 | 决策 | Tech lead |
| 0.8 | **PR 审查规则（freeze）：** 标签 `area:desktop` 的 PR 须说明是否符合 §10.6；超出 freeze 须标 `freeze-exception` + 维护者 ack | `CONTRIBUTING` 或 `project_rules.md` | Tech lead |

### 6.3 验收

- [ ] 新成员能回答：底层在哪？融合面是哪一层？桌面为何不合并进 tui？
- [ ] 无开放 PR 把 DS Pick 功能接到 `app-server` 或在 WebView 实现 turn
- [x] 运行时/桌面 **维护性** 修复可合并（例：模型输出重复、SSE 去重）— 不视为 GAP 扩张，见 §10.6

---

## 7. 阶段 A — 底层夯实（4–10 周，主战场）

**层级：L1。** 本阶段 **不依赖** 桌面新功能交付；TUI 与 sidecar 共用同一底层，TUI 可随 A 项一并验证。

对应 [UNDERLYING_ITERATION_REFERENCE.md](../tui/UNDERLYING_ITERATION_REFERENCE.md) 阶段 A 与 §3.1–3.8。

### 7.1 A1 — 会话与消息存储（优先级最高）

**问题：** 长跑会话 `api_messages`、工具结果、JSONL/SQLite 全量序列化导致内存与主线程阻塞；compaction 与 TUI transcript 可能分叉。

**现状：** 已有 `large_output_router`（Engine/工具链接线）、`capacity_memory` / `capacity.rs`；缺口在 **落盘同构、热/冷窗口、长跑基准**（§12.6、R-015）。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| A1.1 | 定义 **热窗口 / 冷区** 数据模型（近期消息 + pin vs 摘要/外部引用） | `core/session.rs`, `compaction.rs` |
| A1.2 | 超大 tool output **外部化**：在 `large_output_router` 上补 **持久化 ref + 消息同构**（非从零造 blob 层） | `large_output_router.rs`, `runtime_threads.rs`, `tools/*` |
| A1.3 | 落盘 **节流 + spawn_blocking**；crash-safe checkpoint 策略文档化 | `thread_store_sqlite.rs`, `session_store_sqlite.rs` |
| A1.4 | compaction/trim 后 **同构校验**：`session.messages`、TUI history、thread JSONL 一致 | `compaction.rs`, `tui/history.rs`, `runtime_threads.rs` |
| A1.5 | `trim_oldest_messages_to_budget`：`Vec::remove(0)` → `VecDeque` 或批量 `drain` | `core/engine.rs` |
| A1.6 | 增加 **长跑基准脚本**（见 §12.7 量化指标） | `scripts/` 或 `crates/tui` 集成测 |

**A1-MVP（可分阶段验收，避免 4–10 周全耗 A1）：**

| 子项 | 内容 | 验收 |
|------|------|------|
| A1-MVP.1 | **large_output_router** 产出 ref 与 session/JSONL **一致** + 消息内 ref | 单测 + 一轮长跑（R-015 脚本可选） |
| A1-MVP.2 | compaction **pin** 单测（与 A5.4 可合并） | pin 不被删 |
| A1-full | 热/冷窗口 + 落盘同构 + A1.6 全量基准 | §12.1 #1 |

**验收：**

- [ ] A1-MVP 或 A1-full 至少达成其一后，A 项 #1 可标部分通过
- [ ] compaction 后无「UI 有、引擎无」类 silent bug（有回归测）

---

### 7.2 A2 — Turn 级可观测与事件模型（L1，为 L2 铺路）

**问题：** 排障依赖零散 `status` 文案；后续桌面融合需要稳定事件模型（完整子集在 A+ 冻结）。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| A2.1 | 扩展 `EngineEvent` / `RuntimeEventRecord`：`turn_summary`（step_count, tool_names, end_reason） | `core/events.rs`, `runtime_threads.rs` |
| A2.2 | Turn 结束 emit 结构化事件；映射到 SSE compat 名 | `runtime_api.rs` `map_compat_stream_event` |
| A2.3 | 内部文档草案：拟议 **稳定事件子集 v1**（A+ 正式写入 API_DESIGN） | 文档草稿 |
| A2.4 | （移至 A+）桌面 `client.ts` 对齐子集 | 见 §8 |
| A2.5 | 结构化 tracing span：`turn_id`, `thread_id`, `step` | `logging.rs`, `turn_loop.rs` |

**验收：**

- [ ] 单条 turn 可从 **日志或内部 EngineEvent** 回答：几步、哪些工具、为何结束（SSE 子集对齐在 **A+**）

---

### 7.3 A3 — 失败路径与重试分类

**现状：** 已有 `ErrorCategory`、`ErrorEnvelope`（`error_taxonomy.rs`）；分散单测在 `engine/tests.rs` 等。本阶段 **扩展分类 + golden tests**，不另起 `ErrorClass` 命名。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| A3.1 | 为 `classify_error_message` 增加 **30+ golden/边界单测**（集中到 `error_taxonomy` 或 `tests/error_taxonomy.rs`） | `error_taxonomy.rs` |
| A3.2 | 扩展 `ErrorCategory`（含 `NetworkRetryable` 等）并与 `ErrorEnvelope` 对齐；`turn_loop` 只读分类结果 | `error_taxonomy.rs` |
| A3.3 | `turn_loop` 重试仅对可重试类；400/thinking 约束走修补或用户提示 | `core/engine/turn_loop.rs` |
| A3.4 | TUI 与 HTTP 错误 payload 统一：`code`, `category`（或 `class` 别名）, `message`, `hint` | `runtime_api.rs`, `tui` 错误 UI |

**验收：**

- [ ] 网络断连与 reasoning 400 用户可见文案不同
- [ ] 无「业务不可重试仍刷 3 次」的额度浪费（可测）

---

### 7.4 A4 — `runtime_api` / `runtime_threads` 模块化

**目标：** 单文件 ~5k 行 → 按域拆分，降低 DS Pick 新端点冲突。  
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

- [ ] `runtime_api.rs` 主文件 < 800 行；**每个**子模块 < 1000 行（code-organization 软上限）
- [ ] 全量测试通过；DS Pick 冒烟（建 thread、发消息、停）

---

### 7.5 A5 — 可测试性与最小回放（**P2 编码硬门槛**）

> **未开始：** 当前 `crates/tui` **无** `lib.rs`（仅 `[[bin]]`）。**开 P2 编码前** 须完成 A5.1 + A5.5（回放）+ A+.4（契约测）。

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
| A6.2 | TUI + DS Pick 设置页：非 macOS 显示「降级模式」 |
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

让 DS Pick 与任意 L3 壳 **只依赖文档化契约** 即可安全迭代 UI，而不触 Engine  internals。

### 8.2 实施步骤

| 步骤 | 任务 | 产出 |
|------|------|------|
| A+.1 | `API_DESIGN.md` 正式发布 **稳定事件子集 v1** + `event_schema_version` | 契约 SSOT |
| A+.2 | `map_compat_stream_event` 与 v1 子集 **100% 对齐** 并加契约单测 | `runtime_api.rs` |
| A+.3 | DS Pick `client.ts` / `streamNormalize.ts` **仅解析 v1**；未知事件忽略 | `web-ui` |
| A+.4 | **Sidecar 契约测**（CI 可选）：随机端口、`DEEPSEEK_RUNTIME_TOKEN` 注入、固定 3 步 `health → create thread → stream → stop`、超时与进程清理 | `scripts/` 或 `crates/tui/tests/` |
| A+.4b | `RuntimeEventRecord` 与 `event_schema_version` **代码落地**（非仅文档） | `runtime_threads.rs`, `API_DESIGN.md` |
| A+.5 | `runtime_proxy` 路径覆盖 `/v1/*` 与 `/health` 回归 | `desktop` |
| A+.6 | 变更流程：**破坏性事件** 必须 bump schema version + CHANGELOG | 流程 |
| A+.7 | **审批契约回归：** `approval.required` SSE + `POST .../resolve-approval` 与桌面多窗口路由；单测覆盖「挂起→resolve→继续 turn」（非同步 auto-approve 误杀） | `runtime_api.rs`, `runtime_threads.rs`, `multi-window-plan` |

> **说明：** HTTP 审批端点已存在（`resolve-approval`）；A+.7 重点是 **语义一致** 与 DS Pick 多窗，而非新造 API。对外事件仍以 **compat 子集 + schema version** 为准（D6）。

### 8.3 验收（§12.2 — 启动 P2 编码的门槛）

- [ ] v1 事件子集与实现一致；契约测 CI 绿
- [ ] 桌面在 **不修改** `tui/src/core/engine.rs` 内部逻辑的前提下可完成一轮完整对话冒烟

---

## 9. 阶段 B — 差异化能力（分轨启动，非整阶段一把锁）

**比 UNDERLYING §2.1 更严**（见 §1.6）：UNDERLYING 允许打底 1+2 后开独有功能原型；本路线图对 **桌面 GAP** 与 **CRAFT 壳 UI** 更晚放行。

| 子轨 | 何时启动 | 内容 |
|------|----------|------|
| **B-L1** | **§12.3 P2 完成后**（不必等 F） | CRAFT：角色工具、黑板、turn_loop 闭环（`core` / `tools`） |
| **B-L2** | B-L1 中、随契约 bump | `craft.*` 事件进 API_DESIGN |
| **B-L3** | **§12.4 F 完成后** | DS Pick `AgentPanel`、记忆地图 UI 等 **L3** |

**顺序：** B-L1 runtime → B-L2 契约 →（并行 F 桌面 GAP）→ B-L3 壳 UI。

### 9.1 B1 — CRAFT（多智能体可靠性）

详见 [agent-reliability-craft-plan.md](../agent-reliability-craft-plan.md)。

| 步骤 | 实施内容 | 主要文件 |
|------|----------|----------|
| B1.1 | **角色级工具白名单** 在 Engine 注册前裁剪 | `core/engine.rs`, `tools/subagent/` |
| B1.2 | 黑板 schema（findings / observed / blockers / verdict） | 新模块 `craft_board.rs` 或 `scratchpad` 扩展 |
| B1.3 | `agent_spawn` 注入黑板快照 | `tools/subagent/mod.rs` |
| B1.4 | Verifier 结构化输出 → Implementer 闭环 | `turn_loop.rs`, prompts |
| B1.5 | HTTP 事件暴露 `craft.*` 子集（**bump 契约 v1.1**） | `runtime_api`, `API_DESIGN.md` |
| B1.6 | （**B-L3**，F 后）DS Pick `AgentPanel` 对接 `craft.*` — **B1.5 合并后** | `web-ui` |

**验收：**

- [ ] explore/review 角色无法调用写盘工具（自动化测试）
- [ ] 一次失败验收能程序化触发修复轮

---

### 9.2 B2 — 话题记忆地图（Topic Memory Graph）

| 步骤 | 实施内容 | 说明 |
|------|----------|------|
| B2.1 | **注入仲裁规则** 文档化（工具结果 > 黑板 > 图摘要） | 先文档后代码 |
| B2.2 | 选型：**桌面 Web** 维护图 + runtime 拉取 Markdown 块；或 Rust 对等实现 | 见 UNDERLYING §2.3 |
| B2.3 | 按当前 turn **k-hop 子图** 检索注入，非每轮全图粘贴 | token 控制 |
| B2.4 | 隐私：不写入密钥；图存储路径与 workspace 绑定 | |
| B2.5 | 轻量指标：重复澄清轮次、长会话任务完成率 | 评测 |

---

### 9.3 B3 — TUI 体验（非阻塞）

| 步骤 | 内容 |
|------|------|
| B3.1 | `main.rs` CLI 拆分到 `cli/` 子模块 |
| B3.2 | `tui-core` 评估：**接入** 或 **删除** 避免死代码 |
| B3.3 | 背压策略：broadcast 满时 delta 合并策略 |

---

## 10. 阶段 F — 桌面融合（**P2 完成后解冻**，D10）

**层级：L3。** 在 **A+A+ + P2** 完成后，才恢复 DS Pick GAP 扩张；此前仅维护 + 冒烟（§10.6）。

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
| **F3** | 快捷键 / a11y | 🟡 Skip link、Escape 停生成、focus-visible、aria 区域标签、`prefers-reduced-motion` 扩展 |
| **F4** | 内联编辑历史消息 | **依赖 L1** 提供「改历史」runtime API 后再做 |

### 10.4 壳层 **禁止**（与 P2 一致）

- Spawn `app-server`；WebView 内实现 turn；`desktop` 直接依赖 `engine.rs`
- 为赶进度在 Tauri 内复制 `turn_loop` 逻辑

### 10.5 融合完成线

- [x] GAP 表 F0–F2 关闭或记入暂缓（F0 路由测、F1a/F1b/F2 已落地；F3 部分）
- [ ] 同一 `deepseek-tui` 二进制：TUI 与 DS Pick 跑 **同一** 长跑/审批/停止语义（抽样验收）

### 10.6 桌面 GAP 冻结（还债窗口，D9）

**自 2026-05-21 起至 §12.3 P2 完成线达标：**

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
| **PR1 类型平移（部分）** | 🟡 进行中 (2026-05-22) | `deepseek-core` 子模块 + tui re-export | `chat`/`models`/`turn`/`compaction`/`capacity`/`workshop` 等；**非** §12.3 完成 |

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

未达标：**不得** 将 A+ 标为完成；**不得** 启动 P2 编码（设计/spike 除外）。

| # | 标准 | 验证方式 |
|---|------|----------|
| 1 | 长跑内存/阻塞可接受（或 A1-MVP 通过） | §12.6 / A1.6 |
| 2 | Turn 结构化可观测（**内部** EngineEvent / 日志） | A2 抽查 |
| 3 | 错误分类用户可见差异 | A3 单测 |
| 4 | `runtime_api` 模块化 + 测试网 | CI；子模块 < 1000 行 |
| 5 | A5.4 pin 测（**推荐**）；A5.1 lib **非** A 硬项 | **P2 编码硬门槛** 见 §7.5、§12.2 G2（须 A5.1 + A5.5 + A+.4） |

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
| 2 | TUI 与 DS Pick 对同一 thread 的 stop/审批/长跑行为一致（抽样） |

### 12.5 阶段 B 完成线

| # | 标准 |
|---|------|
| 1 | CRAFT 角色工具 + 黑板 + 一轮闭环验收 |
| 2 | 记忆地图注入可开关、可评测 |
| 3 | DS Pick GAP 表 P0–P2 项关闭或明确暂缓 |

### 12.6 量化指标（写入 CI/脚本；首版基准见 R-015）

| 门槛 | 验收规则 |
|------|----------|
| **A1.6 长跑** | 场景：N=50 轮，至少 1 次 tool 输出 ≥ 1MB。**首版数值** 由 R-015 脚本在 `main`（或指定 tag）上跑 3 次取中位数，写入 [adr/RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md)（占位已建；填数后须 CHANGELOG）。**A1 过渡门禁：** 不得比基线 commit 劣化 >10% RSS 或落盘 p99。 |
| **A+.4 契约测** | `GET /health` → `POST /v1/threads` → `POST .../messages`（SSE，含 `turn.completed` 或 `done`）→ `POST .../stop`；随机端口；超时 120s；进程清理 |
| **P2 薄包装** | `tui/src/core/engine.rs` < 300 行 |
| **A4 模块** | `runtime_api` 根 < 800 行；每个子模块 < 1000 行 |

### 12.7 发布节奏建议

| 类型 | 建议 |
|------|------|
| workspace `deepseek-tui` | 随 A 项合并发 patch |
| DS Pick SemVer | 独立版本；A2/B1 可视用户面发 minor |
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
- DS Pick 双 sidecar；WebView 内嵌 Engine
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
- [ ] （若桌面）DS Pick 冒烟：建会话 / 发消息 / SSE

## 依赖
- 阻塞：#xxx
- 不阻塞 app-server / core Runtime
```

### 14.1 建议首批 Issue 列表（底层优先）

| ID | 标题 | 阶段 | 层级 | 备注 |
|----|------|------|------|------|
| R-001 | 文档：RUNTIME_ARCHITECTURE + 底层/壳分离共识 | 0 | — | ✅ |
| R-002 | `project_rules.md`：禁止 app-server 扩展 + 禁止 WebView Engine | 0 | — | ✅ |
| R-008 | 术语对照表 + 桌面 Feature freeze 公告 + [§6.2 步骤 0.8](#62-实施步骤) PR 规则 | 0 | — |
| **R-003** | **A4.1–A4.3：** 拆分 runtime_api router + auth + stream | **A** | L1 | **优先** |
| R-004 | A2.1–A2.2：turn_summary 事件（L1） | A | L1 |
| R-005 | A1.5：消息历史 VecDeque / drain | A | L1 |
| R-006 | A5.0–A5.1：审计 + deepseek-tui lib target | A | L1 |
| R-007 | A3.1–A3.2：error_taxonomy golden tests + 分类扩展 | A | L1 |
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
| [API_DESIGN.md](./API_DESIGN.md) | HTTP/SSE 契约 SSOT |
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
| **二** | 2026-05-21（DS Pick） | C1/C2 **已在 v1.3 修复**（无 §8b）；M1 P2′、M2 AGENTS、M3 阶段 C、L1 F1、L3 B 时序 | → **v1.4** |
| **三** | 2026-05-21 | 代码快照 §3.4；A1/A3/A5 对齐现状；§11.0 职责图+PR0；§12.6 基准流程；freeze 与维护性修复；R-003/014/015；§6.2 步骤 0.8 | → **v1.5** |
| **四** | 2026-05-21 | 定稿前抛光：§6.2.0.8 引用统一、§12.1 #5 与 G2 门控一致、§1.4 v2.0 重排策略、`RUNTIME_BASELINE.md` 占位 | → **v1.6** |
| **五** | 2026-05-22（实施后） | 代码≠路线图完成：A4/P2 局部；D10 仍有效；修正 CHANGELOG「含 router.rs」表述 | → **§17** |

**定稿：** 2026-05-21 维护者签收 §4.2（D4–D7、D9）→ **v2.0-final**。R-015 首版基准仍 **并行**（填 [RUNTIME_BASELINE.md](./adr/RUNTIME_BASELINE.md)）。**实施状态** 见 **§17**（2026-05-22）。

---

## 17. DS Pick 实施后审核（2026-05-22）

> **结论：** 路线图 **战略与门控仍然有效**；代码处于 **阶段 0 收尾 + A 部分项 + P2 PR0/PR1 局部**，**尚未** 满足 §12.1 A 完成线、§12.2 A+、§12.3 P2。桌面 **D10 freeze 不得解除**。

### 17.1 里程碑对照

| 阶段 | 路线图要求 | 审核结论 |
|------|------------|----------|
| **0 治理** | 文档 SSOT、D4–D9、freeze 规则 | **✅ 文档侧基本完成**（§6.2 0.1–0.4、0.6；§4.2 签收） |
| **A L1** | A1–A5、A4 模块化 | **🟡 部分** — A4 达标；**A3** golden 测已入 core；A1/A2/A5 未全量验收 |
| **A+ L2** | 契约 v1、sidecar 契约测、审批回归 | **🟡 自动化 ✅** — G2 门控（2026-05-23）；审批 UI 手测可复测（接线已合） |
| **P2** | Engine→core、engine.rs <300 行 | **🟡 L2 终态有条件达标** — G3 签收（2026-05-23）；`handle_thread(Message)` **委托 turn port**（app-server 单轮 LLM；无 port 仍 queued） |
| **F / B** | P2 后解冻 | **🟡 F0–F2 已落地** — F3 a11y 大部分（Composer tab 顺序 + skip link；手测待签） |

### 17.2 已交付（可勾选 issue）

| ID | 交付物 | 证据 |
|----|--------|------|
| R-001 | `RUNTIME_ARCHITECTURE.md` | 存在且引用 v2.0-final |
| R-003（部分） | `runtime_api` → `mod` + `auth` + `stream` + `router` + `threads` + `sessions` + `tasks` | 目录 `crates/tui/src/runtime_api/` |
| R-009（部分） | A+.4 sidecar 契约测 + CI 显式跑 | `sidecar_contract_full_lifecycle`、`.github/workflows/ci.yml` |
| R-006（部分） | `deepseek-tui` lib target | `crates/tui/src/lib.rs` |
| R-013 | P2 PR0 spike | `docs/tech/adr/P2_MIGRATION_SPIKE.md` |
| R-015（部分） | 长跑脚本 + ADR dry-run p99 + `-Gate` + 1MB fixture | `scripts/runtime-longrun-baseline.ps1`（`-DryRun` / `-Gate`）、`adr/RUNTIME_BASELINE.md`（RSS @ `ab4c3c4`） |
| — | P2 PR2 局部 | `deepseek-core::{session,working_set,project_context,approval,cycle::CycleBriefing,engine}` + tui re-export |
| — | P2 PR1 类型/`LlmClient` 入 core | `crates/core/src/{chat,models,turn,...}` + tui re-export |
| — | DS Pick 生产路径 | Phase 1 harness、v0.4.3 流式去重、多窗口（CHANGELOG） |

### 17.3 阻塞与债务（收尾优先序）

1. **编译/测试：** ✅ `cargo check --workspace` + `cargo test -p deepseek-tui --lib`（2026-05-22，2368 passed）。
2. **A4：** `runtime_api/mod.rs` <800 行已达标（2026-05-22）；后续仅增量提取时随改动维护域模块边界。
3. **A+ 契约：** ✅ G2 自动化（2026-05-23）；审批 UI 手测可复测。
4. **P2 绞杀者：** L2 终态 G3 签收；`handle_thread(Message)` 已委托 `ThreadMessageTurnPort`（app-server 单轮 LLM；生产 HTTP 仍 `RuntimeThreadManager`）。
5. **R-015 填数：** ✅ RSS 26.6 MB @ `ab4c3c4`；**`-Gate` 回归门 + 1.1 MB fixture** 已接线（全量重跑可选）。
6. **F0 路由：** ✅ `start_turn_applies_route_intent_routing_rule_to_model`（`route_intent` → `routing_rules.json` → model）。

### 17.4 维护者签收待办

- [x] 维护者签收 §17 审核结论（**实施中**，非 **已完成**；G2/G3 2026-05-23 更新）
- [x] §11.0 ADR **G3** 正式签收 — [P2_G3_ENGINE_L2_SIGNOFF.md](./adr/P2_G3_ENGINE_L2_SIGNOFF.md)
- [ ] D10 解冻评审 → 启动阶段 F（xterm / diff / a11y）

---

## 修订记录

| 日期 | 版本 | 说明 |
|------|------|------|
| 2026-05-21 | v1.0 | 初版：基于架构评估与现有 PLAN/REFERENCE 合并为可执行路线图 |
| 2026-05-21 | v1.1 | 底层优先 + L1/L2/L3 分离；新增 A+ 契约层、阶段 F 桌面融合（后置） |
| 2026-05-21 | v1.2 | Phase 1 harness 验收；P2 提升为主债；桌面 GAP 冻结 |
| 2026-05-21 | v1.3 | 第一轮审核修订：删 §8b、门控 A→A+→P2→F、§12 去重、P2 ADR/绞杀者表 |
| 2026-05-21 | v1.4 | 第二轮（DS Pick）审核：确认 §8b 已删；统一 **P2** 命名；0.5/ D5 阶段 C 释义；B 分 B-L1/B-L3；F1a/F1b；执行顺序指引 |
| 2026-05-21 | v1.5 | 第三轮审核：§3.4 体量快照；A1/A3/A5 与代码对齐；P2 core↔ThreadManager；§12.6+R-015 基准；A+.7 审批；freeze PR 规则；Issue 优先级 |
| 2026-05-21 | v1.6 | 定稿前抛光：步骤 0.8 引用、§12.1 A5.1 门控、§1.4 章节重排策略、`adr/RUNTIME_BASELINE.md` 占位 |
| 2026-05-21 | v2.0-final | 维护者签收 §4.2（D4–D7、D9）；路线图定稿，可按 §14.1 开工 |
| 2026-05-22 | v2.0-final+audit | §17 DS Pick 实施后审核；§3.4 体量快照更新；D10 freeze 仍有效 |
