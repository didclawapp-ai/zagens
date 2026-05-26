# D6 Phase B Spike — `runtime_api` 物理迁移阻塞分析

> **Status:** **Superseded** — 2026-05-26 采用 [D6_PHASE_B_CLI_SUNSET.md](./D6_PHASE_B_CLI_SUNSET.md) **方案 B**（单 crate 合并，非 `agent-host` 分叉）；Phase B **已落地**。本文保留 spike 结论作历史参照。  
> **Related:** [D6_IMPLEMENTATION_PLAN.md](./D6_IMPLEMENTATION_PLAN.md) §4 · [D6_RUNTIME_SERVER.md](./D6_RUNTIME_SERVER.md)

## 结论

**Phase A + A+ 已满足 D6 生产目标**（sidecar 不链 ratatui + binary 契约测）。  
**Phase B（`runtime_api` / `runtime_threads` 迁入 `runtime-server` lib）不能**在「只搬两个目录 + tui 重导出」的粒度下完成 — 会产生 **`runtime-server` ↔ `deepseek-tui` 循环依赖**，除非同步搬迁或抽取中间 crate。

**建议：** Phase B 拆为 **3–4 个独立 PR**（见 §3），在功能迭代空档做；**不挡**当前产品功能开发。

---

## 1. 当前依赖（Phase A 后）

```text
deepseek-runtime (bin)
  └─ deepseek_tui::runtime_serve
       └─ deepseek_tui::runtime_api / runtime_threads  (仍在 tui crate)
            └─ crate::core::engine, tools, mcp, session_manager, task_manager, …
```

Sidecar **已不链 ratatui**；HTTP 源码仍在 `crates/tui/src/`。

---

## 2.  naive 搬迁为何失败

若将 `runtime_api/` + `runtime_threads/` 移到 `runtime-server/src/`：

| 方向 | 依赖 | 问题 |
|------|------|------|
| `runtime-server` → `tui` | `automation_manager`, `task_manager`, `core::engine`, `tools`, `mcp`, … | 搬出的模块仍 `use crate::automation_manager` |
| `tui` → `runtime-server` | `pub use deepseek_runtime_server::runtime_api` | 与上行形成 **环** |

`runtime_threads/engine_load.rs`  alone 引用：`core::engine`, `tools::plan`, `network_policy`, `localization`, `prompts` 等 **20+** tui 内部模块。

---

## 3. 推荐 PR 链（Phase B 正确打开方式）

| PR | 内容 | 解除的阻塞 |
|----|------|------------|
| **B0** | 新建 `crates/agent-host`（名可议）— 迁入 `automation_manager`, `task_manager`, `session_manager`, `thread_store_sqlite`, `session_store_sqlite` 等 **HTTP+Engine 共用** 模块 | runtime 层不依赖 tui |
| **B1** | `runtime-server` 增 `[lib]`；迁入 `runtime_api/`, `runtime_threads/`, `runtime_serve.rs`；依赖 `agent-host` + `core` + `tools` | HTTP 与 ratatui 解耦 |
| **B2** | `deepseek-tui` 删 `pub mod runtime_*`；`pub use deepseek_runtime_server::*`；TUI 路径改 import | tui 变瘦 |
| **B3** | OpenAPI `export-runtime-openapi` bin 改指向 `runtime-server`；CI / 文档 | D8 对齐 |

**人力：** ~2–3 周（与 [D6_IMPLEMENTATION_PLAN](./D6_IMPLEMENTATION_PLAN.md) §4 一致）。

---

## 4. 与功能迭代的关系

| 项 | Phase A/A+ | Phase B |
|----|------------|---------|
| sidecar 不链 ratatui | ✅ | — |
| 二进制体积 / 冷启动 | ✅ 已改善 | 边际改善有限 |
| 「HTTP 新代码往哪放」 | 仍 `crates/tui/src/runtime_api/` | ✅ 清晰 |
| 新增 `/v1/*` | 遵守 §7.1 OpenAPI 即可 | B 完成后更自然 |

**判定：** 功能迭代前 **必须完成的是 Phase A+**；Phase B 降低长期复杂度，宜在第一个大 HTTP 特性前或空档排期。

---

## 5. 验收（Phase B 全部完成后）

```bash
cargo check -p deepseek-runtime-server
cargo test -p deepseek-runtime-server
cargo tree -p deepseek-runtime-server -i ratatui   # no match
cargo tree -p deepseek-tui -i ratatui            # match (TUI bin only)
cargo test -p deepseek-runtime-server --test sidecar_binary_contract
cargo test -p deepseek-tui --lib sidecar_contract_full_lifecycle
```

- [ ] `runtime_api` 物理路径在 `crates/runtime-server/src/`
- [ ] `deepseek-tui` 不 `pub mod runtime_api`（仅 re-export 或删除）
- [ ] 无 workspace 循环依赖（`cargo tree` / `cargo check --workspace`）
