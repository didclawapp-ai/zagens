# D7 — 持久化统一（Phase C）

**Status:** Landed (2026-05-26)  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 #6 · [PERSISTENCE.md](../PERSISTENCE.md) · [BACKLOG_RUNTIME_UNIFICATION.md](./BACKLOG_RUNTIME_UNIFICATION.md)

## Context

生产 Zagens 经 sidecar HTTP 使用 **两套** SSOT（非 `deepseek-state`）：

| 存储 | 默认路径 | 格式 | 职责 |
|------|----------|------|------|
| **SessionManager** | `~/.deepseek/sessions/` | `sessions.db`（WAL）或 `{id}.json` | 侧栏 SavedSession、消息快照、`runtime_thread_id` 链接 |
| **RuntimeThreadStore** | `~/.deepseek/tasks/runtime/` | `runtime.db` + 可选 JSON/JSONL | 线程/turn/事件流、SSE replay |
| **StateStore**（CLI legacy） | `~/.deepseek/state.db` | SQLite + `session_index.jsonl` | **非** sidecar HTTP SSOT |

**已知缺口（D7 前）：** `session_store_sqlite` 在 SQLite 主路径上 **未读写** `metadata.runtime_thread_id`。

## Decision

1. **Sidecar 双库 + 链接字段：** Sessions 与 Runtime threads 保持独立 DB，**`runtime_thread_id`** + HTTP 契约；叙事 SSOT → [PERSISTENCE.md](../PERSISTENCE.md)。
2. **CLI：** `deepseek thread list` 默认 `--source runtime`（只读 `runtime.db`）；`--source state` 保留 legacy。
3. **`app-server` 删除：** D4 defer 兑现；生产 HTTP 仅 `runtime_api` / `deepseek-runtime`。
4. **不物理合并** `sessions.db` 与 `runtime.db`（非目标）。

## PR 链（已落地）

| PR | 状态 |
|----|------|
| **C1** | ✅ `runtime_thread_id` SQLite 列 + 迁移 |
| **C2** | ✅ [PERSISTENCE.md](../PERSISTENCE.md) + RUNTIME_ARCHITECTURE §4 |
| **C3** | ✅ `session_resume_reuses_runtime_thread_when_sqlite_has_link` |
| **C4** | ✅ `crates/cli/src/runtime_thread_cli.rs` + `--source runtime\|state` |
| **C5** | ✅ 删除 `crates/app-server`、CLI `app-server` 子命令 |
| **C6** | ✅ §1 #6 勾选（可文档化双库故事，非单文件） |

## Acceptance

- [x] C1–C6 如上
- [x] Zagens `persist-session` / `resume-session` 链接无已知 SQLite 缺口
- [x] CLI 默认 list 读 production runtime store

## Non-goals（仍 defer）

- 合并为单 `.db` 文件
- 删除 JSON 回退路径
- 删除 `deepseek-state` crate（CLI legacy 元数据仍用）
- OpenAPI / TS 生成（D8）
