# 新会话交接 — D16 Phase E 维护性拆分

> **最后更新：** 2026-05-27  
> **HEAD：** `0d843c1` · 分支 `master` · **无 `origin` remote**（本地 commit 未 push）

---

## 给新窗口的第一句话（复制即用）

```
继续 D16 Phase E（Zagens runtime 维护性拆分）。请先读：
1. docs/tech/adr/SESSION_HANDOFF_D16_PHASE_E.md（本文）
2. docs/tech/adr/D16_PHASE_E_MAINTAINABILITY.md

现状：E2/E3/E5 已 Landed；E1-c6/E1-d2/E1-a8 ✅；E1-b phase 6 WIP（`task_manager/` 已拆）；下一项 E1-b 继续或 E4 可选。
完整 tools/ 迁 adapters 需阶段 2 ToolContext host ADR，勿贸然整包迁移。

规则：不要 commit/push 除非我明确要求；中文回复；最小 diff；每步跑 check-openapi-contract + 相关 cargo test。
```

---

## 1. 项目与目标

**产品：** Zagens（`crates/desktop/`）— 专有桌面 Agent harness；embedded runtime MIT 见 `third-party/deepseek-tui/LICENSE`。

**D16 目标：** 拆分过大 crate/模块以提升可维护性；**不改变** `/v1/*` HTTP 契约、Turn 执行路径、Desktop 不 path-depend runtime 的红线。

**权威 ADR：** [`D16_PHASE_E_MAINTAINABILITY.md`](./D16_PHASE_E_MAINTAINABILITY.md)  
**架构 SSOT：** [`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md)  
**构建/命令：** [`AGENTS.md`](../../../AGENTS.md)

---

## 2. D16 子项完成度

| 子项 | 状态 | 验收要点 |
|------|------|----------|
| **E2** SubAgent 拆文件 | ✅ Landed | `subagent/mod.rs` ~82 行；108 测试 |
| **E3** App.tsx hooks | ✅ Landed | `App.tsx` ~776 行；`AppShell` + 13 hook/模块 |
| **E5** OpenAPI contract CI | ✅ Landed | CI：OpenAPI JSON + TS diff；golden path 已有 |
| **E1-a** adapters tools | 🟡 阶段 1 完成 | a3–**a8** ✅（`shell/`/`file/`/`web_run/` 模块内拆分） |
| **E1-b** orchestrator | 🟡 Partial | monitor/engine_load/task_port + **task_manager/** 模块内拆分 |
| **E1-c** runtime-api | 🟡 Partial | phase 1–5 + **c6** ✅ task schemas 已迁 runtime-api |
| **E1-d** server 瘦身 | 🟡 Partial | bootstrap + **d2** ✅ re-export/文档对齐；`lib.rs` 62 行 |
| **E4** client.ts 拆分 | ⏸ 可选 | 未做 |

**D16 可不全部完成即发布 Zagens** — E2/E3/E5 已可视为独立 Landed。

---

## 3. Git 状态

| 项 | 值 |
|----|-----|
| 分支 | `master` |
| HEAD | `948341e` |
| 工作区 | **未 commit** — E1-a8 `skills/install/` 拆分（见 §9） |
| Remote | **未配置 `origin`** — push 需用户先 `git remote add origin <url>` |

### 近期 commit（新 → 旧）

| Commit | 说明 |
|--------|------|
| `948341e` | **E1-d2** re-export/文档 + **E1-a8** `shell/tools/` + **E1-b** `task_manager/` 拆分 |
| `223eb1e` | **E1-a8** shell/file/web_run 拆分 + **E1-c6** task schemas → runtime-api |
| `0d843c1` | handoff + **E1-a7** — `skills/install` → `network_gate` |
| `6fdc011` | **E5** — OpenAPI/TS CI + regenerate spec |
| `24c9754` | **E1-a6** — `network_gate`（fetch_url/web_run/web_search + SSRF） |
| `04a4599` | **E1-a5** — `workspace_walk` / `arg_repair` → adapters |
| `7115442` | **E1-a4** — ToolTaskHost / ToolAutomationHost |
| `39b966f` | **E1-a3** + E1-c phase 5 StreamTurnRequest 去重 |
| `e8373a1` | **E1-d** — `runtime_serve/http.rs` |
| `223521a`…`cff6c03` | E1-c phase 1–4 — runtime-api crate |
| `8b1ea8a`…`859ceb2` | E1-b — orchestrator 迁入 |

---

## 4. Crate 布局与依赖

```text
crates/
├── runtime-api/           deepseek-runtime-api
│   └── OpenAPI paths/schemas, auth, health, cors, ApiError, wire types, task wire types
├── runtime-orchestrator/  deepseek-runtime-orchestrator
│   └── RuntimeThreadManager, monitor, engine_load, task_port, pricing
├── runtime-adapters/      deepseek-runtime-adapters
│   └── mcp, network_policy, persist, snapshot, scratchpad_gates
│   └── tools/: diff_format, schema_sanitize, path, host, workspace_walk,
│              arg_repair, network_gate
└── runtime-server/        deepseek-runtime-server (bin: deepseek-runtime)
    └── runtime_api/ handlers, router (bearer 中间件在此)
    └── runtime_serve/http.rs — run_http_server DI
    └── runtime_threads/ — engine_spawn, monitor_host, engine_host (sidecar host)
    └── tools/ — 全部 ToolSpec 实现（循环依赖，暂留）

runtime-server → runtime-api, runtime-orchestrator, runtime-adapters
deepseek-desktop → 不 path-depend 任何 runtime crate（红线）
```

### 关键路径

| 区域 | 路径 |
|------|------|
| Task wire 类型 | `crates/runtime-api/src/task.rs` |
| Sidecar HTTP handlers | `crates/runtime-server/src/runtime_api/` |
| Sidecar OpenAPI | `crates/runtime-server/src/runtime_api/openapi.rs`（re-export runtime-api） |
| Sidecar 路由 + bearer | `crates/runtime-server/src/runtime_api/router.rs` |
| HTTP 装配 | `crates/runtime-server/src/runtime_serve/http.rs` |
| Orchestrator 核心 | `crates/runtime-orchestrator/src/runtime_threads/` |
| Tool host 端口 | `crates/runtime-adapters/src/tools/host.rs` |
| Tool host impl | `crates/runtime-server/src/tools/host_impl.rs` |
| OpenAPI 检入契约 | `docs/tech/openapi/zagens-runtime-v1.openapi.json` |
| TS 生成类型 | `crates/desktop/web-ui/src/api/generated/runtime-api.ts` |
| CHANGELOG | `[Unreleased]` Architecture 段 |

---

## 5. E1 架构要点（必读）

### runtime-adapters/tools 已有能力

- **Host 端口：** `RuntimeToolHostWire`, `ToolTaskHost`, `ToolAutomationHost`, `ToolShellEnvHost`, `ToolProgressEmit`
- **纯 helper：** `workspace_walk`, `arg_repair`, `diff_format`, `schema_sanitize`, `path`
- **network_gate：** `check_host_policy`, `check_url_policy`, `check_host_with_policy`, `host_policy_decision`, `is_restricted_ip`, `is_http_url`
- **server 侧：** `tools/mod.rs` re-export 保持 `crate::tools::*` 路径

### RuntimeToolServices 当前字段

```text
wire: RuntimeToolHostWire
task_host: Option<Arc<dyn ToolTaskHost>>
automation_host: Option<Arc<dyn ToolAutomationHost>>
shell_env: Option<Arc<dyn ToolShellEnvHost>>
shell_manager: Option<SharedShellManager>
```

### Axum 0.8 状态类型（E1-c/d 陷阱）

1. `/v1/*` 先 `.route_layer(from_fn_with_state(...))` → 仍为 `Router<RuntimeApiState>`
2. `compose_router` 合并 health/probe 后 **一次** `.with_state(state)` → `Router<()>`
3. **不要**在传入 `compose_router` 前对 `api_routes` 单独 `with_state`

### E1 阻塞：tools/ 完整迁移

`ToolSpec::execute(&ToolContext)` 依赖 server 侧 engine/runtime_threads → **与 adapters 循环依赖**。  
阶段 2 需：adapters 定义 `ToolExecutionHost` trait → `ToolContext` 瘦身 → 先迁纯 I/O 工具。

---

## 6. 推荐下一步（优先级）

### 立即可做（阶段 1 — 低风险）

| ID | 任务 | 说明 |
|----|------|------|
| ~~**E1-a8**~~ | ~~shell/file/web_run + `shell/tools/`~~ | ✅ |
| **E1-b** | `task_manager/` 迁 orchestrator（可选） | 模块内拆分已完成；整包迁移需评估 host 依赖 |

### 需设计（阶段 2）

- 写 `E1-a_TOOL_CONTEXT_HOST.md` ADR
- `ToolContext` trait 化后再迁 `glob_files`、`truncate` 等纯 I/O 工具

### 可选 / 并行

- **E4：** `web-ui/src/api/client.ts` 按 domain 拆分
- **Push + CI：** 配置 `origin` 后 push，验证 OpenAPI/TS/sidecar contract

### 建议 **不要** 做

- 一口气迁完 `tools/` 到 adapters
- 无 OpenAPI 更新地改 `/v1/*` 形状
- 误 commit `workspacePaths.ts`（与 D16 无关）

---

## 7. 验证命令（每 PR 必跑）

```powershell
# 契约（E5 — 必须无 diff）
.\scripts\check-openapi-contract.ps1

# E1-a 回归
cargo test -p deepseek-runtime-adapters -p deepseek-runtime-server --lib tools

# E1-c/d 回归
cargo test -p deepseek-runtime-api -p deepseek-runtime-server --lib runtime_api

# E1-b 回归
cargo test -p deepseek-runtime-orchestrator -p deepseek-runtime-server --lib runtime_threads

# Sidecar 契约（CI golden path）
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract

# E1-a7 回归
cargo test -p deepseek-runtime-server --lib skills::install

# Web UI（若动 TS）
cd crates/desktop/web-ui && npm run build
```

**已知：** `turn_completed_event_includes_turn_summary` 偶有 flaky；磁盘满时 `cargo clean`。

---

## 8. 不变量（D15 延续）

| 规则 | 说明 |
|------|------|
| HTTP 契约 | 不改 `/v1/*`（除非刻意版本变更 + OpenAPI + TS 再生） |
| Desktop 边界 | 不 path-depend runtime-server/core |
| Turn 路径 | `RuntimeThreadManager` → `core::Engine` |
| 文件体量 | soft cap ~1000 行（`.cursor/rules/code-organization.mdc`） |
| CHANGELOG | notable 变更记 `[Unreleased]`，与 commit 同 PR |
| 品牌 | 对外总结以 **Zagens** 为主 |

---

## 9. 勿误 commit

| 文件 | 说明 |
|------|------|
| `crates/desktop/web-ui/src/lib/workspacePaths.ts` | 与 D16 无关，未跟踪 |

---

## 10. 相关文档索引

| 文档 | 用途 |
|------|------|
| [`D16_PHASE_E_MAINTAINABILITY.md`](./D16_PHASE_E_MAINTAINABILITY.md) | E1–E5 完整方案与 DoD |
| [`D15_FINAL_ARCHITECTURE_CONVERGENCE.md`](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md) | 架构收敛前置 |
| [`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md) | OpenAPI + TS 生成纪律 |
| [`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md) | 运行时架构图与路径表 |
| [`CHANGELOG.md`](../../../CHANGELOG.md) | 已 Landed / WIP 条目 |
| [`AGENTS.md`](../../../AGENTS.md) | Agent 构建命令与 API quirks |

---

*维护：Land 新子项后更新 §2、§3、§6。*
