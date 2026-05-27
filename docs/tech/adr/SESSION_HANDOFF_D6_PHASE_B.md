# 新会话交接 — D6 Phase B（Runtime 单 crate + CLI/TUI 退场）

> **最后更新：** 2026-05-27  
> **状态：** ✅ **Landed**（2026-05-26）— 非进行中任务  
> **权威 ADR：** [`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md)  
> **后续维护拆分：** 见 [`SESSION_HANDOFF_D16_PHASE_E.md`](./SESSION_HANDOFF_D16_PHASE_E.md)（D16 E1 把 Phase B 合并后的大 crate 再拆成 4 crate）

---

## 给新窗口的第一句话（复制即用）

```
请读 D6 Phase B 交接：docs/tech/adr/SESSION_HANDOFF_D6_PHASE_B.md
及 ADR：D6_PHASE_B_CLI_SUNSET.md、D6_RUNTIME_SERVER.md、RUNTIME_ARCHITECTURE.md。

D6 Phase B 已于 2026-05-26 落地（删除 cli/tui，runtime 单 crate deepseek-runtime）。
若要做新工作：要么是 Phase B 后遗留项（冷启动 profiling、文档核对），
要么是 D16 E1 继续拆 runtime-server（见 SESSION_HANDOFF_D16_PHASE_E.md）——不要重复做 Phase B 迁移。

不要 commit/push 除非我要求。中文回复。
```

---

## 1. D6 Phase B 是什么

| 项 | 说明 |
|----|------|
| **目标** | Sidecar 与 ratatui/CLI **彻底解耦**；运行时合并为 **单 crate** |
| **方案** | **方案 B**（[`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md)）— 直接合并进 `runtime-server`，**非** spike 里的 `agent-host` 分叉 |
| **Spike** | [`D6_PHASE_B_SPIKE.md`](./D6_PHASE_B_SPIKE.md) 已 **Superseded**，仅作历史参照 |
| **产品影响** | Zagens **不变**：仍嵌入 bin `deepseek-runtime`，Desktop **不** path-depend runtime lib |

### Phase A/A+ → Phase B 演进

```text
Phase A/A+ (2026-05-26 初)
  deepseek-runtime (bin) → deepseek_tui::runtime_serve
  HTTP 源码仍在 crates/tui，sidecar 已不链 ratatui

Phase B (2026-05-26 落地)
  deepseek-runtime (bin) → deepseek_runtime::runtime_serve
  全部 runtime 源码 → crates/runtime-server/src/
  删除 crates/cli、crates/tui
```

---

## 2. 落地结果（DoD）

| 验收项 | 状态 |
|--------|------|
| workspace 无 `crates/cli`、`crates/tui` | ✅ |
| `runtime_api/*`、`runtime_threads/*` 在 `crates/runtime-server/src/` | ✅ |
| lib 名 `deepseek_runtime`，bin `deepseek-runtime` | ✅ |
| `cargo tree -p deepseek-runtime-server -i ratatui` 无匹配 | ✅ |
| `export-runtime-openapi` 在 `runtime-server` | ✅ |
| `sidecar_contract_full_lifecycle` + `sidecar_binary_contract` CI | ✅ |
| [`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md) 已更新 | ✅ |
| CHANGELOG v0.5.0 里程碑记录 | ✅ |

**实现 commit（历史）：** `613a6e3` — *D6 Phase B: merge runtime into single crate and drop CLI/TUI.*

---

## 3. 当前 crate 布局（Phase B 后）

```text
crates/runtime-server/
  Cargo.toml   package: deepseek-runtime-server
  [lib]        name = deepseek_runtime
  [bin]        deepseek-runtime  → runtime_serve::run_from_args

  src/
    runtime_api/      # /v1/* HTTP/SSE handlers
    runtime_threads/  # RuntimeThreadManager、turn 编排
    runtime_serve/    # run_http_server、sidecar 入口
    core/engine/      # Engine shim（→ deepseek-core）
    tools/            # 全部工具
    mcp/              # MCP 池
    …

已删除:
  crates/cli/         # deepseek 分发器
  crates/tui/         # ratatui TUI + 原 runtime 宿主

保留（非 sidecar SSOT）:
  deepseek-state      # deepseek-core 仍编译依赖
```

### 依赖方向（无环）

```text
deepseek-desktop  →  config, secrets（不链 runtime-server）
deepseek-runtime (bin)  →  deepseek_runtime (lib)
deepseek_runtime  →  deepseek-core, tools, config, protocol, …
```

---

## 4. PR 链回顾（B0→B3，均已 ✅）

| PR | 内容 |
|----|------|
| **B0** | 删 `crates/cli`；`transcript_isomorphism` 抽出；测试改 `ApprovalMode` |
| **B1** | 删 TUI 树（`src/tui/`、`commands/`）；去 ratatui/crossterm；OpenAPI bin 迁 server |
| **B2** | `tui/src/*` → `runtime-server/src/`；删 `crates/tui`；CI `-p deepseek-runtime-server` |
| **B3** | 文档/CI 同步；`RUSTFLAGS=-Dwarnings` workspace 绿；Zagens 冒烟 |

---

## 5. 与 D16 的关系（易混淆）

| 阶段 | 做什么 |
|------|--------|
| **D6 Phase B** | 多 crate → **单 crate**（合并） |
| **D16 E1** | 单 crate → **4 crate**（`runtime-api` / `orchestrator` / `adapters` / 瘦 `server`） |

Phase B 解决的是「sidecar 不该链 ratatui + HTTP 代码该放哪」。  
D16 解决的是 Phase B 之后 **`runtime-server` ~10 万行** 的维护性。

**若新会话目标是继续架构工作 → 读 D16 handoff，不是重做 D6 PB。**

---

## 6. Phase B 后可选跟进（非阻塞）

| 项 | 来源 | 说明 |
|----|------|------|
| **冷启动 ~9s** | [D6_PHASE_B_CLI_SUNSET §6](./D6_PHASE_B_CLI_SUNSET.md) | Tauri/WebView、SQLite 延迟打开、skills 后台安装；需 profiling |
| **删 `deepseek-state`** | D6 非目标 | 等 `deepseek-core` 零引用后再删 |
| **P2 multi-sidecar** | D12 | **不在** D6/D16；需单独 ADR |
| **D16 E1 继续** | [SESSION_HANDOFF_D16](./SESSION_HANDOFF_D16_PHASE_E.md) | 当前活跃维护线 |

---

## 7. 验证命令

```powershell
# Phase B 核心验收
cargo check --workspace
cargo tree -p deepseek-runtime-server -i ratatui      # 应无输出
cargo tree -p deepseek-runtime-server -i crossterm     # 应无输出

# Sidecar 契约（CI）
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract

# OpenAPI（D8，bin 在 runtime-server）
.\scripts\check-openapi-contract.ps1

# Desktop 冒烟
cd crates/desktop/web-ui && npm run build
# Zagens bundle: 见 docs/desktop/DEV_NOTES.md
```

**目录应不存在：**

```powershell
Test-Path crates/cli   # False
Test-Path crates/tui   # False
```

---

## 8. 不变量（Phase B 未改）

- `/v1/*` HTTP 契约语义不变  
- Turn 路径：`RuntimeThreadManager` → `core::Engine`  
- Sidecar **不**合并进 Tauri 进程  
- Desktop WebView **不**直接链 `deepseek-core` / runtime lib  
- Bearer token 不出 WebView（经 Tauri `runtime_proxy`）

---

## 9. 文档索引

| 文档 | 用途 |
|------|------|
| [`D6_PHASE_B_CLI_SUNSET.md`](./D6_PHASE_B_CLI_SUNSET.md) | Phase B 决策 + B0–B3 + 验收 |
| [`D6_IMPLEMENTATION_PLAN.md`](./D6_IMPLEMENTATION_PLAN.md) | D6 全阶段（A/A+/B） |
| [`D6_RUNTIME_SERVER.md`](./D6_RUNTIME_SERVER.md) | sidecar crate 一页摘要 |
| [`D6_PHASE_B_SPIKE.md`](./D6_PHASE_B_SPIKE.md) | ⚠️ 历史 spike（agent-host 路径已废弃） |
| [`RUNTIME_ARCHITECTURE.md`](../RUNTIME_ARCHITECTURE.md) | 架构 SSOT 附图 |
| [`SESSION_HANDOFF_D16_PHASE_E.md`](./SESSION_HANDOFF_D16_PHASE_E.md) | Phase B **之后**的维护拆分 |

---

## 10. 常见误判

| 误判 | 实际 |
|------|------|
| 「还要把 runtime_api 从 tui 迁出」 | 已在 `runtime-server`；tui 已删 |
| 「要做 agent-host crate」 | Spike 方案已 superseded；用了单 crate 合并 |
| 「D6 PB = 当前任务」 | **已 Landed**；活跃线是 D16 E1 |
| 「OpenAPI bin 还在 tui」 | 在 `crates/runtime-server`，feature/bin `export-runtime-openapi` |

---

*维护：Phase B 无新 land 项；架构后续变更更新 D16 handoff 与 RUNTIME_ARCHITECTURE。*
