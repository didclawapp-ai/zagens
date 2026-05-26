# 持久化布局（D7 SSOT）

**Status:** Landed 2026-05-26 · [D7_PERSISTENCE_UNIFICATION.md](./adr/D7_PERSISTENCE_UNIFICATION.md)

Zagens 与 **`deepseek-runtime`** sidecar 的生产数据路径。**非**物理单库：Sessions 与 Runtime threads 各用 SQLite（或 JSON 回退），由 **`runtime_thread_id`** 链接。

## 目录与文件

| 轨 | 默认路径 | 主文件 | 环境变量 |
|----|----------|--------|----------|
| **Sessions** | `~/.deepseek/sessions/` | `sessions.db`（WAL）；回退 `{id}.json` | — |
| **Runtime threads** | `~/.deepseek/tasks/runtime/` | `runtime.db`；回退 `threads/`、`events/*.jsonl` | `DEEPSEEK_RUNTIME_DIR` 覆盖 runtime 根；`DEEPSEEK_TASKS_DIR` 覆盖 tasks 根（默认 `~/.deepseek/tasks`） |

`DEEPSEEK_RUNTIME_DIR` 非空时作为 runtime store 根；否则为 `{tasks_dir}/runtime`，其中 `tasks_dir` = `DEEPSEEK_TASKS_DIR` 或 `~/.deepseek/tasks`。

**SSOT 叙事（D15）：** Thread + Event（`runtime.db`）为执行与回放权威；Session（`sessions.db`）为桌面侧栏 **投影**，经 `runtime_thread_id` 链接，不得独立增长无主 thread 的新会话。

## HTTP 契约（谁写谁读）

| 操作 | 写入 | 读取 |
|------|------|------|
| 侧栏会话列表 | — | `GET /v1/sessions` → SessionManager |
| 保存会话快照 | `POST /v1/threads/{id}/persist-session` | SessionManager（含 **`runtime_thread_id`**） |
| 恢复会话 | `POST /v1/sessions/{id}/resume-thread` | 若 `runtime_thread_id` 且 runtime 有 events → **复用** thread；否则 create + seed |
| 线程 / SSE | `POST /v1/threads`、`/turns`、events | RuntimeThreadStore |

## 链接字段

`SessionMetadata.runtime_thread_id`（SQLite 列 `runtime_thread_id`，D7 C1）指向 `ThreadRecord.id`。桌面 **replay 工具卡 / thinking** 依赖此链接；缺失时 resume 会 seed 新 thread。

## 非 SSOT / 已移除

- **`crates/app-server`** — D7 已删除；生产 HTTP 仅 `runtime_api` / `deepseek-runtime`
- **`deepseek-state` / `core::Runtime`** — D15 已删除；Zagens 唯一入口，无 CLI legacy 路径
