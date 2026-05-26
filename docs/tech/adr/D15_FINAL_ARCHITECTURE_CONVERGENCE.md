# D15 — 架构收官实施计划（Desktop-only SSOT）

> **类型：** 实施计划（收官阶段，非新功能主线）  
> **状态：** Landed（2026-05-26）  
> **前置（已 Landed）：** M1–M8 · D6 Phase B · D7 · D8 · D9/D10 · Zagens v0.5.0  
> **产品意图：** 以 **Zagens Desktop** 为唯一用户入口，替换 upstream deepseek-tui 0.8.15 的 TUI/CLI；Sidecar 为内嵌运行时，非第二产品面  
> **SSOT 架构图：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)  
> **与旧编号关系：** 本文 **D15** 指「架构收敛收官」；[ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) 中 **D11–D14**（metrics / 多 sidecar / Capability Manifest / MCP 池）为 **定型后的增强项**，不在本计划阻塞范围内

---

## 0. 目标与完成定义

### 0.1 要解决的问题

多轮重构（Sidecar 验证 → Engine 进 core → D6 去 TUI → D7 持久化链接）后，代码仍保留 **TUI/CLI 时代的幽灵路径** 与 **双轨 mental model**，导致：

- 新人仍按「TUI + Sidecar 并行」理解代码  
- `deepseek-state` / `core::Runtime` 无生产调用却占编译图与认知  
- Session 与 Thread 在 D7 已 **链接**，但 API/存储仍像两套主数据  
- `runtime-server` 单 crate ~10 万行，阻碍长期稳定维护  

### 0.2 D15 做什么 / 不做什么

| 做 | 不做 |
|----|------|
| 删除 `state` crate 及 `core::lib.rs` legacy `Runtime` 链 | 重写 `/v1/*` HTTP 契约 |
| 确认 D7 持久化叙事为 **Thread/Event SSOT**，Session 降为投影 | 每 workspace 独立 sidecar（旧 roadmap D12） |
| 统一注释/命名（TUI → Runtime adapter / Sidecar） | Prometheus metrics（旧 roadmap D11） |
| 加 CI 架构 gate，防止 legacy 反弹 | Capability Manifest 合并（旧 roadmap D13） |
| 可选：拆 `runtime-server` 为 api / orchestrator / adapters | Desktop 内嵌 runtime（去 HTTP hop）— 留 v0.7+ |
| 可选：Web UI `App.tsx` 状态机下沉 | 追 upstream TUI 新特性 |

### 0.3 「架构统一完成」验收（Definition of Done）

满足 **全部** 下列条件，可对外/对内宣布 **Desktop 已替换 TUI，架构 SSOT 确立**：

1. **用户入口：** 文档与产品仅描述 Zagens Desktop；无 CLI/TUI 用户路径  
2. **Legacy 零生产引用：** workspace 无 `deepseek-state`；`core` 无 `Runtime` / `ThreadManager` / `JobManager` / `ThreadMessageTurnPort`  
3. **Turn 单路径：** 生产代码仅 `RuntimeThreadManager` → `core::Engine` → `EngineToolDispatch`  
4. **持久化 SSOT：** Thread + Event 为权威；Session API 可读可写但 **不独立增长主数据**（见 §3）  
5. **Desktop 边界：** `desktop` 不 path-depend `runtime-server` / `deepseek-tui`（已有测试，保持）  
6. **契约：** OpenAPI 导出 + 至少一条 golden path 集成测（create thread → turn → SSE → approval）  
7. **发布：** CHANGELOG 记录 D15；Zagens ≥ v0.5.x 稳定跑通主流程  

---

## 1. 当前基线（2026-05-26）

### 1.1 已完成（不必重做）

| 里程碑 | 证据 |
|--------|------|
| TUI crate / ratatui 删除 | D6 Phase B；`cargo tree -i ratatui` 无匹配 |
| Sidecar 二进制 | `deepseek-runtime` HTTP only |
| Engine strangler | M1–M8；`core::engine::Engine` + Host traits |
| Desktop 进程隔离 | `architecture_boundary.rs` |
| D7 链接 | `sessions.db.runtime_thread_id` ↔ `runtime.db` |
| OpenAPI + TS | D8；`export-runtime-openapi` + `generate:api-types` |
| 产品发布 | `release(zagens): v0.5.0` |

### 1.2 待收敛（D15 范围）

| 项 | 现状 | 目标 |
|----|------|------|
| `crates/state` | 仅 `core` 依赖；Sidecar 注释标明非 SSOT | **删除** |
| `core/src/lib.rs` | ~1839 行；含 `Runtime`/`ThreadManager`/`JobManager` | **删除 legacy 块**；保留 re-export 或拆到 `core/src/legacy/` 后删 |
| `ThreadMessageTurnPort` | 仅 `runtime_threads/tests.rs` 使用 | **删除** trait + `RuntimeThreadMessageTurnPort` |
| Session vs Thread | D7 已链接；Sidebar 仍走 `/v1/sessions` | **Session = Thread 投影**（§3） |
| 命名 | 大量 `deepseek-tui` / `Tui-side` 注释 | **批量替换** |
| `sidecar.rs` | `legacy_tui` binary 分支 | **删除** |
| `runtime-server` 体量 | ~103k 行 Rust | **Phase E 可选拆分** |

### 1.3 架构不变量（PR 审查用）

合并后每条 PR 必须遵守：

1. 新代码 **不得** 依赖 `deepseek-state`  
2. 新 Turn **不得** 绕过 `RuntimeThreadManager::start_turn`  
3. Desktop WebView **不得** 持有 runtime Bearer（保持 Tauri proxy）  
4. 新持久化 **不得** 引入第三套 SSOT  
5. 新 `.rs`/`.tsx` 默认 **≤1000 行**（超限需 ADR 豁免）  

---

## 2. 目标架构（收官后）

```text
┌─────────────────────────────────────────────────────────────┐
│  Zagens（唯一用户产品）                                        │
│  crates/desktop + web-ui                                     │
│  Tauri · Sidecar supervisor · runtime_proxy · PTY            │
└──────────────────────────┬──────────────────────────────────┘
                           │ localhost /v1/* + Bearer (Rust 注入)
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek-runtime（内嵌子进程，非用户 CLI）                     │
│  ┌─────────────┐  ┌──────────────────┐  ┌───────────────┐ │
│  │ runtime-api │→ │ RuntimeThread    │→ │ core::Engine  │ │
│  │ (HTTP/SSE)  │  │ Manager          │  │ + turn_loop   │ │
│  └─────────────┘  └────────┬─────────┘  └───────┬───────┘ │
│                              │                    │         │
│                    RuntimeThreadStore      tools/mcp/llm    │
│                    (threads/turns/events)   (adapters)    │
└─────────────────────────────────────────────────────────────┘

持久化 SSOT:
  ~/.deepseek/tasks/runtime/runtime.db   ← Thread / Turn / Event
  ~/.deepseek/sessions/sessions.db       ← Session 投影（含 runtime_thread_id）

已删除:
  deepseek-state · core::Runtime · CLI/TUI 入口 · ThreadMessageTurnPort
```

---

## 3. 阶段划分与 PR 链

建议 **5 个阶段、8–12 个 PR**，总工期 **4–8 周**（按每周 2–3 个 refactor PR 估算）。  
**规则：每个 PR 可独立合并；阶段内顺序不可颠倒。**

---

### Phase A — 冻结与护栏（≈3 天，1 PR）

**PR-A1：`chore(arch): D15 invariants + CI grep gates`**

| 动作 | 细节 |
|------|------|
| 新增测试 | `crates/runtime-server/tests/architecture_invariants.rs` |
| Gate 1 | `runtime-server` / `desktop` 生产代码无 `deepseek_state` / `StateStore` |
| Gate 2 | `desktop/Cargo.toml` 无 `runtime-server` / `deepseek-tui` path dep（扩展现有 boundary test） |
| Gate 3 | 可选：`scripts/check-architecture.sh` 供 CI 调用 |
| 文档 | 本文 Status → In Progress；CHANGELOG `[Unreleased]` 增 D15 条目 |

**退出标准：** CI 红则阻断；legacy 引用 **允许存在于 core/state**，但 **禁止新增**。

---

### Phase B — 删除 legacy 编排层（≈1–2 周，2 PR）

**PR-B1：`refactor(core): remove legacy Runtime and state dependency`**

| 删除/修改 | 路径 |
|-----------|------|
| 删除 crate | `crates/state/`（含 `parity_state` 测试 — 迁移必要断言到 runtime 测试） |
| 大删 | `crates/core/src/lib.rs` 中 `JobManager`、`ThreadManager`、`Runtime` 及仅服务于它们的 ~1500 行 |
| 删除 | `crates/core/src/thread_message_turn.rs` |
| 修改 | `crates/core/Cargo.toml` — 移除 `deepseek-state` |
| 修改 | 根 `Cargo.toml` workspace members — 移除 `state` |
| 保留 | `core/src/engine/*`、`protocol` re-export 如需则从 `lib.rs` 精简 re-export |

**PR-B2：`refactor(runtime): drop ThreadMessageTurnPort shim`**

| 删除 | `crates/runtime-server/src/runtime_threads/thread_message_turn_port.rs` |
| 修改 | `runtime_threads/mod.rs` — 移除 `RuntimeThreadMessageTurnPort` export |
| 修改 | `runtime_threads/tests.rs` — 删除 PR5 port 测试；保留 `start_turn` 直连测试 |

**验证：**

```bash
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features
rg 'deepseek-state|StateStore|ThreadMessageTurnPort|core::Runtime' crates/ --glob '!**/target/**'
# 期望：0 生产匹配
```

**退出标准：** workspace 编译通过；无 `state` crate；grep 零生产命中。

**风险：** 若有隐藏测试依赖 `core::Runtime` — 改为 `RuntimeThreadManager` fixture。  
**回滚：** 单 PR revert；不涉及数据 migration。

---

### Phase C — 持久化 SSOT 确认（≈1–2 周，2–3 PR）

D7 已落地 `runtime_thread_id` 链接；本阶段 **确认叙事与写路径一致**。

**PR-C1：`docs+test(runtime): document Thread/Event as SSOT; session as projection`**

| 动作 | 细节 |
|------|------|
| 更新 | `docs/tech/PERSISTENCE.md` — Thread/Event SSOT；Session 投影表 |
| 测试 | Golden path：`create_thread` → `persist-session` → `list_sessions` → `resume` 重用同一 thread |
| 审计 | 列出所有写 `SessionManager` 的路径；标注是否可改为写 Thread 后投影 |

**PR-C2：`refactor(runtime): session writes derive from thread (no orphan sessions)`**

| 动作 | 细节 |
|------|------|
| 修改 | `runtime_api/threads.rs` — create/update thread 时同步 session 投影 |
| 修改 | `runtime_api/sessions.rs` — delete/resume 仅操作链接 thread |
| 禁止 | 新建 **无** `runtime_thread_id` 的 session（API 层校验） |
| 可选 | 一次性 migration：扫描 orphan session → 创建 thread 或标记 archived |

**PR-C3（可选）：`feat(runtime): session list reads thread store directly`**

| 动作 | 细节 |
|------|------|
| 优化 | `list_sessions` 优先 JOIN `runtime.db` metadata |
| Desktop | 确认 Sidebar 在 sidecar restart 后仍一致（回归 `b04864d` 类 bug） |

**退出标准：**

- 任意 session 必有 `runtime_thread_id`（新数据）  
- Event log 可 rebuild UI（已有 `rebuildMessagesFromThreadEvents`）  
- 无「只写 session 不写 thread」的新代码路径  

**风险：** 旧用户 orphan session — migration PR 需 dry-run 日志。  
**回滚：** 保留 `runtime_thread_id` 列；回滚写路径逻辑即可。

---

### Phase D — 命名与 Sidecar 收尾（≈1 周，2 PR）

**PR-D1：`refactor: TUI → Runtime adapter naming (comments + prompts)`**

| 范围 | 示例 |
|------|------|
| `runtime-server/src/core/engine.rs` | 「Tui-side」→「Runtime adapter」 |
| `core/src/engine/mod.rs` | 更新 stale「live in deepseek-tui」注释 |
| `prompts.rs` | 确认 `CLIENT_IDENTITY_DS_PICK` 为 Desktop SSOT；删除或 gate `CLIENT_IDENTITY_TERMINAL` |
| `config/src/lib.rs` | 「TUI-compatible」→「runtime-compatible config.toml」 |

**PR-D2：`refactor(desktop): remove legacy_tui sidecar spawn paths`**

| 修改 | `crates/desktop/src/sidecar.rs` |
|------|--------------------------------|
| 删除 | `legacy_tui` 分支、`deepseek-tui` candidate binary |
| 保留 | `deepseek-runtime` + bundled `binaries/deepseek-runtime-*` |
| 测试 | sidecar spawn 单路径集成测 |

**退出标准：** `rg 'deepseek-tui|ratatui|Tui-side' crates/ --glob '*.rs'` 仅剩 NOTICE/测试 fixture/历史 ADR。

---

### Phase E — 维护性（可选，v0.6+，3–5 PR）

**不阻塞 D15 DoD。** 架构统一宣布后可并行产品功能。

| PR | 内容 | 优先级 |
|----|------|--------|
| E1 | 拆 `runtime-server` → `runtime-api` + `runtime-orchestrator` + `runtime-adapters` | 中 |
| E2 | `tools/subagent/mod.rs` 拆模块（mailbox/craft/spawn 已有子文件，主文件 <800 行） | 中 |
| E3 | `web-ui/App.tsx` 抽 `useRuntimeConnection` + `useTurnSession` | 低 |
| E4 | 单一 ToolRegistry 审计（确认 `deepseek-tools` 仅协议层，dispatch 唯一） | 低 |
| E5 | OpenAPI contract test in CI（export diff + smoke HTTP） | 高（建议尽早） |

---

## 4. PR 顺序总览

```text
A1  CI gates + D15 doc
 │
 ├─ B1  delete state + core::Runtime
 ├─ B2  delete ThreadMessageTurnPort
 │
 ├─ C1  persistence SSOT docs + golden test
 ├─ C2  session projection writes
 ├─ C3  (opt) session list from thread store
 │
 ├─ D1  naming cleanup
 └─ D2  sidecar legacy removal
      │
      └─ E*  optional maintainability (parallel)
```

**建议合并策略：** B1+B2 可同周；C 依赖 B；D 可与 C 并行；E1 依赖 D15 DoD。

---

## 5. 验证矩阵

每个 Phase 合并前跑：

| 命令 | 用途 |
|------|------|
| `cargo check --workspace` | 编译 |
| `cargo test --workspace` | 单元 + 集成 |
| `cargo clippy --workspace --all-targets --all-features` | lint |
| `cargo tree -p deepseek-runtime-server -i ratatui` | 无 TUI |
| `.\scripts\export-runtime-openapi.ps1` | OpenAPI 无意外 diff |
| `cd crates/desktop/web-ui && npm run build` | TS strict |
| 手动 | Zagens 冷启动 → 对话 → 工具 → 审批 → 重启 sidecar → Sidebar 一致 |

---

## 6. 风险与缓解

| 风险 | 缓解 |
|------|------|
| 删除 `state` 后遗漏测试引用 | B1 前全 workspace `rg StateStore`；CI gate |
| Session migration 丢历史 | C2 migration dry-run + 备份 `sessions.db` |
| 大 PR 难 review | 严格按 B1/B2 拆分；B1 只动 core/state |
| 重构反弹 | Phase A CI gate + PR template 检查不变量 |
| 与产品发版冲突 | D15 收官 PR 可与 Zagens v0.5.1 patch 同发 |

---

## 7. CHANGELOG 模板

```markdown
### Architecture
- **D15:** Final architecture convergence — removed `deepseek-state` and legacy `core::Runtime`; Session is a projection of RuntimeThreadStore; Sidecar spawn paths unified to `deepseek-runtime` only. Desktop is the sole user entry (replaces upstream TUI/CLI).
```

---

## 8. 新会话推荐开场白

```text
执行 D15 架构收官。请先读 docs/tech/adr/D15_FINAL_ARCHITECTURE_CONVERGENCE.md。
当前阶段：[A/B/C/D/E]。从 PR-__ 开始。不要 commit/push 除非我要求。中文回复。
```

---

## 9. 与后续 roadmap 的关系

| 旧编号（ASSESSMENT §5） | 与 D15 关系 |
|-------------------------|-------------|
| D11 metrics | D15 完成后可选 |
| D12 每 workspace 一 sidecar | **不在** Desktop-only 收官范围；需单独 ADR |
| D13 Capability Manifest | D15 完成后；Harness 提案已挂接 |
| D14 MCP 池稳定性 | 可与 E 阶段并行 |

---

*维护：D15 DoD 全部勾选后，本文 Status 改为 **Landed**，并在 RUNTIME_ARCHITECTURE.md §剩余债 移除对应项。*
