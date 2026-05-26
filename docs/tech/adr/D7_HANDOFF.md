# D7 交接 — 持久化统一（新会话读本文件）

**更新：** 2026-05-26  
**SSOT 排期：** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §5.1 阶段 C  
**D7 主 ADR：** [D7_PERSISTENCE_UNIFICATION.md](./D7_PERSISTENCE_UNIFICATION.md)

## 已完成（git）

| Commit | 内容 |
|--------|------|
| `01f2624` | D9/D10 — `turnControl.ts`、SSE owner 过滤 |
| `2c1e86a` | D7 **C1** — `sessions.db` 持久化 `runtime_thread_id`（`session_store_sqlite.rs` + 单元测试） |

## 进行中 / 待做

| PR | 状态 | 说明 |
|----|------|------|
| **C1** | ✅ | `runtime_thread_id` SQLite 列 + ALTER + round-trip |
| **C2** | ⬜ | 路径 / `DEEPSEEK_*` env / 契约表文档 |
| **C3** | ✅ | `session_resume_reuses_runtime_thread_when_sqlite_has_link` |
| **C4–C6** | ⬜ | CLI 适配、StateStore 收缩、§1 #6 签收 |

## 三套存储（生产相关）

| 存储 | 路径 | SSOT 职责 |
|------|------|-----------|
| SessionManager | `~/.deepseek/sessions/sessions.db` | SavedSession、`runtime_thread_id` 外键 |
| RuntimeThreadStore | `~/.deepseek/tasks/runtime/runtime.db` | 线程 / turn / 事件 JSONL 或 SQLite |
| StateStore (CLI) | `~/.deepseek/state.db` | **非** sidecar HTTP；D7 末段收缩 |

**关键代码：**

- `crates/tui/src/session_store_sqlite.rs` — C1
- `crates/tui/src/runtime_api/sessions.rs` — `resume_session_thread`（约 205–233 行复用 `runtime_thread_id`）
- `crates/tui/src/runtime_api/threads.rs` — `persist_thread_session`
- `crates/tui/src/runtime_api/tests.rs` — 现有 `session_resume_thread_*` 测例旁加 C3

## C3 测试意图

1. `spawn_test_server_with_root` → 自动 `sessions.db`（勿手写 `{id}.json`）
2. `POST /v1/threads` → `seed_thread_from_messages`（或 turn）→ 有 events
3. `POST /v1/threads/{id}/persist-session` → 写入 `runtime_thread_id`
4. `SessionManager::load_session` 断言 `runtime_thread_id` 非空
5. `POST /v1/sessions/{id}/resume-thread` → **`200` + `state: ready` + 同一 `thread_id`**（非 `202` seeding）

## 工作区未提交

- `.gitignore` — `.docs/topic-memory-graph-main/`
- `docs/Agent+Harness组合式编程方案.md` — 与 D7 无关

## 命令

```bash
cargo test -p deepseek-tui --lib session_store_sqlite::tests
cargo test -p deepseek-tui session_resume_reuses_runtime_thread_when_sqlite_has_link
cd crates/desktop/web-ui && npm run build
```

## §1 进度

仍为 **7/10**；D7 勾选 #6 需 **C6** 架构 owner 签收（非单 PR）。
