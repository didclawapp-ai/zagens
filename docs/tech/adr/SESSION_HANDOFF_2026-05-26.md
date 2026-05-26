# 新会话交接 — Zagens 架构定型（2026-05-26）

> **给新窗口的第一句话：**  
> 请读本文 + [`ARCHITECTURE_ASSESSMENT_2026-05-25.md`](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §5.1。主线已完成 **D6 / D9–D10 / D7 / D8**；§1 **9/10**；**下一优先 D1**（巨型文件收尾）。未 push 到 remote 除非用户明确要求。

---

## 1. 当前状态一览

| 项 | 值 |
|----|-----|
| 分支 | `master`（本地） |
| §1 定型进度 | **9/10** |
| 下一主线 | **D1** — `config.rs` / `compaction.rs` / `mcp.rs` / `client.rs` / `localization.rs` / `commands.rs` 等 ≤1000 行 |
| 再之后 | **10/10** 定型完成 → P2（D11→D14→D13→D12） |

### §1 勾选（10 项）

| # | 项 | 状态 |
|---|-----|------|
| 1–5 | L1/L2/安全/Engine/core/sidecar 去 ratatui | ✅ |
| 6 | 持久化统一（双库 + `runtime_thread_id`） | ✅ **D7** |
| 7 | app-server 决策 | ✅ deprecated → **D7 已删 crate** |
| 8 | tui-core 删除 | ✅ |
| 9 | OpenAPI + TS 自动生成 | ✅ **D8** |
| 10 | 巨型文件 ≤1000 行 | ⬜ **D1** |

---

## 2. 本会话已完成（按 §5.1 顺序）

| 阶段 | ID | Commit（本地） | 要点 |
|------|-----|----------------|------|
| A | **D6** | `a1aacbe` | `deepseek-runtime` sidecar；`crates/runtime-server`；Zagens bundle 切换 |
| B | **D9** | `01f2624` | `turnControl.ts` 两层 Stop；`stopThreadTurn` |
| B | **D10** | `01f2624` | `filterThreadStreamEvents` + `windowOwnsThreadForStream` |
| C | **D7 C1** | `2c1e86a` | `sessions.db` 列 `runtime_thread_id` |
| C | **D7 C3** | `04fb379` | 集成测 `session_resume_reuses_runtime_thread_when_sqlite_has_link` |
| C | **D7 C2–C6** | `952f706` | [`PERSISTENCE.md`](../PERSISTENCE.md)；CLI `thread list --source runtime`；删 `app-server`；§1 #6 |
| D | **D8** | *(未 commit)* | OpenAPI export + `web-ui` TS 生成；见 [`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md) |

**D7 主 ADR：** [`D7_PERSISTENCE_UNIFICATION.md`](./D7_PERSISTENCE_UNIFICATION.md)（Status: **Landed**）  
**D8 主 ADR：** [`D8_OPENAPI_TS_GENERATION.md`](./D8_OPENAPI_TS_GENERATION.md)（Status: **Landed**）

---

## 3. D8 产物速查

| 路径 | 说明 |
|------|------|
| [`docs/tech/openapi/zagens-runtime-v1.openapi.json`](../openapi/zagens-runtime-v1.openapi.json) | 检入 OpenAPI 3.1 |
| `crates/tui/src/runtime_api/openapi/` | Rust 导出 + 54 path 回归测 |
| `cargo run … --bin export-runtime-openapi` | 再生 JSON |
| `crates/desktop/web-ui/src/api/generated/runtime-api.ts` | 生成 TS |
| `crates/desktop/web-ui/src/api/runtimeTypes.ts` | 稳定 re-export |
| [`V2_API_VERSIONING.md`](./V2_API_VERSIONING.md) | `/v2` 草案 |

```powershell
.\scripts\export-runtime-openapi.ps1
cd crates\desktop\web-ui
npm install --include=dev
npm run generate:api-types
```

---

## 4. 持久化（D7 后必读）

**生产 SSOT（sidecar HTTP）：**

```
~/.deepseek/sessions/sessions.db     ← SessionManager（含 runtime_thread_id）
~/.deepseek/tasks/runtime/runtime.db   ← RuntimeThreadStore（线程/事件）
```

- 叙事 SSOT：[`docs/tech/PERSISTENCE.md`](../PERSISTENCE.md)

---

## 5. 下一任务：D1 → 10/10

按 §5.1 阶段 E，优先拆分（各 ≈1 PR）：

1. `crates/tui/src/config.rs`（最大）
2. `compaction.rs` · `mcp.rs` · `localization.rs` · `client.rs`
3. `crates/desktop/src/commands.rs`

软上限 ~1000 行：`.cursor/rules/code-organization.mdc`

---

## 6. Git 与工作区

### 用户规则

- **不要主动 `git commit` / `git push`**，除非用户明确要求
- D8 变更记 [`CHANGELOG.md`](../../../CHANGELOG.md) `[Unreleased]`

### 未提交（与主线无关，默认不要 `git add`）

| 路径 | 说明 |
|------|------|
| `.gitignore` | `.docs/topic-memory-graph-main/` 忽略 |
| `docs/Agent+Harness组合式编程方案.md` | 产品草案 |
| `docs/tech/adr/HARNESS_INTEGRATION_PROPOSAL.md` | Harness 提案 |

---

## 7. 验证命令

```bash
cargo check --workspace
cargo test -p deepseek-tui --lib runtime_api::openapi
cargo run -p deepseek-tui --no-default-features --features openapi-export,json,toml --bin export-runtime-openapi

cd crates/desktop/web-ui && npm install --include=dev && npm run generate:api-types && npm run build
```

---

## 8. 新窗口推荐开场白

```
继续 Zagens 架构定型。§1 已 9/10，D8 已落地。请从 D1 开始：巨型文件拆分。
不要 commit/push 除非我要求。用中文回复。
```
