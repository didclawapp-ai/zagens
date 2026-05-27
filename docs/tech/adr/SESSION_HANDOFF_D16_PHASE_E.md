# 新会话交接 — D16 Phase E 维护性拆分（2026-05-27 更新）

> **给新窗口的第一句话（复制即用）：**
>
> ```
> 继续 D16 Phase E。请先读 docs/tech/adr/SESSION_HANDOFF_D16_PHASE_E.md 和 D16_PHASE_E_MAINTAINABILITY.md。
> E2/E3/E5 已 Landed；E1-a 阶段 1 进行中（a3–a7）；完整 tools/ 迁 adapters 需 ToolContext host ADR（阶段 2）。
> 不要 commit/push 除非我要求。中文回复。
> ```

---

## 1. 当前状态一览

| 项 | 值 |
|----|-----|
| 分支 | `master`（本地；push 后见 remote） |
| 主线 ADR | [`D16_PHASE_E_MAINTAINABILITY.md`](./D16_PHASE_E_MAINTAINABILITY.md) |
| 前置（已 Landed） | [D15_FINAL_ARCHITECTURE_CONVERGENCE.md](./D15_FINAL_ARCHITECTURE_CONVERGENCE.md) |

### D16 子项 DoD（§5）

| 子项 | 状态 | 说明 |
|------|------|------|
| **E2** SubAgent 拆文件 | ✅ Landed | `subagent/mod.rs` ~82 行 |
| **E3** App.tsx 状态机下沉 | ✅ Landed | `App.tsx` ~776 行 |
| **E5** OpenAPI contract CI | ✅ Landed | OpenAPI + TS diff gate；`check-openapi-contract.{sh,ps1}` |
| **E1-a** runtime-adapters tools | 🟡 **阶段 1** | a3–a7：host 端口、纯 helper、network_gate、skills/install 去重 |
| **E1-b** runtime-orchestrator | 🟡 **Partial** | monitor/engine_load/task_port 已迁；sidecar host impl 保留 |
| **E1-c** runtime-api | 🟡 **Partial** | phase 1–5；task schemas 仍部分在 server |
| **E1-d** runtime-server 瘦身 | 🟡 **Partial** | bootstrap 完成；`lib.rs` 62 行 |
| **E4** api/client.ts | ⏸ 可选 | 未做 |

---

## 2. 本地 Git 历史（E1 近期）

从新到旧：

| Commit | 说明 |
|--------|------|
| `6fdc011` | **E5** — OpenAPI + TS 契约 CI；regenerate spec |
| `24c9754` | **E1-a6** — `network_gate`（fetch_url/web_run/web_search） |
| `04a4599` | **E1-a5** — `workspace_walk` / `arg_repair` |
| `7115442` | **E1-a4** — ToolTaskHost / ToolAutomationHost |
| `39b966f` | **E1-a3** + E1-c phase 5 StreamTurnRequest |
| `e8373a1` | **E1-d** — `runtime_serve/http.rs` |
| （更早） | E1-b/c phase 1–4、E1-a 初建 |

---

## 3. Crate 布局

```text
crates/
├── runtime-api/           # OpenAPI、auth、health、cors、ApiError、wire types
├── runtime-orchestrator/  # RuntimeThreadManager、monitor、engine_load、task_port
├── runtime-adapters/      # mcp、persist、snapshot、tools/{host,network_gate,...}
└── runtime-server/        # handler、tools/（ToolSpec 仍在此）、engine host impl

runtime-adapters/src/tools/ — diff_format, schema_sanitize, workspace_walk, arg_repair,
  network_gate, host (RuntimeToolHostWire, ToolTaskHost, …)
```

---

## 4. 推荐实施路线（2026-05-27）

### 阶段 0 — 收口 ✅
- Push 本地 commit → CI 验证
- 本文 + CHANGELOG 更新

### 阶段 1 — 低风险增量（进行中）
| PR | 内容 | 状态 |
|----|------|------|
| E1-a7 | `skills/install.rs` → `network_gate` | 进行中 |
| E1-a8 | 大文件模块内拆分（`shell.rs` / `file.rs` / `web_run.rs`） | 待做 |
| E1-c6 | task OpenAPI schemas → runtime-api | 待做 |
| E1-d2 | `run_http_server` re-export / 文档对齐 | 待做 |

### 阶段 2 — 解循环依赖（待 ADR）
- 在 adapters 定义 `ToolExecutionHost` trait 面
- `ToolContext` 瘦身 → 先迁纯 I/O 工具 → 最后 registry + shell/file/subagent

---

## 5. 验证命令

```powershell
.\scripts\check-openapi-contract.ps1
cargo test -p deepseek-runtime-adapters -p deepseek-runtime-server --lib tools
cargo test -p deepseek-runtime-api -p deepseek-runtime-server --lib runtime_api
cargo test -p deepseek-runtime-server --lib skills::install
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
```

---

## 6. 不变量

- 不改 `/v1/*` HTTP 契约（除非刻意 + OpenAPI 更新）
- `deepseek-desktop` 不 path-depend runtime crate
- Turn 路径：`RuntimeThreadManager` → `core::Engine`
- 单文件 soft cap ~1000 行

---

## 7. 未跟踪 / 勿误 commit

| 文件 | 说明 |
|------|------|
| `crates/desktop/web-ui/src/lib/workspacePaths.ts` | 与 D16 无关 |

---

*维护：Land 新子项后更新 §1 + §2。*
