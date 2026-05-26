# D6 详细实施方案 — `deepseek-runtime` sidecar

> **类型：** 实施计划（配套决策 ADR：[D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md)）  
> **状态：** Phase A ✅ 已落地（2026-05-26）· Phase A+ / B 待办  
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
| 用 **`tui-ui` feature** 门控 TUI 栈 | 删除 `deepseek-tui serve --http`（dev fallback 保留） |
| Zagens `externalBin` 切到 `deepseek-runtime-*` | Phase A 内物理搬迁 `runtime_api/*`（属 Phase B） |
| 复用 `runtime_api` + `runtime_threads` 同一实现 | 让 `desktop` crate 直接 `use deepseek_tui` |

### 0.3 成功标准（Phase A — 已达成）

```bash
cargo check -p deepseek-runtime-server
cargo tree -p deepseek-runtime-server -i ratatui   # → no match
cargo check -p deepseek-tui --bin deepseek-tui     # 全量 TUI 仍绿
cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle
```

---

## 1. 目标架构（实施后）

```text
┌─────────────────────────────────────────────────────────────┐
│  Zagens (crates/desktop)                                     │
│  externalBin: binaries/deepseek-runtime-<triple>             │
│  sidecar.rs spawn → deepseek-runtime --host … --port …       │
└──────────────────────────┬──────────────────────────────────┘
                           │ HTTP/SSE + Bearer
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek-runtime  (crates/runtime-server, ~10 LOC main)     │
│  deepseek_tui::runtime_serve::run_from_args                  │
└──────────────────────────┬──────────────────────────────────┘
                           │ deepseek-tui lib, default-features=false
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  deepseek-tui lib (无 tui-ui)                                │
│  runtime_api/ · runtime_threads/ · tools/ · core/engine shim │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
                    deepseek-core (Engine, turn_loop)
```

**dev fallback：** `sidecar.rs` 检测到二进制名含 `deepseek-tui` 时，argv 仍用 `serve --http --host … --port …`。

---

## 2. Phase A — 二进制拆分（✅ 已完成）

> 下列步骤为**回顾性清单**，便于复现或 code review；commit 参考 `SESSION_HANDOFF_2026-05-26.md`（`a1aacbe`）。

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

## 4. Phase B — 源码物理迁入 `runtime-server` lib（可选 · 见 spike）

**阻塞分析：** [D6_PHASE_B_SPIKE.md](./D6_PHASE_B_SPIKE.md) — 不能只搬 `runtime_api/` 两目录；需 **B0 `agent-host` 中间 crate** 打破循环依赖。

**不阻塞：** 架构定型 §1 #5；可在 P2 功能迭代间隙做。

### 4.1 目标 crate 形态

```text
crates/runtime-server/
  Cargo.toml          # lib + bin
  src/
    lib.rs            # pub mod runtime_api; pub mod runtime_threads; pub mod runtime_serve;
    main.rs           # 薄入口
    runtime_api/      # 从 tui 迁入
    runtime_threads/  # 从 tui 迁入
    runtime_serve.rs
```

`deepseek-tui` 保留：

- `mod tui`（freeze）  
- `tools/`、`core/engine` shim、`commands`（CLI）  
- 对 `deepseek-runtime-server` 的 **可选** 重导出（过渡期）

### 4.2 推荐 PR 链（~2–3 周，1 人）

| 顺序 | PR | 内容 | 风险 |
|------|-----|------|------|
| B1 | Spike | `runtime-server` 增 `[lib]`；`tui` 改为 `pub use deepseek_runtime_server::runtime_api` 重导出 | 低 |
| B2 | 迁 `runtime_api` | 移动目录 + 修正 `use` 路径；OpenAPI export bin 改指向新 crate | 中 |
| B3 | 迁 `runtime_threads` | 与 B2 同 PR 或紧接；持久化 monitor 测全绿 | 中 |
| B4 | 迁 `runtime_serve` | CLI 入口留在 runtime-server；tui `serve --http` 调用 `deepseek_runtime_server::runtime_serve` | 低 |
| B5 | 清理 | 删 tui 侧空壳；`deepseek-tui` 依赖 `deepseek-runtime-server` lib（仅 dev serve 需要） | 中 |

### 4.3 依赖边界规则（Phase B）

| Crate | 可依赖 |
|-------|--------|
| `runtime-server` lib | `core`, `config`, `protocol`, `tools`, `topic-memory`, …（**无** ratatui） |
| `deepseek-tui` bin（TUI） | `runtime-server` lib + `tui-ui` + 全工具栈 |
| `deepseek-desktop` | **仍只** config + secrets；**不**链 runtime-server lib |

### 4.4 Phase B 验收

```bash
cargo check -p deepseek-runtime-server
cargo test -p deepseek-runtime-server
cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle
cargo test -p deepseek-runtime-server --test sidecar_binary_contract
npm run test:f3   # web-ui 冒烟
```

- [ ] `runtime_api` / `runtime_threads` 物理路径在 `crates/runtime-server/src/`  
- [ ] `deepseek-tui` 不再 `pub mod runtime_api`（仅 re-export 或删除）  
- [ ] OpenAPI `export-runtime-openapi` 从新 crate 导出  
- [ ] 文档 [RUNTIME_ARCHITECTURE.md](../RUNTIME_ARCHITECTURE.md) §2/§3 更新  

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
| **Phase A+**（binary CI） | 1–2 天 | 待办 |
| **Phase B**（物理迁移） | 2–3 周 | 可选 / P2 间隙 |

Assessment [§5.1](./ARCHITECTURE_ASSESSMENT_2026-05-25.md) 将 D6 定为阶段 A 主线；D6 完成后才推进 D7/D8，避免在「tui 巨壳」内同时改持久化与 OpenAPI。

---

## 7. 相关文件索引

| 关注点 | 路径 |
|--------|------|
| runtime-server binary | `crates/runtime-server/src/main.rs` |
| Sidecar CLI / 入口 | `crates/tui/src/runtime_serve.rs` |
| HTTP server 装配 | `crates/tui/src/runtime_api/mod.rs` |
| Feature 门控 | `crates/tui/Cargo.toml` · `crates/tui/src/lib.rs` |
| TUI-free 共享类型 | `crates/tui/src/agent_surface.rs` · `auto_route.rs` · `context_reference.rs` |
| Desktop 监督 | `crates/desktop/src/sidecar.rs` |
| 打包 | `crates/desktop/build.rs` · `scripts/prepare-bundle.mjs` |
| 契约测（lib） | `crates/tui/src/runtime_api/tests.rs` · `sidecar_contract_full_lifecycle` |
| CI | `.github/workflows/ci.yml` |

---

## 8. 与后续 ADR 的关系

| ADR | 关系 |
|-----|------|
| **D5 M-series** | D6 **前置 blocker** — Engine 在 core 后才值得拆 binary |
| **D7 持久化** | 建议在 D6 Phase A 后做 — runtime 边界已清 |
| **D8 OpenAPI** | 导出 bin 暂在 tui；Phase B 后迁到 runtime-server |
| **D11+ P2** | metrics / 多 sidecar 依赖 D6 叙事稳定 |
