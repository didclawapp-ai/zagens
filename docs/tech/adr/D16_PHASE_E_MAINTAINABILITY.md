# D16 — Phase E 维护性拆分方案（不阻塞发布）

> **类型：** 实施计划（维护性 / 工程效率）  
> **状态：** Proposed（2026-05-26）  
> **前置（已 Landed）：** [D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md)  
> **SSOT 架构图：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **与 D15 关系：** D15 确立 Desktop-only SSOT；**D16 不改变架构不变量**，仅拆分过大模块以提升可维护性

---

## 0. 边界

### 0.1 要解决的问题

D15 完成后，架构 SSOT 已确立，但以下 **单体文件 / 单 crate** 仍影响迭代效率：

| 热点 | 规模（约） | 影响 |
|------|-----------|------|
| `crates/runtime-server` | ~10 万行 Rust | 编译慢、职责耦合、改 HTTP 易误触 tools/MCP |
| `tools/subagent/mod.rs` | ~4000 行 | CRAFT 多 Agent 改动集中、review 困难 |
| `web-ui/src/App.tsx` | ~2566 行 | 连接 / 会话 / SSE / 审批 / Agent 状态全堆根组件 |

### 0.2 D16 做什么 / 不做什么

| 做 | 不做 |
|----|------|
| 物理拆分 crate / 模块 / hooks | 改 `/v1/*` HTTP 契约 |
| 文件体量降至 ~1000 行 soft cap 以内 | 改 Turn 执行路径（仍经 `RuntimeThreadManager` → `core::Engine`） |
| 测试随模块迁移 | Desktop 内嵌 runtime（去 HTTP hop） |
| 可选 OpenAPI contract test 进 CI（E5） | metrics / 多 sidecar / Capability Manifest（旧 D11–D14） |

### 0.3 建议优先级

| 顺序 | 项 | 理由 |
|------|-----|------|
| **E2** | SubAgent `mod.rs` 拆文件 | 单文件 4000 行，范围清晰，对 D15 零冲突 |
| **E3** | `App.tsx` 状态机下沉 | 桌面功能迭代高频触碰 |
| **E1** | 拆 `runtime-server` crate | 依赖链改动大，适合稳定期 |
| **E4–E5** | `client.ts` 拆分、OpenAPI CI | 可与 E3 并行 |

---

## 1. E1 — 拆分 `runtime-server` 大 crate

### 1.1 现状

**单 crate：** `crates/runtime-server/`（package `deepseek-runtime-server`，lib `deepseek_runtime`，bin `deepseek-runtime`）

`src/lib.rs` 注册 60+ 顶层模块，包括但不限于：

| 模块目录 / 文件 | 职责 |
|----------------|------|
| `runtime_api/` | Axum HTTP/SSE、`/v1/*` 路由 |
| `runtime_serve.rs` | Sidecar CLI 入口 |
| `runtime_threads/` | Thread/Turn 编排、事件广播 |
| `core/engine/` | Engine 适配层（Host 注入） |
| `tools/` | 全部工具实现（shell、file、subagent…） |
| `mcp/` | MCP 连接池 |
| `session_manager.rs` + `session_store_sqlite.rs` | Session 投影 |
| `thread_store_sqlite.rs` + `runtime_threads/persist.rs` | Thread/Event SSOT |
| `task_manager.rs` / `automation_manager.rs` | 后台任务 |
| `llm_client/` / `client/` | LLM 调用 |
| `compaction/` / `prompts.rs` / `symbol_index.rs` | 上下文与索引 |

**生产代码热点（>1000 行，2026-05-26 统计）：**

| 行数 | 路径 |
|------|------|
| 4003 | `src/tools/subagent/mod.rs` |
| 2371 | `src/tools/shell.rs` |
| 1991 | `src/tools/file.rs` |
| 1754 | `src/task_manager.rs` |
| 1638 | `src/tools/web_run.rs` |
| 1531 | `src/skills/install.rs` |
| 1494 | `src/client/chat.rs` |
| 1417 | `src/session_manager.rs` |
| 1324 | `src/tools/apply_patch.rs` |
| 1290 | `src/tools/office_write.rs` |
| 1276 | `src/tools/registry.rs` |
| 1248 | `src/command_safety.rs` |
| 1182 | `src/symbol_index.rs` |
| 1824 | `src/localization.rs` |

> 测试文件（如 `runtime_threads/tests.rs` ~2500 行）体积大但属测试代码；拆 crate 时随对应模块迁移，不单独列为生产债。

### 1.2 目标 crate 布局

```text
crates/
├── runtime-api/              # package: deepseek-runtime-api
│   └── HTTP/SSE 薄网关：auth、router、openapi、stream
│
├── runtime-orchestrator/     # package: deepseek-runtime-orchestrator
│   └── RuntimeThreadManager、task/automation wiring、审批挂起桥接
│
├── runtime-adapters/         # package: deepseek-runtime-adapters
│   └── tools/、mcp/、persist、session、llm_client、compaction…
│
└── runtime-server/           # package: deepseek-runtime-server（瘦壳）
    ├── lib: 仅 re-export + DI 组装（目标 <500 行）
    └── bin: deepseek-runtime（main 不变）
```

### 1.3 依赖方向（强制）

```text
runtime-server
  → runtime-api, runtime-orchestrator, runtime-adapters

runtime-api
  → runtime-orchestrator, deepseek-protocol

runtime-orchestrator
  → deepseek-core, runtime-adapters, deepseek-protocol

runtime-adapters
  → deepseek-core, deepseek-tools, deepseek-mcp, deepseek-config, …

deepseek-core
  → protocol, config（不依赖 runtime-api / adapters）
```

**红线（与 D15 一致）：**

- `deepseek-desktop` **仍不** path-depend 任何 runtime crate  
- `/v1/*` 路径与 OpenAPI **不变**  
- 拆 crate 前后 `export-runtime-openapi` diff 应为空（除非刻意改契约）

### 1.4 建议 PR 链（E1）

| PR | 内容 | 退出标准 |
|----|------|----------|
| E1-a | 新建 `runtime-adapters`，迁 `tools/` + `mcp/` + persist 相关 | `runtime-server` 通过 adapters 调用；测试绿 |
| E1-b | 新建 `runtime-orchestrator`，迁 `runtime_threads/` + `task_manager` | Turn 单路径不变 |
| E1-c | 新建 `runtime-api`，迁 `runtime_api/` + `runtime_serve` 路由层 | OpenAPI export 无 diff |
| E1-d | `runtime-server` 瘦身为 DI + main | `lib.rs` <500 行 |

**工期估算：** 3–5 周（可与产品功能并行，按 PR 渐进合并）。

---

## 2. E2 — SubAgent `mod.rs` 拆文件

### 2.1 现状

```
crates/runtime-server/src/tools/subagent/
├── mod.rs          ← ~4003 行（问题所在）
├── blackboard.rs   ← 已有
├── craft.rs        ← 已有
├── mailbox.rs      ← 已有
└── tests.rs        ← ~1521 行
```

`mod.rs` 内混杂：

| 块 | 代表类型 / 函数 |
|----|----------------|
| Runtime 核心 | `SubAgentRuntime`、`SubAgentManager`、`SubAgent` |
| Tool 实现（7+） | `AgentSpawnTool`、`AgentWaitTool`、`DelegateToAgentTool`、`AgentListTool`… |
| 执行逻辑 | `run_subagent()`、`run_subagent_task()`、`wait_for_agents()` |
| 解析 / 路由 | `parse_spawn_request()`、`subagent_flash_router()` |
| 持久化 | `PersistedSubAgent`、`write_json_atomic()`、`default_state_path()` |
| Prompt / 工具面 | `subagent_system_prompt()`、`subagent_allowed_tools()` |

### 2.2 目标目录

```text
subagent/
├── mod.rs              # re-export + ToolRegistry 注册（目标 <300 行）
├── runtime.rs          # SubAgentRuntime、SubAgentManager、SubAgent
├── spawn.rs            # AgentSpawnTool、parse_spawn_request
├── wait.rs             # AgentWaitTool、wait_for_result、wait_for_agents
├── delegate.rs         # DelegateToAgentTool
├── list_close.rs       # AgentListTool、AgentCloseTool、AgentCancelTool
├── assign_send.rs      # AgentAssignTool、AgentSendInputTool、AgentResumeTool
├── result.rs           # AgentResultTool
├── persist.rs          # PersistedSubAgent*、write_json_atomic、default_state_path
├── prompts.rs          # subagent_system_prompt、subagent_allowed_tools、build_subagent_system_prompt
├── router.rs           # subagent_flash_router、fallback_subagent_assignment_route
├── blackboard.rs       # 保持
├── craft.rs            # 保持
├── mailbox.rs          # 保持
└── tests.rs            # 保持（可按新模块加 #[path] 或 mod tests 子目录）
```

### 2.3 拆分原则

1. **每个 `*Tool` struct + `ToolHandler` impl 独立文件**（或按 spawn/wait/delegate 分组）  
2. **`mod.rs` 只做：** `pub mod` 声明、`pub use`、向 `ToolRegistryBuilder` 注册  
3. **共享类型**（`SubAgentCompletion`、`SpawnRequest` 等）放 `types.rs` 或留在 `runtime.rs`  
4. **单文件 soft cap ~800 行**；超限再拆  

### 2.4 建议 PR 链（E2）

| PR | 内容 |
|----|------|
| E2-a | 抽 `runtime.rs` + `persist.rs` + `prompts.rs`（无行为变更） |
| E2-b | 抽各 Tool 文件（spawn / wait / delegate / list_close / assign_send / result） |
| E2-c | 抽 `router.rs`；`mod.rs` 瘦身至 <300 行 |

**验证：** `cargo test -p deepseek-runtime-server subagent` 全绿；CRAFT blackboard / AgentPanel 手动冒烟。

**工期估算：** 1–2 周。

---

## 3. E3 — Web UI `App.tsx` 状态机下沉

### 3.1 现状

**文件：** `crates/desktop/web-ui/src/App.tsx`（~2566 行）

根组件 `App()` 直接 import **40+** API / lib 函数，并持有 **50+** `useState` / `useRef`，职责包括：

| 职责域 | 典型 state / ref | 当前职责 |
|--------|------------------|----------|
| **Runtime 连接** | `runtimeConn`、`runtimeSessionEstablished`、`retryConnectRef`、`runtimeProbeFailStreakRef` | `initRuntimeConfig`、Sidecar 就绪、`waitForRuntimeReady`、断线重连 |
| **会话 / 线程** | `sessions`、`activeSessionId`、`resumedThreadId`、`messages` | 侧栏列表、切换 session、`resumeSessionThread`、消息缓存 |
| **SSE 流式 Turn** | `streamingThreadIds`、`streamControllersRef`、`threadTurnRef`、`streamSessionRef` | `postStreamTurn` / poll SSE、`stopThreadTurn`、多窗口 `filterThreadStreamEvents` |
| **审批** | `approval`、`approvalBusy` | `ApprovalDialog`、`postResolveApproval` |
| **Agent / CRAFT** | `agentStates`、`pendingSpawnMetaRef` | SubAgent 面板、spawn 元数据 |
| **Composer 偏好** | `selectedModel`、`runMode`、`taskTypePreference`、`selectedWorkspace`… | 模型 / 模式 / 工作区 / 路由意图 |
| **UI 布局** | `sidebarCollapsed`、`activeInspector`、`panelPreview` | 面板折叠、预览、Inspector |

**相关问题文件（可选后续 E4）：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `api/client.ts` | ~1330 | 所有 REST/SSE 客户端逻辑 |
| `components/Composer.tsx` | ~1380 | 输入框与发送 |

### 3.2 目标结构

```text
web-ui/src/
├── App.tsx                    # 目标 <800 行：layout + hooks 组合
├── hooks/
│   ├── useRuntimeConnection.ts   # Sidecar 端口、ready、probe、重连
│   ├── useTurnSession.ts         # sessions、activeSession、messages、resume/switch
│   ├── useTurnStream.ts          # SSE 订阅、streaming、stop、事件 normalize
│   ├── useTurnApproval.ts        # approval 挂起 / resolve / busy
│   └── useAgentPanelState.ts     # agentStates、spawn meta、inspector 联动
├── api/                       # 可选 E4
│   ├── sessions.ts
│   ├── threads.ts
│   ├── stream.ts
│   └── client.ts              # 薄 re-export 或 deprecated 转发
└── components/                # 不变；App 不再直接调 40+ client 函数
```

### 3.3 Hook 职责划分

#### `useRuntimeConnection`

- 封装：`initRuntimeConfig`、`waitForRuntimeBootReady`、`probeRuntimeConnection`、`invalidateRuntimeBootReadyCache`
- 输出：`runtimeConn`、`retryConnect()`、`runtimeSessionEstablished`
- 消费方：App 壳、Sidebar 刷新 sessions 时机

#### `useTurnSession`

- 封装：`getSessions`、`getSessionDetail`、`resumeSessionThread`、`deleteSession`、`persistThreadSession`
- 输出：`sessions`、`activeSessionId`、`messages`、`selectSession()`、`refreshSessions()`
- 与 `sessionUiCache`、`rebuildMessagesFromThreadEvents` 协作

#### `useTurnStream`

- 封装：`postStreamTurn`、`pollThreadTurnEvents`、`startThreadTurn`、`stopThreadTurn`、`filterThreadStreamEvents`
- 输出：`streaming`、`sendMessage()`、`stopTurn()`、`threadTurnRef`
- 持有：`streamControllersRef`、`streamingThreadIds`

#### `useTurnApproval`

- 封装：`postResolveApproval`、审批策略 ref
- 输出：`approval`、`approvalBusy`、`resolveApproval()`、`clearApproval()`

#### `useAgentPanelState`

- 封装：`agentStates`、`pendingSpawnMetaRef`、与 `useInspectorUnread` / CRAFT blackboard 通知
- 输出：Agent 面板所需状态与 upsert  helpers

### 3.4 建议 PR 链（E3）

| PR | 内容 |
|----|------|
| E3-a | 抽 `useRuntimeConnection`；App 改用 hook |
| E3-b | 抽 `useTurnSession` + `useTurnStream`（耦合度高，可同 PR） |
| E3-c | 抽 `useTurnApproval` + `useAgentPanelState` |
| E3-d | App.tsx 瘦身验收（<800 行） |

**验证：**

```bash
cd crates/desktop/web-ui && npm run build
```

手动：冷启动 → 发消息 → 流式 → Stop → 审批 → 切换 session → Sidecar 重启后会话列表恢复。

**工期估算：** 2–3 周。

---

## 4. E4 / E5 — 可选跟进

### E4 — 拆分 `api/client.ts`

按 domain 拆文件，`client.ts` 保留 re-export 以免大规模改 import：

| 新文件 | 职责 |
|--------|------|
| `api/sessions.ts` | `getSessions`、`getSessionDetail`、`resumeSessionThread`、`deleteSession` |
| `api/threads.ts` | `getThreadDetail`、`patchThread`、`startThreadTurn`、`forkThreadAtUserMessage`… |
| `api/stream.ts` | `postStreamTurn`、`pollThreadTurnEvents`、`filterThreadStreamEvents` |
| `api/runtimeBoot.ts` | `initRuntimeConfig`、`waitForRuntimeReady`、`probeRuntimeConnection` |

### E5 — OpenAPI contract test（CI）

| 步骤 | 说明 |
|------|------|
| CI job | `export-runtime-openapi` → diff 检入的 `zagens-runtime-v1.openapi.json` |
| Smoke | create thread → start turn → SSE 事件 → resolve approval（可复用现有 test helper） |
| TS | `npm run generate:api-types` 与检入 `runtime-api.ts` 一致 |

---

## 5. 完成定义（D16 DoD）

满足以下条件可标记 D16 **Landed**（可分 E2/E3/E1 子项单独勾选）：

| 子项 | 验收 |
|------|------|
| **E2** | ✅ Landed（2026-05-26）— `subagent/mod.rs` ~82 行；`cargo test -p deepseek-runtime-server --lib tools::subagent` 108/108 |
| **E3** | ✅ Landed — 13 个 hook/模块 + `AppShell`；`App.tsx` **776 行**（<800）；`npm run build` ✅ |
| **E1** | 4 crate 依赖方向符合 §1.3；OpenAPI 无意外 diff |
| **E5** | CI 含 OpenAPI diff + 至少 1 条 golden path |

**不要求：** E1–E5 全部完成才发布 Zagens；可按子项独立合并。

---

## 6. 与 roadmap 对照

| 编号 | 主题 | 与 D16 关系 |
|------|------|-------------|
| D11 | Prometheus metrics | D16 完成后可选 |
| D12 | 每 workspace 一 sidecar | **不在** D16；需单独 ADR |
| D13 | Capability Manifest | D16 完成后可选 |
| D14 | MCP 池稳定性 | 可与 E1-adapters 部分重叠 |

---

## 7. 新会话推荐开场白

```text
执行 D16 Phase E 维护性拆分。请先读 docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md。
当前子项：[E2 SubAgent / E3 App hooks / E1 crate split]。从 PR-__ 开始。
不要 commit/push 除非我要求。中文回复。
```

---

*维护：子项 Landed 后在本文 §5 勾选，并在 [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) §10 追加链接。*
