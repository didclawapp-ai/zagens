# D6 详细实施方案 — `deepseek-runtime` sidecar

> **类型：** 实施计划（配套决策 ADR：[D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md)）  
> **状态：** Phase A ✅ · Phase A+ ✅ · **Phase B ✅**（2026-05-26，[D6_PHASE_B_CLI_SUNSET.md](./D6_PHASE_B_CLI_SUNSET.md)）  
> **前置：** D5 M-series（Engine + op loop 在 `deepseek-core`）  
> **SSOT 架构图：** [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)

---

## 0. 目标与边界

### 0.1 要解决的问题

Zagens 桌面嵌入的 sidecar 若使用 `deepseek-tui serve --http`，会**静态链接** ratatui、crossterm、arboard 及整棵 TUI 模块树（`tui/ui.rs` ~7.8k 行等）。桌面产品**从不渲染终端 UI**，却为 sidecar 背负两大 TUI 库与 CLI 表面，导致：

- 二进制体积与冷启动偏大  
- 攻击面与依赖面不必要地宽  
- 架构叙事与「Desktop-only / sidecar-first」不一致  

### 0.2 D6 做什么 / 不做什么

| 做 | 不做 |
|----|------|
| 新建 **`deepseek-runtime`** 瘦二进制（无 ratatui） | 重写 `/v1/*` HTTP 契约 |
| Zagens `externalBin` 切到 `deepseek-runtime-*` | 让 `desktop` crate 直接链 runtime lib |
| **Phase B：** 合并为 **`crates/runtime-server`** 单 crate（lib `deepseek_runtime`） | 在本阶段做 P2 multi-sidecar |
| 删除 `crates/cli`、`crates/tui`、ratatui TUI 树 | 物理删除 `deepseek-state`（core 仍有引用时保留） |

### 0.3 成功标准（Phase A — 已达成；Phase B 后见 §4.4）

```bash
cargo check -p deepseek-runtime-server
cargo tree -p deepseek-runtime-server -i ratatui   # → no match
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
RUSTFLAGS=-Dwarnings cargo check --workspace
```

---

## 1. 目标架构（Phase B 落地后）

```text
┌─────────────────────────────────────────────────────────────┐
│  Zagens (crates/desktop)                                     │
│  externalBin: binaries/deepseek-runtime-<triple>             │
│  sidecar.rs spawn → deepseek-runtime --host … --port …       │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP/SSE + Bearer
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek-runtime  (crates/runtime-server bin)               │
│  deepseek_runtime::runtime_serve::run_from_args              │
└──────────────────────────┬──────────────────────────────────┘
                           │ same crate
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek_runtime lib (crates/runtime-server)                │
│  runtime_api/ · runtime_threads/ · tools/ · core/engine shim   │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
                    deepseek-core (Engine, turn_loop)
```

**已删除：** `crates/cli`、`crates/tui`、ratatui 全屏 TUI。  
**可选遗留：** 若开发机 PATH 上仍有旧 `deepseek-tui` 二进制，`sidecar.rs` 可识别并改用 `serve --http` argv（Zagens 打包不包含）。

---

## 2. Phase A — 二进制拆分（✅ 已完成 · **历史回顾**）

> **注意：** Phase A 实施时 sidecar 仍通过 `deepseek_tui::runtime_serve` 调用 HTTP（源码在 `crates/tui`）。**Phase B 已合并为单 crate 并删除 `crates/tui`** — 以下步骤仅供考古 / code review。

### PR-A1 · Spike + crate 骨架（~0.5 天）

| 步骤 | 动作 | 验收 |
|------|------|------|
| A1.1 | 根 `Cargo.toml` 增加 `crates/runtime-server` member；`default-members` 含 runtime-server | `cargo check -p deepseek-runtime-server` |
| A1.2 | 新建 `crates/runtime-server/Cargo.toml`：`deepseek-tui = { default-features = false, features = ["json","toml"] }` | `cargo tree -i ratatui` 无匹配 |
| A1.3 | `src/main.rs` 调用 `deepseek_tui::runtime_serve::run_from_args` | binary 名 `deepseek-runtime` |

### PR-A2 · `tui-ui` feature 门控（~2–3 天）

**原则：** 凡被 `runtime_api` / `runtime_threads` / `runtime_serve` 引用的类型，要么留在无 feature 模块，要么抽到 **TUI-free** 模块。

| 模块 / 依赖 | 门控方式 | 说明 |
|-------------|----------|------|
| `ratatui`, `crossterm`, `arboard` | `[features] tui-ui = ["dep:ratatui", …]` | `Cargo.toml` optional deps |
| `mod tui`, `mod commands`, `mod palette`, `mod config_ui`, `mod deepseek_theme` | `#[cfg(feature = "tui-ui")]` in `lib.rs` | TUI 专用树 |
| `agent_surface`, `auto_route`, `context_reference` | 始终编译 | 从 TUI 路径抽出，供 HTTP 用 |
| `runtime_serve`, `runtime_api`, `runtime_threads` | 始终 `pub mod` | sidecar 入口 |

**常见编译错误处理顺序：**

1. `cargo check -p deepseek-runtime-server 2>&1 | head` — 找第一个 `use crate::tui::…`  
2. 将类型/函数下沉到 neutral 模块，或加 `#[cfg(feature = "tui-ui")]` 分支  
3. 重复直到 `deepseek-runtime-server` 与 `deepseek-tui --bin deepseek-tui` 双绿  

**验收：**

```bash
cargo check -p deepseek-runtime-server
cargo check -p deepseek-tui --bin deepseek-tui --all-features
```

### PR-A3 · `runtime_serve` 入口（~1 天）

| 步骤 | 文件 | 要点 |
|------|------|------|
| A3.1 | `crates/tui/src/runtime_serve.rs` | 扁平 CLI：`RuntimeServeCli`（`--host`, `--port`, `--cors-origin`, `--auth-token`）；**无** `serve` 子命令 |
| A3.2 | 同上 | 调用现有 `runtime_api::run_http_server` + `RuntimeApiOptions` |
| A3.3 | `crates/tui/src/main.rs` | `Commands::Serve { http: true }` 可委托 `runtime_serve::run`（dev 路径统一） |
| A3.4 | 日志前缀 | stderr 统一 `[deepseek-runtime]`（`runtime_api/mod.rs` 已有） |

**CLI 对照：**

```text
# 生产（Zagens / headless）
deepseek-runtime --host 127.0.0.1 --port 7878 --cors-origin https://tauri.localhost

# dev fallback
deepseek-tui serve --http --host 127.0.0.1 --port 7878 --cors-origin …
```

### PR-A4 · Zagens 打包与监督（~1 天）

| 步骤 | 文件 | 变更 |
|------|------|------|
| A4.1 | `crates/desktop/tauri.conf.json` | `externalBin`: `binaries/deepseek-runtime` |
| A4.2 | `crates/desktop/build.rs` | 复制 `target/.../deepseek-runtime` → `binaries/deepseek-runtime-<triple>` |
| A4.3 | `crates/desktop/scripts/prepare-bundle.mjs` | `cargo build --release -p deepseek-runtime-server` |
| A4.4 | `crates/desktop/src/sidecar.rs` | `runtime_sidecar_cli_args()`：默认 flat argv；`legacy_tui` 分支保留 `serve --http` |
| A4.5 | 二进制发现 | 候选顺序：`deepseek-runtime` → `deepseek-tui` → `deepseek` |

**环境变量（不变）：**

- `DEEPSEEK_RUNTIME_TOKEN` — Bearer  
- `DEEPSEEK_CLIENT_SURFACE=zagens`  
- `DEEPSEEK_API_KEY` — 由 desktop 从 OS keyring 注入  

### PR-A5 · 文档与回归（~0.5 天）

- 更新 [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md)、[D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md)  
- Assessment §1 #5 勾选  
- `cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle`  
- `cargo test -p deepseek-desktop --test architecture_boundary`  

---

## 3. Phase A+ — CI 对真实 binary 验契约（✅ 2026-05-26）

**Phase A+ 已落地。** Lib 测 `sidecar_contract_full_lifecycle` 仍保留（进程内 axum，快）；binary 测 `sidecar_binary_contract` 补 exec 真实 `deepseek-runtime` 路径。

### PR-A+1 · Binary contract test（~1–2 天）

| 步骤 | 动作 |
|------|------|
| 1 | 新增 `crates/runtime-server/tests/sidecar_binary_contract.rs`（或 `tui` integration test gated by env） |
| 2 | `Command::new(env!("CARGO_BIN_EXE_deepseek-runtime"))` spawn；写 `DEEPSEEK_RUNTIME_TOKEN`；解析 stdout `DS_PICK_READY` 取 port |
| 3 | 复用 health → create thread → start turn → SSE subset → interrupt 步骤（与 `runtime_api/tests.rs` 对齐） |
| 4 | CI job 增加：`cargo test -p deepseek-runtime-server --test sidecar_binary_contract` |
| 5 | 可选：Windows/macOS matrix 各跑一条 |

**验收：**

- [x] CI 绿 — `.github/workflows/ci.yml` · `cargo test -p deepseek-runtime-server --test sidecar_binary_contract`
- [x] D6 ADR Acceptance 第 4 项勾选

**实现：** [`crates/runtime-server/tests/sidecar_binary_contract.rs`](../../../crates/runtime-server/tests/sidecar_binary_contract.rs) — spawn `CARGO_BIN_EXE_deepseek-runtime` → 解析 `DS_PICK_READY` → health / thread / turn / SSE / interrupt 全链路。

**非目标：** 替换现有 lib 测 — 保留双轨（lib 快、binary 准）。

---

## 4. Phase B — Runtime 单 crate + CLI/TUI 退场（✅ 2026-05-26）

**决策 ADR：** [D6_PHASE_B_CLI_SUNSET.md](./D6_PHASE_B_CLI_SUNSET.md)（方案 B：直接合并，非 `agent-host` 分叉）。

### 4.1 落地形态

```text
crates/runtime-server/
  Cargo.toml          # package: deepseek-runtime-server
  [lib] name = deepseek_runtime
  [[bin]] name = deepseek-runtime
  src/
    lib.rs            # deepseek_runtime
    main.rs
    runtime_api/      # 自原 tui 迁入
    runtime_threads/
    runtime_serve.rs
    core/engine/      # ~130 LOC shim
    tools/ …
```

**已删除：** `crates/cli/`、`crates/tui/`（含 ratatui TUI 树）。

### 4.2 PR 链（回顾）

| 阶段 | 内容 | 状态 |
|------|------|------|
| B0 | ADR；删 CLI；`transcript_isomorphism` | ✅ |
| B1 | 删 TUI/commands；ratatui 依赖；OpenAPI bin 迁 runtime-server | ✅ |
| B2 | `tui/src/*` → `runtime-server/src/`；删 `crates/tui` | ✅ |
| B3 | CI/文档/验收；`-Dwarnings`；`deepseek-state` 按条件保留 | ✅ |

### 4.3 依赖边界（Phase B 后）

| Crate | 可依赖 |
|-------|--------|
| `runtime-server` lib | `core`, `config`, `protocol`, `tools`, `topic-memory`, …（**无** ratatui） |
| `deepseek-desktop` | **仍只** config + secrets；**不**链 runtime lib |
| `deepseek-state` | 保留于 workspace；**仅** `deepseek-core` 编译依赖；**非** sidecar SSOT |

### 4.4 Phase B 验收

```bash
cargo check --workspace
RUSTFLAGS=-Dwarnings cargo check --workspace
cargo test -p deepseek-runtime-server --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract
cargo tree -p deepseek-runtime-server -i ratatui    # 无匹配
test ! -d crates/cli && test ! -d crates/tui
```

- [x] `runtime_api` / `runtime_threads` 物理路径在 `crates/runtime-server/src/`  
- [x] workspace 无 `crates/cli`、`crates/tui`  
- [x] OpenAPI `export-runtime-openapi` 在 `crates/runtime-server`  
- [x] [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) 更新为单 lib 叙事  
- [x] `deepseek-state`：**未删**（`deepseek-core` 仍有引用 — ADR B3.1 条件不满足）

**实现 commit：** `613a6e3` — *D6 Phase B: merge runtime into single crate and drop CLI/TUI.*

---

## 5. 风险与回滚

| 风险 | 缓解 |
|------|------|
| `tui-ui` 门控漏网，runtime 仍链 ratatui | CI 强制 `cargo tree -p deepseek-runtime-server -i ratatui`；release bundle 脚本 build runtime-server |
| 共享类型仍引用 `crate::tui::` | Phase A spike 列出 call-graph；优先抽 `agent_surface` 模式 |
| 桌面 dev 未 build runtime 导致 Tauri 失败 | `build.rs` 明确 panic 文案 + `npm run bundle:prepare` |
| Phase B 大挪移回归面宽 | 小 PR + 每 PR 跑 sidecar contract + OpenAPI diff |

**回滚 Phase A：** 将 `externalBin` / `build.rs` / `sidecar.rs` 指回 `deepseek-tui-*`；保留 `runtime-server` crate 不删即可。

---

## 6. 人力与排期（参考）

| 阶段 | 工作量 | 状态 |
|------|--------|------|
| **Phase A**（A1–A5） | 2–3 周（含 M-series 前置） | ✅ 2026-05-26 |
| **Phase A+**（binary CI） | 1–2 天 | ✅ 2026-05-26 |
| **Phase B**（单 crate + 删 CLI/TUI） | 2–3 周 | ✅ 2026-05-26 |

Assessment [§5.1](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) 将 D6 定为阶段 A 主线；D6 完成后才推进 D7/D8，避免在「tui 巨壳」内同时改持久化与 OpenAPI。

---

## 7. 相关文件索引

| 关注点 | 路径 |
|--------|------|
| runtime-server binary | `crates/runtime-server/src/main.rs` |
| Sidecar CLI / 入口 | `crates/runtime-server/src/runtime_serve.rs` |
| HTTP server 装配 | `crates/runtime-server/src/runtime_api/mod.rs` |
| TUI-free 共享类型 | `crates/runtime-server/src/agent_surface.rs` · `auto_route.rs` · `context_reference.rs` |
| Desktop 监督 | `crates/desktop/src/sidecar.rs` |
| 打包 | `crates/desktop/build.rs` · `scripts/prepare-bundle.mjs` |
| 契约测（lib） | `crates/runtime-server/src/runtime_api/tests.rs` · `sidecar_contract_full_lifecycle` |
| 契约测（binary） | `crates/runtime-server/tests/sidecar_binary_contract.rs` |
| CI | `.github/workflows/ci.yml` |

---

## 8. 与后续 ADR 的关系

| ADR | 关系 |
|-----|------|
| **D5 M-series** | D6 **前置 blocker** — Engine 在 core 后才值得拆 binary |
| **D7 持久化** | 建议在 D6 Phase A 后做 — runtime 边界已清 |
| **D8 OpenAPI** | 导出 bin 在 `crates/runtime-server`（Phase B ✅） |
| **D11+ P2** | metrics / 多 sidecar 依赖 D6 叙事稳定 |
