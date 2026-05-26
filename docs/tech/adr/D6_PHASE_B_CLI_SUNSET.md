# D6 Phase B — 方案 B：Runtime 单 crate + CLI/TUI 退场

> **Status:** Accepted（2026-05-26）  
> **Supersedes:** [D6_PHASE_B_SPIKE.md](./D6_PHASE_B_SPIKE.md) 中的 `agent-host` 分叉路径（改为单 crate 合并）  
> **Related:** [D6_IMPLEMENTATION_PLAN.md](./D6_IMPLEMENTATION_PLAN.md) · [D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md) · [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) · [DEV_NOTES.md](../../desktop/DEV_NOTES.md)

---

## 0. 决策

**在未正式发布前，执行方案 B：**

1. **删除** `crates/cli`（`deepseek` 分发器）与 ratatui 全屏 TUI（`crates/tui/src/tui/`、`commands/`）。
2. **合并** 运行时宿主代码到 **`crates/runtime-server`**（lib + bin `deepseek-runtime`）。
3. **删除** `crates/tui` crate（内容迁入 `runtime-server` 后）。
4. **收缩** `deepseek-state`（CLI legacy；无引用则删）。
5. Zagens **不变**：仍嵌入 `deepseek-runtime`，不链 runtime lib。

**不再保留：** `deepseek-tui` binary、`deepseek serve --http` dev fallback（统一为 `deepseek-runtime`）、`delegate_to_tui` 子进程链。

---

## 1. 动机

| 因素 | 说明 |
|------|------|
| **D6 Phase A+ 后** | sidecar 已瘦，但 **442 条** headless `dead_code` 警告 — 同一 lib 兼 HTTP + TUI |
| **产品** | D12 Desktop-only；CLI/TUI 非用户面（DEV_NOTES D14 升级为 **移除**） |
| **未发布** | 无外部脚本契约负担，适合一次性改 crate 边界 |
| **9s 冷启动** | 部分来自 sidecar 体积/初始化；合并 + 启动路径优化可并行（§6） |

---

## 2. 目标架构

```text
crates/runtime-server/          # package: deepseek-runtime-server
  lib: deepseek_runtime          # 原 tui lib 减 TUI 树
  bin: deepseek-runtime          # 已有

crates/core/                    # 不变
crates/desktop/                 # 不变（仅 sidecar 路径/文档）
crates/config, tools, mcp, …    # 不变

删除:
  crates/cli/
  crates/tui/
  crates/state/                 # 若无剩余引用
```

**依赖方向（无环）：**

```text
desktop → config, secrets
runtime-server (lib) → core, tools, config, protocol, topic-memory, …
runtime-server (bin) → runtime-server (lib)
```

---

## 3. PR 链（按序执行）

### B0 — 准备与 CLI 删除 ✅

| 步骤 | 动作 |
|------|------|
| B0.1 | 本 ADR + CHANGELOG ✅ |
| B0.2 | 从 workspace 移除 `crates/cli` ✅ |
| B0.3 | 抽出 `transcript_isomorphism` ✅ |
| B0.4 | 测试改用 `deepseek_core::approval::ApprovalMode` ✅ |

### B1 — 剥离 TUI 树 ✅

| 步骤 | 动作 |
|------|------|
| B1.1 | 删除 `src/tui/`、`src/commands/`、`src/main.rs` ✅ |
| B1.2 | 删除 `config_ui`、`palette`、`deepseek_theme`、ratatui/crossterm/arboard 依赖 ✅ |
| B1.3 | 清理 `lib.rs`；CLI helpers 保留于 `cli/{doctor,setup,pr_prompt}.rs` ✅ |
| B1.4 | `export-runtime-openapi` bin 迁到 `runtime-server` ✅ |

### B2 — 合并 crate ✅

| 步骤 | 动作 |
|------|------|
| B2.1 | `runtime-server/Cargo.toml` 合并原 `tui` 依赖；增 `[lib] name = deepseek_runtime` ✅ |
| B2.2 | `tui/src/*` → `runtime-server/src/`（含 `assets/`、`tests/`） ✅ |
| B2.3 | bin 调 `deepseek_runtime` lib ✅ |
| B2.4 | 删除 `crates/tui/` ✅ |
| B2.5 | CI/脚本 `-p deepseek-runtime-server`；`deepseek_tui` → `deepseek_runtime`（代码路径） ✅ |

### B3 — 清理与验收

| 步骤 | 动作 |
|------|------|
| B3.1 | 删 `deepseek-state`（若零引用） |
| B3.2 | 更新 CI、`sidecar.rs` legacy 路径、OpenAPI 脚本、文档 |
| B3.3 | 验收命令（§5） |

**人力：** 约 **2–3 周**（1 人）；每 PR 保持 `cargo test -p deepseek-runtime-server` 可回归。

---

## 4. 非目标

- 不改 `/v1/*` HTTP 契约语义  
- 不合并 sidecar 进 Tauri 进程  
- 不在本阶段做 P2 multi-sidecar  

---

## 5. 验收

```bash
cargo check --workspace
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract
cargo tree -p deepseek-runtime-server -i ratatui    # 无匹配
cargo tree -p deepseek-runtime-server -i crossterm  # 无匹配
! test -d crates/cli
! test -d crates/tui
```

- [ ] workspace 无 `crates/cli`、`crates/tui`  
- [ ] `RUNTIME_ARCHITECTURE.md` 仅描述 `deepseek-runtime` 单 lib  
- [ ] Zagens `npm run bundle:prepare` + 冒烟通过  

---

## 6. 冷启动 9s（并行优化，非 Phase B 阻塞）

**假设：** 从点击图标到 Web 主界面可交互 ≈ 9s（维护者手测，2026-05-26）。

Phase B **边际**收益：sidecar 二进制更小、少链无用 code path、去掉 dead_code 编译体积。

**更可能占大头的项（需 profiling）：**

| 阶段 | 可能耗时 | 优化方向 |
|------|----------|----------|
| Tauri/WebView2 进程 + WebView 首屏 | 2–4s | release 构建、资源压缩、减少首屏 JS |
| `initRuntimeConfig` 等 `get_runtime_port` | 阻塞至 `DS_PICK_READY` | sidecar 并行 spawn；skills 安装改后台 |
| Sidecar `RuntimeThreadManager::open` + SQLite | 0.5–2s | WAL、延迟打开非关键 store |
| React hydrate + 首屏 API | 1–3s | 骨架屏、defer 非关键 panel |

**建议：** Phase B 与启动优化 **并行** — B 完成后对 `deepseek-runtime` 做 `tracing` 时间戳（已有 `[deepseek-runtime] bound … (+Duration)`），Desktop 侧记录 `DS_PICK_READY` → 首屏 ready。

---

## 7. 风险

| 风险 | 缓解 |
|------|------|
| 大 PR 回归面宽 | 严格按 B0→B3 拆分；每步契约测 |
| `history_isomorphism` 与 TUI history 耦合 | B0.3 抽出 Message-only 模块 |
| 内部脚本依赖 `deepseek` | 文档改为 `deepseek-runtime` + curl；无发布用户 |
| MIT 上游命名 `deepseek-tui` | `NOTICE.md` / `third-party/` 保留归因 |

---

## 8. DEV_NOTES 修订

- **D14 CLI 定位** → **已移除**（2026-05-26，本 ADR）  
- **D13 Sidecar** → 仅 `deepseek-runtime`；crate 名 `deepseek-runtime-server`  
- ratatui TUI → **删除**（非 freeze）
