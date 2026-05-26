# D4 决策 — `app-server` 实验栈标记废弃

**Status:** Removed (2026-05-26, D7 C5) — was deprecated 2026-05-26  
**Supersedes:** 路线图 §4.2「D4 冻结 app-server」（2026-05-21）— 冻结升级为 **deprecated**  
**Related:** [ARCHITECTURE_ASSESSMENT_2026-05-25.md](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) §1 #6 · [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) · [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md)

## 背景

仓库长期存在两条 HTTP 运行时路径：

| 路径 | 用途 |
|------|------|
| **生产** | `deepseek-tui serve --http` → `runtime_api` (`/v1/*`) → `Engine` |
| **实验** | `deepseek app-server` → `crates/app-server` → `core::Runtime` + `deepseek-state` SQLite |

Zagens / 桌面 **仅**使用生产路径。实验路径与 sidecar 持久化、鉴权、SSE 契约 **不互通**，造成认知与维护成本（Assessment §3.9）。

M-series（D5）已闭合；下一结构优先项为 **D6 `runtime-server`**（从 sidecar 血统抽瘦 binary），**不是**扶正 `app-server`。

## 决策

**`crates/app-server` 及 `deepseek app-server` CLI 子命令标记为 deprecated。**

- **不晋升**为第二套正式 HTTP 运行时。
- **本阶段不删代码** — ~~crate、CLI 入口、依赖保留~~ **已删除（D7 C5）**。
- **`deepseek-state`（`crates/state`）暂不整体废弃** — CLI `thread` 等仍可能读写 `StateStore`；D7 持久化统一时再迁移或收缩 scope。

## 生产路径（不变）

- Zagens / 桌面：`deepseek-tui` sidecar + `runtime_api`
- 长期演进：D6 `crates/runtime-server`（sidecar 去 ratatui，**同一** `/v1/*` 契约）

## 执行（2026-05-26）

| 项 | 动作 |
|----|------|
| 文档 | 本 ADR；Assessment D4 ✅、§1 #6 勾选 |
| CLI | `deepseek app-server` help 标注 DEPRECATED |
| Crate | `deepseek-app-server` crate / `run` / `run_stdio` 文档 + `#[deprecated]` |
| 禁止 | 不新增 app-server 端点、不扩展 turn/API、不让 desktop 依赖 |

## 后续移除（D7 C5 ✅）

1. ~~确认无外部脚本依赖 `deepseek app-server`~~
2. ~~删除 `crates/app-server`、CLI 子命令、workspace 依赖~~ — **done 2026-05-26**
3. `StateStore` 收缩为 CLI legacy（`thread list --source state`）；生产 list 默认 `runtime.db`

## 验收

- [x] 书面决策（本文件）
- [x] Assessment §1 #6 可勾选
- [ ] 代码物理删除（刻意 defer）
