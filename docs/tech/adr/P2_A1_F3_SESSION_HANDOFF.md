# A1 / F3 / P2 ports — 新会话对接（2026-05-23）

> **用途：** 新开 Cursor 窗口继续路线图代码工作时，把本文 + 下方 **§8「复制给 Agent 的提示」** 一并贴上。  
> **权威路线图：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §11–12、§17  
> **门控签收：** [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md)、[P2_G3_ENGINE_L2_SIGNOFF.md](./P2_G3_ENGINE_L2_SIGNOFF.md)  
> **手测清单（可选）：** [G2_PR5_MANUAL_SMOKE_CHECKLIST.md](./G2_PR5_MANUAL_SMOKE_CHECKLIST.md)

---

## 1. 我们在做什么

按 [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) **代码优先**推进：

- **A1 / A+** — 上下文、compaction、持久化、错误 taxonomy（L1）
- **P2** — Engine→core 绞杀者剩余 port（L2 适配留在 tui）
- **F3** — 桌面 a11y（D10 freeze 内允许）
- **PR5** — 生产 sidecar 路径（`RuntimeThreadManager`），**不是** app-server

**硬约束（D4）：** **冻结 `app-server`** — 不扩展 turn/API、不做 hybrid 接线、不让 Zagens 依赖 app-server。

**生产路径（不变）：** Zagens → `deepseek-tui serve --http` → `runtime_api` → `RuntimeThreadManager` → Engine。

---

## 2. Git 锚点

提交本对接文档后，以 **`git log -1`** 为准。上一批主题：

- A1-MVP.1/2、A1.3、A1.5
- A3.4 HTTP `ApiError` envelope
- P2 `lsp_edit_paths`、`SubAgentSpawnPort`
- PR5 `RuntimeThreadMessageTurnPort` + 回归测
- F3 Composer card 修复、Tab 顺序、skip link

---

## 3. 本批已交付（事实清单）

| ID | 内容 | 关键路径 |
|----|------|----------|
| A1.5 | `count_oldest_messages_to_drain` 批量 drain | `crates/core/src/engine/context.rs` |
| A1-MVP.1 | `LargeOutputExternalRef` + `[workshop-ref: …]` | `crates/tui/src/tools/large_output_router.rs` |
| A1-MVP.2 | compaction 保留 working-set pinned 测 | `crates/tui/src/compaction.rs` |
| A1.3 | event/checklist/scratchpad → `spawn_blocking` | `runtime_threads/persist.rs`, `session_store_sqlite.rs`, `thread_store_sqlite.rs` |
| A3.4 | HTTP `ApiError` category/code/recoverable/severity | `crates/tui/src/runtime_api/mod.rs` |
| P2 | `lsp_edit_paths` → core；tui `lsp_hooks` 薄壳 | `crates/core/src/engine/lsp_edit_paths.rs` |
| P2 | `SubAgentSpawnPort::list_subagents` + op-loop 委托 | `subagent_port.rs`, `subagent_spawn.rs`, `op_loop.rs` |
| P2 | `session::{apply_model_selection,is_auto_model_label}` + `session_ops.rs` | `crates/core/src/session.rs`, `engine/session_ops.rs` |
| P2 | `session::apply_sync_session_payload` + `sync_session_from_op` | `session.rs`, `session_ops.rs`, `op_loop.rs` |
| A2 | `events::TurnSummary` — `turn.completed` payload SSOT | `crates/core/src/events.rs`, `runtime_threads/monitor.rs` |
| A2.5 | `turn_streaming` / `turn_tools` `.instrument` spans + turn/monitor events | `turn_loop/run.rs`, `monitor.rs` |
| P2 | `handle_spawn_subagent_op` + session op helpers with status emit | `subagent_spawn.rs`, `session_ops.rs`, `op_loop.rs` |
| P2 | `op_handlers.rs` — cancel/approve/list/change-mode/query-context | `op_handlers.rs`, `op_loop.rs` |
| A2 | `monitor_turn` `TurnSummary` log with `thread_id` | `monitor.rs`, `events.rs` |
| P2 | `truncate_before_last_user_message` + `handle_edit_last_turn` | `session.rs`, `edit_turn_ops.rs` |
| P2 | `compaction_ops.rs` — `Op::CompactContext`; RLM via `handle_rlm_op` | `compaction_ops.rs`, `op_handlers.rs` |
| A2.3 | Turn observability v1 draft (L1 + L2) | `docs/tech/adr/A2_TURN_OBSERVABILITY_V1_DRAFT.md` |
| F3 | Skip link `:focus-visible` | `globals.css` |
| PR5 | `RuntimeThreadMessageTurnPort`（sidecar 真 turn） | `runtime_threads/thread_message_turn_port.rs`, `turn_wait.rs` |
| F3 | Sidebar `role="navigation"` | `Sidebar.tsx` |
| F3 | Composer `#composer-input`、toolbar roles、**DOM Tab 顺序**（input 在 options 前，CSS `order-*` 保视觉） | `Composer.tsx` |
| F3 | Skip link「跳到输入框」 | `SkipToMainLink.tsx`, `globals.css`, i18n `a11y.skipToComposer` |

---

## 4. 明确不做 / 已否决

| 项 | 原因 |
|----|------|
| app-server hybrid（`RuntimeThreadMessageTurnPort` 接到 app-server） | **D4 冻结** |
| Zagens 改走 app-server HTTP | D1/D11 |
| 统一 `StateStore` vs JSONL 持久化 | P2 backlog |
| D10 解冻后的大 GAP（xterm/diff 扩展） | 待维护者 §17.4 评审 |

---

## 5. 建议下一批（路线图优先序）

1. **A1 收尾** — R-015 全量长跑（可选）、A1/A+ 正式签收项对照 §12.1–12.2  
2. **P2 session ops** — `apply_model_selection` in core；`SetModel`/`SyncSession` 经 `session_ops.rs`  
3. **F3 手测** — Tab / skip link / Escape（§G2 清单 §8）  
4. **审批 UI** — `approval_policy` ↔ Composer 已接线；维护者可复测 G2 §2  

**验证命令（新会话先跑）：**

```powershell
cd f:\DeepSeek-TUI-desktop
cargo build -p deepseek-tui --lib
cargo test -p deepseek-tui runtime_thread_message_turn_port --lib
cargo test -p deepseek-core lsp_edit_paths subagent_spawn_error --lib
cd crates\desktop\web-ui && npm run build
```

---

## 6. 易踩坑

- **Composer.tsx** 结构敏感：card 内 premature `</motion.div>` 会破坏 JSX；DOM 重排后须 `npm run build`。
- **app-server** 仅保留既有 `AppServerLlmTurnPort`（单轮 LLM 验证），**勿扩展**  
- **Windows 长跑** 用 **release** binary（debug serve 曾 stack overflow）  
- 勿提交 `deepseek-session-*.json`、`_turn_body.txt`、临时 `scripts/fix_composer_tab_order.py`

---

## 7. 相关文档索引

| 文档 | 用途 |
|------|------|
| [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §17 | 里程碑审核表 |
| [RUNTIME_BASELINE.md](./RUNTIME_BASELINE.md) | R-015 RSS / `-Gate` |
| [P2_MIGRATION_SPIKE.md](./P2_MIGRATION_SPIKE.md) | PR0–PR5 切片 |
| [P2_PR4_SESSION_HANDOFF.md](./P2_PR4_SESSION_HANDOFF.md) | 更早 PR4/engine 拆分上下文 |

---

## 8. 复制给 Agent 的提示

```
继续 DeepSeek-TUI-desktop 路线图代码工作（只写代码，手测可跳过除非我要求）。

必读：
- docs/tech/RUNTIME_EVOLUTION_ROADMAP.md §17
- docs/tech/adr/P2_A1_F3_SESSION_HANDOFF.md（本文件）

约束：
- D4：冻结 app-server，不做 hybrid 接线，不扩展 app-server turn/API
- D10：桌面 freeze — 仅 bugfix / 契约 / a11y（F3）/ runtime 流式修复
- 生产路径：RuntimeThreadManager + runtime_api，不是 app-server

当前 HEAD 已含：A1-MVP/A1.3/A1.5、P2 lsp_edit_paths + SubAgentSpawnPort、PR5 RuntimeThreadMessageTurnPort、F3 Composer a11y。

建议优先：A1 收尾 → P2 SubAgent list port → F3 手测文档化（可选）。

每批结束：cargo test 相关 crate + web-ui build；更新 CHANGELOG [Unreleased]；我未要求时不要 git commit。
```
