# D7 — 持久化统一（Phase C spike + PR 链）

**Status:** In progress (2026-05-26) — spike landed; **PR-C1** `runtime_thread_id` SQLite 修复已开  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 #6 · §5.1 阶段 C · [BACKLOG_RUNTIME_UNIFICATION.md](./BACKLOG_RUNTIME_UNIFICATION.md) · [BACKLOG_STATESTORE_JSONL.md](./BACKLOG_STATESTORE_JSONL.md)

## Context

生产 Zagens 经 sidecar HTTP 使用 **两套** SSOT（非 `deepseek-state`）：

| 存储 | 默认路径 | 格式 | 职责 |
|------|----------|------|------|
| **SessionManager** | `~/.deepseek/sessions/` | `sessions.db`（WAL）或 `{id}.json` | 侧栏 SavedSession、消息快照、`runtime_thread_id` 链接 |
| **RuntimeThreadStore** | `~/.deepseek/tasks/runtime/` | `runtime.db` + 可选 JSON/JSONL | 线程/turn/事件流、SSE replay |
| **StateStore**（CLI / 废弃 app-server） | `~/.deepseek/state.db` | SQLite + `session_index.jsonl` | **非** sidecar HTTP SSOT；D7 末段收缩 |

**已知缺口（D7 前）：** `session_store_sqlite` 在 SQLite 主路径上 **未读写** `metadata.runtime_thread_id`，导致 `persist-session` / `resume-session` 在桌面 SQLite 用户上链接丢失。

## Decision（Phase C 目标）

1. **Sidecar 双库合一视图（不物理合并文件）：** Sessions 与 Runtime threads 保持独立 DB，但通过 **`runtime_thread_id` 外键语义** + HTTP 契约稳定链接；文档 SSOT 明确谁写谁读。
2. **StateStore 收缩：** CLI 逐步读 runtime 镜像或 HTTP；`app-server` 随 D4 已 deprecated，D7 完成后删 crate。
3. **迁移策略：** 各库内 JSON→SQLite 已有；跨库 **不做 dual-write SSOT**，仅 **列补丁 + 回填**；跨 StateStore 合并需单独 migration ADR + dual-write 窗口（末 PR）。

## 数据流（当前生产）

```
WebView → /v1/sessions*     → SessionManager  → ~/.deepseek/sessions/sessions.db
WebView → /v1/threads/*     → RuntimeThreadStore → ~/.deepseek/tasks/runtime/runtime.db
WebView → persist-session   → export thread → save SessionManager (+ runtime_thread_id)
WebView → resume-session    → load session → reuse runtime_thread_id → replay events

CLI thread *                → StateStore      → ~/.deepseek/state.db  (parallel, not HTTP SSOT)
```

## PR 链（阶段 C，小步可回归）

| PR | 范围 | 风险 | §1 |
|----|------|------|-----|
| **C1** | `sessions.db` 增加 `runtime_thread_id` 列；save/load/list/migrate 全路径 | 低 | — |
| **C2** | D7 文档：路径、`DEEPSEEK_*`  env、契约表；BACKLOG 状态更新 | 无 | — |
| **C3** | `resume-session` / `persist-session` 集成测（SQLite 路径） | 低 | — |
| **C4** | CLI 只读适配层：list thread 元数据可选走 runtime store | 中 | — |
| **C5** | 收缩 `deepseek-state`；删 `app-server` crate（D4 defer 兑现） | 中 | — |
| **C6** | 择机：单 SQLite 文件多 schema 或统一 data dir（需 spike 再 ADR） | 高 | **#6 勾选 → 8/10** |

**C6 勾选条件（Assessment §1 #6）：** Sessions + Runtime threads 对 Zagens 而言 **单一可文档化的持久化故事**（路径/迁移/链接无已知缺口），而非必须一个物理 `.db` 文件。

## Acceptance（阶段 C 整体）

- [x] C1：`runtime_thread_id` round-trip SQLite
- [x] C3：sidecar `session_resume_reuses_runtime_thread_when_sqlite_has_link` 集成测
- [ ] C4–C5：CLI 不依赖第二套 thread SSOT 写路径
- [ ] C6：架构 owner 签收 §1 #6

## Non-goals（本阶段不做）

- 合并 `runtime.db` 与 `sessions.db` 为单文件
- 删除 JSON 回退路径
- OpenAPI / TS 生成（D8）
