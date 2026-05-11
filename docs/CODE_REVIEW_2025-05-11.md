# DeepSeek-TUI-desktop 代码审核报告

> **审核日期**: 2025-05-11
> **项目版本**: 0.8.15
> **审核范围**: 全 workspace（15 个 crate + 桌面端前端）

---

## 目录

1. [项目概览](#一项目概览)
2. [总体评分](#二总体评分)
3. [架构设计](#三架构设计)
4. [代码质量与可维护性](#四代码质量与可维护性)
5. [安全性](#五安全性)
6. [错误处理](#六错误处理)
7. [测试覆盖与 CI](#七测试覆盖与-ci)
8. [依赖管理](#八依赖管理)
9. [关键发现（按优先级）](#九关键发现按优先级)
10. [改进建议路线图](#十改进建议路线图)

---

## 一、项目概览

| 维度 | 数据 |
|------|------|
| Workspace 版本 | 0.8.15 |
| Crate 数量 | 15 个 |
| Rust Edition | 2024 (MSRV 1.88) |
| 核心代码量 | `crates/tui` 约 50+ 模块、30+ 工具、60+ 源文件 |
| 桌面端 | Tauri 2 + React 18 + TypeScript + TailwindCSS |
| CI | 三平台矩阵 (Ubuntu/macOS/Windows) |
| 仓库 | [https://github.com/Hmbown/DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI) |

### 1.1 Workspace 成员

| Crate | 描述 | 代码量评估 | 测试覆盖 |
|-------|------|-----------|---------|
| `tui` | 核心 TUI 应用（引擎、客户端、工具、沙箱） | 极重（45+ 依赖） | 优秀（引擎 76 测试） |
| `cli` | CLI 入口（deepseek 二进制） | 轻 | 中等 |
| `desktop` | Tauri 桌面端（DS Pick） | 中 | 极薄 |
| `config` | 配置管理 | 重（2165 行） | 中等 |
| `core` | 核心抽象 | 重（1698 行） | 薄弱 |
| `agent` | Agent 抽象 | 轻 | 中等 |
| `protocol` | 协议层 | 轻 | 薄弱（3 测试） |
| `state` | 状态管理 | 轻 | 薄弱（1 测试） |
| `tools` | 工具系统 | 轻 | 薄弱（5 测试） |
| `secrets` | 密钥管理 | 轻 | 中等 |
| `execpolicy` | 执行策略 | 轻 | 中等 |
| `mcp` | MCP 协议（桩实现） | 轻 | 薄弱 |
| `hooks` | 钩子系统 | 轻 | 薄弱 |
| `tui-core` | TUI 核心抽象 | 极轻 | 薄弱 |
| `app-server` | 应用服务器 | 轻 | 无 |

---

## 二、总体评分

| 评估维度 | 评分 | 简评 |
|----------|------|------|
| **架构设计** | ⭐⭐⭐⭐ (4/5) | Workspace 拆分合理，职责边界清晰；但 tui crate 承担过多职责，"上帝对象"问题突出 |
| **代码质量** | ⭐⭐⭐ (3/5) | 核心逻辑健壮，流式处理、工具系统质量高；但巨型文件（>150KB）严重损害可维护性 |
| **安全性** | ⭐⭐⭐ (3/5) | macOS Seatbelt 沙箱实现完整；**Linux/Windows 沙箱为空壳**，是重大安全缺口 |
| **错误处理** | ⭐⭐⭐⭐ (4/5) | `ErrorEnvelope` 分层设计清晰；但启发式错误分类存在误分类风险 |
| **测试覆盖** | ⭐⭐⭐ (3/5) | 核心引擎测试优秀；但 Engine 不可 Mock 阻塞端到端测试，基础 crate 测试极薄 |
| **依赖管理** | ⭐⭐⭐ (3/5) | 14/15 crate 规范使用 workspace 继承；**tui 有 17 个依赖未使用 workspace** |
| **CI/CD** | ⭐⭐⭐⭐ (4/5) | 三平台覆盖、发布门控完善；缺少安全审计和代码覆盖率 |

### 综合评分：⭐⭐⭐½ (3.5/5)

功能完整度优秀，架构方向正确，但需要系统性瘦身和安全加固。

---

## 三、架构设计

### 3.1 优点

#### ✅ Workspace 拆分合理

15 个 crate 按职责清晰分层：

```
协议层:     protocol, state, tools, core
基础设施层:  config, secrets, execpolicy, hooks
业务逻辑层:  tui, agent, mcp
入口层:     cli, desktop, app-server, tui-core
```

#### ✅ 沙箱策略梯度设计清晰

[`SandboxPolicy`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs) 枚举从 `DangerFullAccess` → `WorkspaceWrite` → `ReadOnly` 形成清晰的安全梯度，macOS Seatbelt 实现完整且细致。

#### ✅ 引擎-UI 通信设计合理

[`EngineHandle`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs) 使用 `mpsc` 通道 + `CancellationToken` 组合，实现了引擎与 UI 的异步解耦。

#### ✅ 模块化裁剪初见成效

`App` 结构体字段已按关注点分组为子结构体（`ComposerState`、`ViewportState`、`GoalState`、`SessionState`），注释标注 #377 表明这是有意识的重构方向。

### 3.2 问题

#### ❌ tui crate — 上帝对象

`Engine` 结构体包含约 25 个字段，涵盖客户端、会话、子代理、MCP、容量控制、LSP、沙箱后端等多个关注点。`EngineConfig` 有约 20 个配置字段。

#### ❌ 主入口未创建 lib.rs

`crates/tui/src/lib.rs` 不存在。该 crate 是纯二进制 crate（没有 `lib` target），导致：
- 无法进行 crate 级别的集成测试
- 无法被其他 crate 作为库引用
- 模块边界模糊

#### ❌ MCP 层默认客户端使用桩数据

MCP 层架构完整（[`McpManager`](file:///f:/DeepSeek-TUI-desktop/crates/mcp/src/lib.rs)、`McpManagedClient` trait、JSON-RPC stdio 服务端已就位），但 `default_stdio_client` 使用 [`InMemoryMcpClient`](file:///f:/DeepSeek-TUI-desktop/crates/mcp/src/lib.rs) 提供固定桩工具（health/capabilities），尚未对接真实 MCP 服务器子进程。缺失的是 stdio 子进程启动和 MCP 协议握手实现。

---

## 四、代码质量与可维护性

### 4.1 巨型文件清单

| 文件 | 大小/行数 | 问题 |
|------|----------|------|
| [crates/tui/src/main.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/main.rs) | >180KB | CLI 定义 + 子命令实现全内联 |
| [crates/tui/src/tui/app.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tui/app.rs) | >176KB | 渲染 + 事件 + 状态全集中 |
| [crates/config/src/lib.rs](file:///f:/DeepSeek-TUI-desktop/crates/config/src/lib.rs) | ~2165 行 | 配置管理全量集中 |
| [crates/core/src/lib.rs](file:///f:/DeepSeek-TUI-desktop/crates/core/src/lib.rs) | ~1698 行 | 核心抽象全量集中 |
| [crates/tui/src/core/engine/turn_loop.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine/turn_loop.rs) | ~1909 行 | `handle_deepseek_turn` 函数认知复杂度极高 |
| [crates/tui/src/core/engine.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs) | ~2020 行 | 引擎主体（已拆分子模块，仍偏大） |
| [crates/tui/src/client/chat.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/client/chat.rs) | ~1604 行 | SSE 解析 + 消息构建 |
| [crates/desktop/web-ui/src/App.tsx](file:///f:/DeepSeek-TUI-desktop/crates/desktop/web-ui/src/App.tsx) | ~1326 行 | 前端入口臃肿 |

### 4.2 代码复杂度热点

#### 🔴 `handle_deepseek_turn` — 认知复杂度极高

[turn_loop.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine/turn_loop.rs) 中 `handle_deepseek_turn` 函数约 800 行，包含：

- 外层 `loop`（步骤循环）
- 内层 `loop`（流事件循环）
- `tokio::select!` 多分支
- 工具并行执行（`FuturesUnordered`）
- 顺序工具执行
- 审批流程
- 上下文溢出恢复
- 透明流重试
- 子代理完成等待
- REPL 内联块执行

任何修改都需要理解所有交互路径。

#### 🔴 `handle_send_message` — 参数过多

签名有 11 个参数，虽然标注了 `#[allow(clippy::too_many_arguments)]`，但强烈暗示需要引入参数结构体。

#### 🔴 `build_chat_messages_with_reasoning` — 孤儿工具调用清理逻辑复杂

约 100 行的反向遍历删除逻辑（第 574-676 行），虽然在压缩后清理孤立的 `tool_calls`/`tool_results` 是必要的，但正确性很难验证。

### 4.3 代码异味

#### `Op::EditLastTurn` 语义不一致

[engine.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs) 中注释说删除"最后一个用户+助手交换"，但实现使用 `truncate(idx)` 删除"最后一个用户消息及之后所有内容"——多轮对话中可能意外删除后续轮次。

#### `trim_oldest_messages_to_budget` 使用 `Vec::remove(0)`

[engine.rs:1292](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs#L1292) 从 Vec 头部删除元素，时间复杂度 O(n)。长会话中性能退化明显。建议使用 `VecDeque`。

#### `has_full_disk_read_access()` 始终返回 `true`

[policy.rs:106](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/policy.rs#L106) 是关联函数而非方法，签名暗示可能因策略而异，但硬编码为 `true`。调用者可能已依赖此行为。

#### 浮点数成本追踪精度损失

`SessionState::session_cost: f64` 累加存在精度损失，长时间会话中可能导致显示不准确。建议使用整数（以最小货币单位计）或定点数。

#### Windows UTF-8 配置缺少安全注释

[main.rs:83](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/main.rs#L83) 中 `unsafe { SetConsoleCP(CP_UTF8); SetConsoleOutputCP(CP_UTF8); }` 缺少 `// SAFETY:` 注释。

---

## 五、安全性

### 5.1 严重问题

#### 🔴 Linux Landlock 沙箱为空壳

[`prepare_landlock`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs#L395-L419) 仅设置环境变量标记，**未实际应用任何 Landlock 规则**。注释说明需要 helper binary，但当前实现不提供任何实际隔离。

```rust
// 实际代码 — 仅设置环境变量
fn prepare_landlock(cmd: &mut Command, _policy: &SandboxPolicy) {
    cmd.env("__DEEPSEEK_LANDLOCK_MODE", "1");
    // 无实际 Landlock 规则应用
}
```

#### 🔴 Windows 沙箱为空壳

[`prepare_windows_sandbox`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs#L426-L448) 同样仅设置环境变量，无实际隔离。

**风险**: AI 生成的命令（如 `rm -rf`）在 Linux/Windows 上可无限制执行，与用户对"沙箱保护"的预期严重不符。

### 5.2 其他安全考量

| 风险 | 严重度 | 位置 | 说明 |
|------|--------|------|------|
| Linux/Windows 沙箱为空壳 | **高** | sandbox/mod.rs | 用户预期保护但实际无隔离 |
| `classify_error_message` 启发式误分类 | 中 | error_taxonomy.rs | 可能导致错误恢复策略不当 |
| `ToolError::PathEscape` 暴露路径 | 低 | error_taxonomy.rs | 路径信息在错误消息中暴露 |
| `DangerFullAccess` 沙箱策略 | 低 | engine.rs | 仅在 YOLO 模式下使用 |
| SSE 缓冲区 10MB 上限 | 低 | client/chat.rs | 已有保护 |

### 5.3 安全亮点

- **macOS Seatbelt 实现完整**: 使用 `sandbox-exec` 和 Seatbelt profile 实现实际的沙箱隔离
- **命令执行策略梯度清晰**: `SandboxPolicy` 枚举设计清晰，默认值为安全的 `WorkspaceWrite { network_access: false }`
- **技能安装安全边界检查完善**: 路径穿越、符号链接、大小限制、网络策略等安全边界全覆盖
- **使用 `rustls` 而非 `native-tls`/`openssl`**: 减少系统级 TLS 依赖的攻击面

---

## 六、错误处理

### 6.1 设计评价

#### ✅ `ErrorEnvelope` 分层设计

错误处理采用清晰的三层架构：

```
底层错误 → ErrorCategory + ErrorSeverity → ErrorEnvelope → UI 事件
```

- [`ErrorCategory`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs): 10 个变体，覆盖 Network、Auth、Tool、State 等类别
- [`ErrorSeverity`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs): 4 级（Error/Warning/Info/Debug），支持过滤
- [`ErrorEnvelope`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs): 统一信封，携带类别、严重度、消息、上下文

#### ✅ 流式错误处理细致

[client/chat.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/client/chat.rs) 中：

- 区分连接超时（`stream_open_timeout`）和空闲超时（`stream_idle_timeout`）
- 流读取错误遍历 `Error::source()` 链以获取底层原因
- 环境变量读取超时值后做 `clamp` 限制（5-300s 和 1-3600s）
- 限制错误响应体大小防止内存溢出

#### ✅ 引擎错误防御

`reset_cancel_token` 正确处理 `StdMutex` 中毒，避免因 panic 导致引擎不可用。

### 6.2 问题

#### ❌ 启发式错误分类

[`classify_error_message`](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs#L295-L356) 基于关键字匹配：

```rust
fn classify_error_message(msg: &str) -> ErrorCategory {
    if msg.contains("502") { return ErrorCategory::Network; }
    if msg.contains("not found") { return ErrorCategory::State; }
    // ...
}
```

- `"502"` 作为独立字符串匹配过于宽泛
- 匹配顺序有优先级效应（如 `"tool"` 关键字会匹配包含 "tool" 的任何消息）
- 建议增加单元测试覆盖边界情况，或逐步替换为结构化错误类型

#### ❌ `ErrorEnvelope` 的 `recoverable` 语义不明确

`ErrorEnvelope::recoverable` 是布尔值，缺乏关于如何恢复的指导信息。建议改为枚举：

```rust
enum Recoverability {
    Retryable { hint: String, max_retries: u8 },
    Fatal { fallback: Option<String> },
}
```

#### ❌ `ErrorEnvelope` 实现 `Clone`

对于可能包含大量上下文的错误信封，`Clone` 可能导致不必要的内存开销。考虑使用 `Arc<ErrorEnvelope>`。

---

## 七、测试覆盖与 CI

### 7.1 测试覆盖矩阵

| Crate | 测试数（约） | 评估 |
|-------|-------------|------|
| **tui** | ~655+ | **优秀** — 核心引擎、子代理、shell 测试质量高 |
| ├─ engine/tests.rs | 76 | 回归驱动，覆盖认证、并行批处理、容量门控等关键路径 |
| ├─ subagent/tests.rs | ~60 | spawn/assign、取消级联、邮箱传播全覆盖 |
| ├─ shell/tests.rs | ~17 | 跨平台兼容（Windows/Unix 条件编译） |
| ├─ protocol_recovery.rs | 7 | 通过 `include_str!` 编译时验证 |
| ├─ palette_audit.rs | 4 | WCAG 对比度守卫（独特价值） |
| ├─ skill_install.rs | 11 | **安全测试优秀** — 路径穿越、符号链、大小限制全覆盖 |
| ├─ eval_harness.rs | 4 | 离线评估 |
| └─ integration_mock_llm.rs | 9+4[ignore] | Mock LLM trait 级别测试 |
| **cli** | ~50 | 中等 |
| **config** | ~36 | 中等 |
| **agent** | ~19 | 中等 |
| **secrets** | ~17 | 中等 |
| **execpolicy** | ~27 | 中等 |
| **protocol** | 3 | **薄弱** — 仅序列化往返 |
| **state** | 1 | **薄弱** — 仅基本 CRUD |
| **tools** | 5 | **薄弱** — 仅基本分派 |
| **desktop** | 1 | **几乎无测试** |
| **app-server** | 0 | **完全无测试** |
| **core** | 少量 | 薄弱 |
| **hooks** | 少量 | 薄弱 |
| **mcp** | 0 | **无测试** |
| **tui-core** | 1 | 薄弱 |

### 7.2 测试质量评价

#### ✅ 亮点

- **回归测试驱动**: 大量测试关联具体 issue 编号（#103, #263, #273, #402, #404 等）
- **安全边界测试**: 路径穿越、符号链接、大小限制、网络策略等安全关键路径覆盖充分
- **跨平台兼容**: `shell/tests.rs` 使用 `#[cfg(windows)]` 条件编译
- **常量钉住**: 关键常量（`MAX_TRANSPARENT_STREAM_RETRIES` 等）有专门回归测试
- **并发安全**: 使用 `lock_test_env()` 互斥锁保护环境变量修改

#### ❌ 不足

- **Engine 不可 Mock**: `Engine` 持有具体 `DeepSeekClient` 而非 `Arc<dyn LlmClient>`，4 个端到端测试被 `#[ignore]`
- **基础 crate 测试极薄**: `protocol`/`state`/`tools` 各仅 1-3 个测试
- **无 UI 渲染测试**: TUI 组件视觉输出没有 snapshot 测试
- **无性能回归测试**: 没有基准测试或性能门控
- **缺少错误路径覆盖**: 许多测试只验证 happy path

### 7.3 CI 流程

| Job | 触发 | 功能 | 评估 |
|-----|------|------|------|
| `versions` | 全触发 | 版本漂移检查 | ✅ |
| `lint` | 全触发 | `cargo fmt --check` + `clippy -D warnings` | ✅ |
| `test` | 全触发 | 三平台 `cargo test --all-features --locked` | ✅ |
| `npm-wrapper-smoke` | 非定时 | 构建二进制 + 验证 npm wrapper | ✅ |
| `docs` | 仅定时 | `cargo doc --no-deps -D warnings` | ⚠️ 仅定时运行 |
| `parity` (release) | tag 推送 | 全量测试复现 | ✅ 发布门控完善 |

#### CI 缺失项

| 缺失 | 影响 |
|------|------|
| `cargo audit`（安全审计） | 无法检测已知漏洞依赖 |
| 代码覆盖率收集 | 无法量化覆盖率 |
| MSRV 验证 | 未显式验证 `rust-version = "1.88"` |
| 代码签名 | macOS/Windows 二进制无签名 |
| 版本一致性验证 | 未验证 tag 版本与 Cargo.toml 一致 |

---

## 八、依赖管理

### 8.1 严重问题

#### 🔴 tui crate 17 个依赖未使用 workspace 继承

[tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) 中以下依赖硬编码了版本号而非使用 `workspace = true`：

| 依赖 | tui 硬编码 | workspace 版本 | 风险 |
|------|-----------|---------------|------|
| **axum** | **"0.8.4"** | **0.8.5** | 🔴 版本冲突，可能引入两个版本 |
| **async-trait** | **"0.1"** | **0.1.89** | 🟡 可能解析到更旧补丁版本 |
| **chrono** | **"0.4"** | **0.4.43** | 🟡 可能解析到更旧补丁版本 |
| anyhow | "1.0.100" | 1.0.100 | 版本一致，未用 workspace |
| serde | "1.0.228" | 1.0.228 | 版本一致，未用 workspace |
| tokio | "1.49.0" | 1.49.0 | 版本一致，未用 workspace |
| reqwest | "0.13.1" | 0.13.1 | features 是超集 |
| 其余 10 个 | 同 workspace | — | 版本一致，未用 workspace |

#### 🔴 所有 crate 缺少 `rust-version.workspace = true`

15 个 crate 均未继承 workspace 的 `rust-version = "1.88"`，MSRV 约束在 crate 级别未生效。

### 8.2 中等问题

#### 🟡 `base64` 版本不一致

- [tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml): `base64 = "0.22.1"`
- [desktop/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/desktop/Cargo.toml): `base64 = "0.22"`

#### 🟡 多 crate 共用依赖未纳入 workspace

`tempfile`（tui, cli, secrets）、`ignore`（tui, desktop）、`base64`（tui, desktop）被多个 crate 使用但未在 workspace.dependencies 中统一管理。

#### 🟡 Workspace 版本约束风格不统一

混用精确补丁版本（`"1.0.100"`）和主版本+次版本（`"2.0"`、`"0.10"`）。

### 8.3 tui crate 依赖膨胀

| 依赖 | 建议 |
|------|------|
| `starlark` | 建议改为可选 feature |
| `pdf-extract` | 建议改为可选 feature |
| `rust_xlsxwriter` | 建议改为可选 feature |
| `zip` | 建议改为可选 feature |
| `tar` + `flate2` | 建议改为可选 feature |
| `portable-pty` | 建议改为可选 feature |
| `tiny_http` | 与 axum 功能重叠，确认是否仍需保留 |
| `image` | 建议改为可选 feature |

### 8.4 合规性总览

| Crate | version | edition | license | repository | rust-version | 依赖使用 workspace |
|-------|---------|---------|---------|------------|-------------|-------------------|
| **tui** | ✅ | ✅ | ✅ | ✅ | ❌ | **❌ 17 个未用** |
| cli | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| desktop | 独立版本 | ✅ | ✅ | ✅ | ❌ | ✅ |
| agent | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| app-server | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| config | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| core | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| execpolicy | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| hooks | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| mcp | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| protocol | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| secrets | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| state | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| tools | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| tui-core | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |

---

## 九、关键发现（按优先级）

### P0 — 紧急

| # | 发现 | 影响 | 涉及文件 |
|---|------|------|---------|
| 1 | Linux/Windows 沙箱为空壳 | 用户预期沙箱保护但实际无隔离，安全缺口 | [sandbox/mod.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs#L395-L448) |
| 2 | tui crate axum `"0.8.4"` vs workspace `"0.8.5"` 版本冲突 | 可能引入两个不同版本的 axum，编译不确定性 | [tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) |
| 3 | `Op::EditLastTurn` 语义不一致 | 多轮对话中可能意外删除后续轮次 | [engine.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs#L770-L779) |

### P1 — 重要

| # | 发现 | 影响 | 涉及文件 |
|---|------|------|---------|
| 4 | 巨型文件（6 个 >150KB/1500 行） | 认知负荷极高，维护成本随功能增长线性上升 | main.rs, app.rs, config/src/lib.rs, core/src/lib.rs, turn_loop.rs, engine.rs |
| 5 | Engine 不可 Mock | 4 个端到端测试被阻塞，关键路径无端到端验证 | [engine.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs) |
| 6 | `handle_deepseek_turn` 800 行 | 认知复杂度极高，修改风险大 | [turn_loop.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine/turn_loop.rs) |
| 7 | 17 个依赖未使用 workspace 继承 | 版本管理隐患，维护负担 | [tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) |
| 8 | 所有 crate 缺少 `rust-version.workspace = true` | MSRV 约束未在 crate 级别生效 | 全部 15 个 Cargo.toml |
| 9 | 基础 crate 测试极薄（protocol/state/tools） | 无法有效防止回归 | 各 crate tests/ 目录 |
| 10 | `classify_error_message` 启发式误分类 | 错误恢复策略可能不当 | [error_taxonomy.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs#L295) |

### P2 — 改善

| # | 发现 | 影响 | 涉及文件 |
|---|------|------|---------|
| 11 | `base64` 版本不一致 | 可能引入两个 base64 版本 | tui/Cargo.toml, desktop/Cargo.toml |
| 12 | `Vec::remove(0)` O(n) 操作 | 长会话性能退化 | [engine.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs#L1292) |
| 13 | 浮点数成本追踪精度损失 | 长时间会话显示不准确 | [app.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tui/app.rs) |
| 14 | 缺少 lib.rs（纯二进制 crate） | 无法 crate 级集成测试 | crates/tui/src/ |
| 15 | workspace 版本约束风格不统一 | 解析不确定性 | Cargo.toml |
| 16 | 缺少 `cargo audit`、覆盖率收集 | CI 安全/质量盲区 | .github/workflows/ci.yml |
| 17 | 缺少代码签名 | 用户信任度受损 | 发布流程 |
| 18 | tui 依赖膨胀（可选项） | 编译时间和二进制体积 | tui/Cargo.toml |

---

## 十、改进建议路线图

### 概览

```
Step 1 (紧急)    安全 + 依赖修复 ── 4 项，预计 1-2 天，零破坏
Step 2 (高优先级) 架构瘦身一阶段   ── 4 项，预计 3-5 天，文件结构变动
Step 3 (高优先级) 架构瘦身二阶段   ── 4 项，预计 3-5 天，函数级拆分
Step 4 (重要)     测试补强         ── 7 项，预计 3-5 天，纯增量
Step 5 (重要)     引擎可测性重构   ── 2 项，预计 2-3 天，涉及 trait 抽象
Step 6 (计划)     工程改善         ── 8 项，预计 3-5 天，长期迭代
```

---

### Step 1 — 安全 + 依赖修复（紧急，零破坏性变更）

> **目标**: 消除已知安全缺口和编译不确定性，所有改动均为纯增量或精确替换。  
> **验收标准**: `cargo check --workspace` 通过，现有测试全部绿，CI lint 零警告。

#### 1.1 修复 tui 依赖版本冲突

| 项目 | 说明 |
|------|------|
| **影响范围** | [tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) 单文件 |
| **破坏性** | 无（Cargo 自动解析兼容版本） |
| **操作** | |

```toml
# 改动 1: axum "0.8.4" → workspace 继承（消除版本冲突）
axum = { workspace = true, features = ["json"] }

# 改动 2: async-trait "0.1" → workspace 继承（精确锁定补丁版本）
async-trait = { workspace = true }

# 改动 3: chrono "0.4" → workspace 继承（精确锁定补丁版本）
chrono = { workspace = true, features = ["serde"] }
```

**验证**：`cargo tree -i axum` 确认只有一个 axum 版本被解析。

#### 1.2 tui 其余 14 个重叠依赖改为 workspace 继承

| 项目 | 说明 |
|------|------|
| **影响范围** | [tui/Cargo.toml](file:///f:/DeepSeek-TUI-desktop/crates/tui/Cargo.toml) 单文件 |
| **破坏性** | 无 | 
| **操作** | 将以下依赖从硬编码版本改为 `{ workspace = true }`，需要额外 features 的使用 `{ workspace = true, features = [...] }`：|

```toml
anyhow = { workspace = true }
clap = { workspace = true, features = ["derive"] }
clap_complete = { workspace = true }
dirs = { workspace = true }
reqwest = { workspace = true, features = ["blocking", "json", "stream", "multipart", "rustls", "http2"] }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true, features = ["preserve_order"] }
toml = { workspace = true }
tokio = { workspace = true, features = ["full"] }
uuid = { workspace = true, features = ["v4"] }
thiserror = { workspace = true }
tracing = { workspace = true }
tower-http = { workspace = true, features = ["cors"] }
sha2 = { workspace = true }
```

| 版本一致的 14 个 | 状态 |
|-----------------|------|
| anyhow, clap, clap_complete, dirs, reqwest, serde, serde_json, toml, tokio, uuid, thiserror, tracing, tower-http, sha2 | 版本与 workspace 相同，仅语法改为继承 |

**验证**：`cargo check -p deepseek-tui` 通过。

#### 1.3 所有 crate 添加 `rust-version.workspace = true`

| 项目 | 说明 |
|------|------|
| **影响范围** | 15 个 Cargo.toml，每个加 1 行 |
| **破坏性** | 无 | 
| **操作** | 在每个 crate 的 `[package]` 段中添加：|

```toml
rust-version.workspace = true
```

**涉及文件**：
- `crates/tui/Cargo.toml`、`crates/cli/Cargo.toml`、`crates/desktop/Cargo.toml`
- `crates/agent/Cargo.toml`、`crates/app-server/Cargo.toml`、`crates/config/Cargo.toml`
- `crates/core/Cargo.toml`、`crates/execpolicy/Cargo.toml`、`crates/hooks/Cargo.toml`
- `crates/mcp/Cargo.toml`、`crates/protocol/Cargo.toml`、`crates/secrets/Cargo.toml`
- `crates/state/Cargo.toml`、`crates/tools/Cargo.toml`、`crates/tui-core/Cargo.toml`

**验证**：`rustc +1.88 --version` 后 `cargo check --workspace` 无误。

#### 1.4 文档标注沙箱现状 + 修复 `Op::EditLastTurn` 语义

| 项目 | 说明 |
|------|------|
| **影响范围** | `sandbox/mod.rs`（文档注释）、`engine.rs:770-779`（语义修复） |
| **破坏性** | `EditLastTurn` 修复可能改变极端多轮对话行为 |

**操作 A — 文档标注**（在不实际实现沙箱的情况下管理用户预期）：

在 [sandbox/mod.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs) 的 `prepare_landlock` 和 `prepare_windows` 函数文档中明确标注当前限制，并在用户文档（README/SECURITY.md）中添加说明。

**操作 B — 修复 EditLastTurn**：

[engine.rs:L770-779](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs#L770-L779) 当前用 `truncate(idx)` 删除最后一条用户消息及之后所有内容。应改为精确删除该用户消息及其紧随的助手回复：

```rust
// 伪代码示意
Op::EditLastTurn => {
    // 找到最后一条用户消息的索引 idx
    // 如果 idx+1 是助手消息，一并删除
    if idx + 1 < len && messages[idx+1].role == Role::Assistant {
        messages.drain(idx..=idx+1); // 精确删除一对 user+assistant
    } else {
        messages.truncate(idx); // 仅删除最后的用户消息
    }
}
```

**验证**：现有 shell/tests.rs 和 engine/tests.rs 全部通过。

---

### Step 2 — 架构瘦身一阶段：文件级拆分（高优先级）

> **目标**: 将 6 个巨型文件中的 4 个拆分为可管理的子文件，降低认知负荷。  
> **验收标准**: `cargo check/clippy/test --workspace` 全部通过，外部 API 无变更。

#### 2.1 创建 `crates/tui/src/lib.rs` 并导出核心 API

| 项目 | 说明 |
|------|------|
| **影响范围** | 新增 `crates/tui/src/lib.rs`，`main.rs` 改为薄入口 |
| **破坏性** | 可能触发未使用导入/死代码警告 |

**实施要点**：

1. 在 `crates/tui/src/lib.rs` 中声明所有需要对外可见的模块（当前在 `main.rs` 中的 `mod` 声明）
2. 使用 `pub use` 导出核心类型：`Engine`、`EngineHandle`、`DeepSeekClient`、`ToolRegistry`、`SandboxPolicy`、`ErrorEnvelope`、`App` 等
3. `main.rs` 删去 `mod` 声明，改为 `use deepseek_tui::*`，仅保留 CLI 分发逻辑和 `fn main()`
4. 调整 `Cargo.toml` 确保 bin 和 lib 共存

**分支策略**：单独 PR，先合并 lib.rs 骨架，后续拆分在主线上增量进行。

#### 2.2 拆分 `main.rs`：子命令实现移入 `commands/`

| 项目 | 说明 |
|------|------|
| **影响范围** | `main.rs` 从 >180KB 缩减到 ~30KB |
| **新增文件** | 约 10 个 `commands/*.rs` |
| **破坏性** | 无 |

**实施要点**：

1. 建立目录 `crates/tui/src/commands/`
2. 每个 CLI 子命令提取为独立文件：
   - `doctor.rs`：`run_doctor()` 及相关诊断逻辑
   - `exec.rs`：`run_exec_agent()` 及 exec 模式逻辑
   - `review.rs`：review 相关逻辑
   - `mcp_cmd.rs`：MCP 管理子命令
   - `sandbox_cmd.rs`：sandbox 管理子命令
   - `one_shot.rs`：`run_one_shot()` 一键模式
3. `main.rs` 仅保留 `fn main()` + `Cli` 定义 + `match` 分发 → 调用各命令模块

**注意**：注意 Windows UTF-8 初始化（`unsafe` 块）的 SAFETY 注释，在拆分时一并补上。

#### 2.3 拆分 `app.rs`：渲染与事件分离

| 项目 | 说明 |
|------|------|
| **影响范围** | `app.rs` 从 >176KB 缩减到 ~50KB |
| **新增文件** | `tui/render.rs`、`tui/events.rs` |
| **破坏性** | 无（仅同类函数位移） |

**实施要点**：

1. `tui/render.rs`：汇集所有 `draw_*` / `render_*` 方法（draw_chat_view、draw_composer、draw_sidebar 等）
2. `tui/events.rs`：汇集所有 `handle_*` 事件处理（handle_key_event、handle_input_submit、handle_mode_switch 等）
3. `app.rs` 保留 `App` 结构体定义、字段分组子结构体（`ComposerState` 等）、生命周期（`new`/`tick`/`reset`）、业务逻辑入口

#### 2.4 拆分 `config/src/lib.rs` 和 `core/src/lib.rs`

| 项目 | 说明 |
|------|------|
| **影响范围** | `config/src/lib.rs` (2165 行)、`core/src/lib.rs` (1698 行) |
| **新增文件** | `config/src/validation.rs`、`config/src/defaults.rs`、`core/src/types.rs` 等 |
| **破坏性** | 可能影响下游使用 `deepseek-config`/`deepseek-core` 的 crate 路径 |

**实施要点**：

- `config/`：按关注点拆分为 `validation.rs`（配置校验逻辑）、`defaults.rs`（默认值）、`env.rs`（环境变量读取），`lib.rs` 仅做 `pub use` 聚合
- `core/`：将 `Event`、`Message`、`ContentBlock` 等核心类型提至 `types.rs`；将序列化/反序列化逻辑提至 `serde_helpers.rs`

---

### Step 3 — 架构瘦身二阶段：函数级拆分（高优先级）

> **目标**: 降低 `handle_deepseek_turn` 等复杂函数的认知负担，配合依赖治理。  
> **验收标准**: 每个提取出的子函数有独立单元测试，引擎行为不变。

#### 3.1 拆分 `handle_deepseek_turn` (turn_loop.rs)

| 项目 | 说明 |
|------|------|
| **影响范围** | `turn_loop.rs` 从 ~1909 行缩减到 ~600 行 |
| **新增** | 约 4-5 个子函数 |

**实施要点**：

1. `process_stream_events(…)` — 内层流事件循环：接收 SSE 事件 → 积累文本/thinking/tool_use → 返回 `StreamOutcome`
2. `execute_tool_batch(…)` — 工具批处理：`FuturesUnordered` 并行执行 + 审批流程 + 超时处理 → 返回 `BatchResult`
3. `handle_tool_results(…)` — 工具结果注入消息历史 + 触发压缩检查 + 容量控制器通知
4. `handle_subagent_completions(…)` — 子代理完成等待 + 邮箱传播逻辑
5. `apply_stream_retry_logic(…)` — 透明流重试决策（当前在 turn_loop.rs:696-729 的 `stream_died_with_nothing` 逻辑）

**每个子函数**：需提供独立的 `#[cfg(test)] mod tests`，至少覆盖正常路径 + 一个边界/错误路径。

#### 3.2 `handle_send_message` 引入参数结构体

| 项目 | 说明 |
|------|------|
| **影响范围** | `engine.rs` 中 `handle_send_message` 签名 |
| **破坏性** | 无（纯内部重构） |

**实施**：将 11 个参数封装为 `struct SendMessageParams`：

```rust
struct SendMessageParams {
    pub messages: Vec<ApiMessage>,
    pub tool_context: ToolContext,
    pub system_prompt: Option<String>,
    pub cancellation_token: CancellationToken,
    pub mode: AppMode,
    pub capacity: CapacityBudget,
    // ... 其余字段
}
```

移除 `#[allow(clippy::too_many_arguments)]` 标注。

#### 3.3 `build_chat_messages_with_reasoning` 添加集成测试

| 项目 | 说明 |
|------|------|
| **影响范围** | `client/chat.rs:574-676` |
| **破坏性** | 无（纯增量测试） |

**实施**：为孤儿 tool_calls/tool_results 清理逻辑补充兼容性测试：
- 压缩后所有 tool_calls 无配对 tool_results → 应全部移除
- 压缩后所有 tool_results 无配对 tool_calls → 应全部移除
- 配对完整的 tool_calls+tool_results → 应保留
- 混合场景（部分配对/部分孤立）→ 应正确筛选

#### 3.4 可选 feature 限流：将重依赖移入 `[features]`

| 项目 | 说明 |
|------|------|
| **影响范围** | `tui/Cargo.toml` |
| **破坏性** | 下游使用默认 build 无影响；使用被移为 optional 的 API 需开启 feature |

**实施**：

```toml
[features]
default = ["tui", "json", "toml"]
tui = ["dep:schemaui", "schemaui/tui", "json", "toml"]
web = ["dep:schemaui", "schemaui/web", "json", "toml"]
json = ["schemaui/json"]
toml = ["schemaui/toml"]
# 新增可选 feature
office = ["dep:pdf-extract", "dep:rust_xlsxwriter", "dep:zip", "dep:tar", "dep:flate2"]
image-extract = ["dep:image"]
scripting = ["dep:starlark"]
```

将对应依赖从 `[dependencies]` 移动到 `[dependencies]` 但标记 `optional = true`。在代码中使用 `#[cfg(feature = "office")]` 条件编译。

---

### Step 4 — 测试补强（高优先级）

> **目标**: 补齐基础 crate 测试缺口，增加 CI 质量门禁。  
> **验收标准**: CI `cargo test --workspace` 全绿，新增 `cargo audit` 零已知漏洞。

#### 4.1 为 `protocol`/`state`/`tools` 补充测试

| Crate | 当前 | 目标 | 重点覆盖 |
|-------|------|------|---------|
| `protocol` | 3 | ≥15 | 反序列化边界（未知字段、缺失字段、类型不匹配）、双向序列化往返、EventFrame 所有变体 |
| `state` | 1 | ≥10 | 并发 upsert（`tokio::spawn` 竞争）、损坏数据恢复、大批量操作性能断言、归档清理 |
| `tools` | 5 | ≥12 | 超时路径、未知工具分派、错误传播链（`ToolError → ErrorEnvelope`）、并发分派正确性 |

#### 4.2 为 `desktop`/`app-server` 补充基础测试

| Crate | 重点覆盖 |
|-------|---------|
| `desktop` | Tauri command 参数校验（边界值、非法路径）、sidecar 启动失败路径、IPC 响应序列化 |
| `app-server` | HTTP 路由正确性、请求体大小限制、JSON 格式错误响应 |

#### 4.3 添加 TUI 组件 snapshot 测试

| 项目 | 说明 |
|------|------|
| **新增依赖** | `insta`（Rust snapshot 测试库） |
| **重点覆盖** | |

1. `MarkdownRender` 输出：常见 markdown 块（代码、表格、引用、数学公式）的渲染快照
2. `ToolCard` 状态：pending / running / success / error 各状态渲染
3. `DiffRender`：增删改行的高亮渲染
4. 错误消息模板：`ErrorEnvelope` 的 UI 展示格式

#### 4.4 CI 添加 `cargo audit`

| 项目 | 说明 |
|------|------|
| **影响范围** | `.github/workflows/ci.yml` |
| **操作** | |

在 `lint` job 的 `Run clippy` 步骤后追加：

```yaml
- name: Install cargo-audit
  uses: taiki-e/install-action@v2
  with:
    tool: cargo-audit
- name: Run cargo audit
  run: cargo audit --deny warnings
```

如果当前存在未修复的已知漏洞，初期先用 `cargo audit`（不 `--deny`）收集基线，然后逐一修复。

#### 4.5 CI 添加代码覆盖率

| 项目 | 说明 |
|------|------|
| **影响范围** | `.github/workflows/ci.yml` |
| **新增依赖** | `cargo-llvm-cov` |

```yaml
- name: Install cargo-llvm-cov
  uses: taiki-e/install-action@v2
  with:
    tool: cargo-llvm-cov
- name: Coverage
  run: cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info
- name: Upload to Codecov
  uses: codecov/codecov-action@v5
  with:
    files: lcov.info
```

#### 4.6 CI `docs` job 从仅定时改为 PR 也触发

| 项目 | 说明 |
|------|------|
| **影响范围** | `.github/workflows/ci.yml` 中 `docs` job 的触发条件 |
| **改动** | 移除 `if: github.event_name == 'schedule'`，改为与 push/PR 同步 |

#### 4.7 为 Release 添加版本一致性验证

| 项目 | 说明 |
|------|------|
| **影响范围** | `.github/workflows/release.yml` |
| **操作** | 在 `parity` job 中添加步骤：从 `${{ github.ref_name }}` 提取 semver，比对 `Cargo.toml` 中的 `version`，不一致则失败 |

---

### Step 5 — 引擎可测性重构（高优先级）

> **目标**: 使 Engine 支持 trait mock，解锁 4 个被 ignore 的端到端测试。  
> **验收标准**: `integration_mock_llm.rs` 中 4 个 `#[ignore]` 测试改为正常运行且全部通过。

#### 5.1 Engine 支持 `Arc<dyn LlmClient>` 注入

| 项目 | 说明 |
|------|------|
| **影响范围** | `engine.rs`、`turn_loop.rs`、`dispatch.rs`、`MockLlmClient` |
| **破坏性** | Engine 构造方式变更 |

**实施要点**：

1. 当前 `Engine` 持有具体 `DeepSeekClient`，改为持有 `Arc<dyn LlmClient>`
2. `Engine::new()` 接受 `Arc<dyn LlmClient>` 参数（默认仍可用 `DeepSeekClient::new()`）
3. `MockLlmClient` 已在 `integration_mock_llm.rs` 中实现，可直接用作 trait object
4. 将 4 个 `#[ignore]` 测试取消忽略，验证通过后提交

**风险点**：`DeepSeekClient` 可能有不在 trait 中的专用方法，需确认 trait 抽象是否覆盖所有调用路径。

#### 5.2 新增端到端测试

| 测试场景 | 目的 |
|---------|------|
| 完整 turn 循环（正常流） | 验证 turn_loop → tool_execution → result_injection 链路 |
| 子代理 spawn 往返 | 验证 agent:spawn → agent:assign → agent:wait 完整生命周期 |
| 并行工具执行排序 | 验证 `FuturesUnordered` 的输出顺序 |
| 容量控制器强制压缩 | 验证上下文超预算 → 压缩 → 重试 的恢复链路 |

---

### Step 6 — 工程改善（计划，长期迭代）

> **目标**: 消除代码异味，统一依赖风格，提升用户信任。  
> **验收标准**: 逐项交付，不要求一次性完成。

#### 6.1 `classify_error_message` 结构化

| 项目 | 说明 |
|------|------|
| **影响范围** | [error_taxonomy.rs:295-356](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/error_taxonomy.rs#L295-L356) |
| **破坏性** | API 兼容，仅改变内部实现 |

**实施**：分两步走：
1. 先添加 30+ 边界用例单元测试，锁死当前分类行为（防止回归）
2. 将关键词匹配改为 `From<HttpStatusCode>` + `From<LlmError>` + `From<ToolError>` 的结构化转换，逐步替换掉字符串匹配路径

#### 6.2 `VecDeque` 优化

| 项目 | 说明 |
|------|------|
| **影响范围** | [engine.rs:1292](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/core/engine.rs#L1292) |
| **破坏性** | 可能影响依赖 `Vec` 索引语义的所有引用 |

**实施**：将消息历史从 `Vec<ApiMessage>` 改为 `VecDeque<ApiMessage>`，或保留 `Vec` 但改 `remove(0)` 为 `drain(0..n)` 批量删除旧消息。批量删除路径同样 O(n)，但常数因子更优。最彻底方案是使用 `VecDeque`。

#### 6.3 `SessionState` 定点数成本追踪

| 项目 | 说明 |
|------|------|
| **影响范围** | [app.rs](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/tui/app.rs) |
| **破坏性** | 存储/序列化格式变更 |

**实施**：将 `session_cost: f64` 改为 `session_cost_millicents: i64`（以 1/10,000,000 元为单位，对应 API 的最小计费粒度），显示时除以 10,000,000 格式化。

#### 6.4 MCP 子进程启动实现

| 项目 | 说明 |
|------|------|
| **影响范围** | [crates/mcp/src/lib.rs](file:///f:/DeepSeek-TUI-desktop/crates/mcp/src/lib.rs) |
| **新增** | `StdioMcpClient` 实现 `McpManagedClient` trait |
| **破坏性** | 无（纯增量） |

**实施**：
1. 实现 `StdioMcpClient` struct，管理 MCP 服务器子进程的 stdin/stdout 管道
2. 在子进程启动时发送 `initialize` JSON-RPC 请求完成协议握手
3. 实现 `list_tools()`/`call_tool()` 通过 stdio 管道与子进程通信
4. 注册到 `McpManager` 中替代 `default_stdio_client() → InMemoryMcpClient`

#### 6.5 实现 Linux Landlock 沙箱

| 项目 | 说明 |
|------|------|
| **影响范围** | [sandbox/mod.rs:395-419](file:///f:/DeepSeek-TUI-desktop/crates/tui/src/sandbox/mod.rs#L395-L419) |
| **新增依赖** | `landlock` crate 或内联 landlock syscall |
| **破坏性** | 行为变更：Linux 上命令将被实际隔离 |

**实施**：
1. 创建 `sandbox/landlock.rs` 模块
2. 根据 `SandboxPolicy` 构建 Landlock 规则集：`WorkspaceWrite` → 仅允许写入 workspace 目录及其子目录；`ReadOnly` → 禁止所有写入；网络访问通过 policy flag 控制
3. `prepare_landlock` 中实际调用 `LandlockRestrictSelf`（通过 `landlock` crate 或内联 syscall）
4. 添加集成测试：验证 `rm -rf /tmp/test` 在 `ReadOnly` 策略下被阻止并返回 `EPERM`

#### 6.6 统一 workspace 版本约束风格

| 项目 | 说明 |
|------|------|
| **影响范围** | [Cargo.toml](file:///f:/DeepSeek-TUI-desktop/Cargo.toml) workspace.dependencies |
| **破坏性** | 无 |
| **操作** | |

将 `thiserror = "2.0"` → `"2.0.12"`、`sha2 = "0.10"` → `"0.10.8"`、`tracing = "0.1"` → `"0.1.41"` 等，全表统一为 `x.y.z` 精确补丁版本。

#### 6.7 `base64`/`tempfile`/`ignore` 纳入 workspace.dependencies

| 项目 | 说明 |
|------|------|
| **操作** | |
1. 在 workspace.dependencies 中添加：`base64 = "0.22.1"`、`tempfile = "3.16"`、`ignore = "0.4"`
2. tui 和 desktop 的 `base64` 改为 `{ workspace = true }`
3. tui/cli/secrets 的 `tempfile` 统一为 workspace 继承

#### 6.8 macOS/Windows 二进制代码签名

| 项目 | 说明 |
|------|------|
| **影响范围** | `.github/workflows/release.yml` |
| **操作** | |

- macOS：集成 `codesign` — 需 Apple Developer 证书存储在 GitHub Secrets 中
- Windows：集成 `signtool` — 需 EV Code Signing Certificate 存储在 GitHub Secrets 中
- 如暂无法获取证书，先研究方案并记录在 `RELEASE_RUNBOOK.md`

---

## 附录

### A. 审核方法

本次审核通过以下方式进行：

1. **静态代码分析**: 逐文件阅读关键模块源码，评估代码结构、逻辑正确性、错误处理、安全性
2. **依赖分析**: 逐个比对 15 个 Cargo.toml 与 workspace.dependencies 的一致性
3. **测试审计**: 检查所有测试文件，评估测试覆盖范围和质量
4. **CI/CD 审计**: 检查 CI 和 Release 工作流配置
5. **子代理并行审核**: 使用三个搜索子代理并行审核不同模块，确保全面性

### B. 文件引用说明

本报告中所有文件引用使用以下格式：
- [文件名](file:///absolute/path/to/file#L123-L145) — 可点击跳转到源代码具体位置

### C. 评分标准

| 评分 | 含义 |
|------|------|
| ⭐⭐⭐⭐⭐ (5/5) | 业界标杆，无可挑剔 |
| ⭐⭐⭐⭐ (4/5) | 优秀，有小幅改进空间 |
| ⭐⭐⭐ (3/5) | 良好，有显著改进空间 |
| ⭐⭐ (2/5) | 及格，存在系统性问题 |
| ⭐ (1/5) | 不及格，需要重大重构 |
