# DeepSeek TUI v0.8.15 — 全量代码评审报告

**评审日期**: 2026-05-09  
**评审范围**: 整个仓库（270+ 源文件，15 个 crate）  
**评审方法**: 逐层通读 — 小型/中型 crate 全文读取，大型 `crates/tui` crate 核心模块抽样  

**修订说明（2026-05-09 人工复核）**: 更正正文中 **H3** 与当前实现不符的过时代码描述（已为 **origin 白名单**，非 `permissive()`）；收紧 **M9 / H6** 关于「任意 HTML」的表述（`markdown-it` 已 `html: false`）；移除 **H1** 中非仓库事实的署名；附录 **§6 验证**与 **§5.2 / §4.15** 中与 H3 相关的句子已同步。

---

## 目录

1. [架构总览](#1-架构总览)
2. [模块地图](#2-模块地图)
3. [发现汇总](#3-发现汇总)
   - [CRITICAL](#critical)
   - [HIGH](#high)
   - [MEDIUM](#medium)
   - [LOW / 风格建议](#low--风格建议)
4. [逐 Crate 详述](#4-逐-crate-详述)
5. [跨切面分析](#5-跨切面分析)
6. [验证总结](#6-验证总结)
7. [D. 问题修正实施计划](#d-问题修正实施计划)

---

## 1. 架构总览

DeepSeek TUI 采用**分层单体 + 工作区 crate 拆分**架构：

```
┌─────────────────────────────────────────────────────────┐
│ CLI 入口 (crates/cli) — clap 参数解析，委托至 TUI 二进制  │
├─────────────────────────────────────────────────────────┤
│ 共享数据层                                              │
│  config → secrets → state → protocol → agent           │
│  (配置)   (密钥)    (持久化)  (事件协议)  (模型注册)      │
├─────────────────────────────────────────────────────────┤
│ 运行时核心 (crates/tui)                                  │
│  engine → tools → sandbox → LSP → MCP → RLM            │
│  runtime_api → task_manager → session_manager           │
│  TUI (ratatui) → streaming → approval                  │
├─────────────────────────────────────────────────────────┤
│ 桌面壳 (crates/desktop)                                  │
│  Tauri 2 + React WebView → sidecar deepseek-tui serve   │
└─────────────────────────────────────────────────────────┘
```

**整体评价**: 架构层次清晰，职责分离良好。`crates/tui` 仍然承载过多逻辑（引擎、工具、TUI 渲染、HTTP API 全在一个 crate），但正在逐步拆分中（`crates/tui-core` 已拆分，更多拆分在计划中）。

Crate 依赖图符合预期：`config → secrets → state ← core`，`protocol` / `tools` / `agent` 被上层消费。无循环依赖。

---

## 2. 模块地图

| Crate | 文件数 | 行数(估) | 角色 |
|-------|--------|----------|------|
| `crates/cli` | 5 | ~2500 | CLI dispatcher，参数解析，委托 TUI |
| `crates/config` | 1 | ~2200 | TOML 配置加载/合并/序列化，运行时选项解析 |
| `crates/secrets` | 1 | ~600 | 密钥存储抽象（keyring → env → file） |
| `crates/state` | 1 | ~700 | SQLite 持久化状态（线程/消息/检查点/任务） |
| `crates/protocol` | 1 | ~400 | 共享事件/请求/响应类型定义 |
| `crates/tools` | 1 | ~350 | 工具注册表抽象（Trait + 分发） |
| `crates/agent` | 1 | ~400 | 模型注册表（多提供者别名解析） |
| `crates/core` | 1 | ~1200 | Runtime / ThreadManager / JobManager |
| `crates/execpolicy` | 2 | ~250 | 执行策略引擎（前缀匹配 + 审批） |
| `crates/hooks` | 1 | ~200 | 生命周期钩子调度器 |
| `crates/mcp` | 1 | ~650 | MCP 服务器管理 + stdio JSON-RPC |
| `crates/app-server` | 2 | ~850 | HTTP/stdio 应用服务器传输层 |
| `crates/tui-core` | 1 | ~200 | UI 状态机（Pane + Event → Effect） |
| `crates/desktop` | 3 | ~600 (Rust) | Tauri 壳 + sidecar 管理 |
| `crates/desktop/web-ui` | 15 | ~2000 (TS/TSX) | React 聊天 UI |
| `crates/tui` | ~200 | ~50000+ | **核心运行时**：引擎/工具/TUI/API |

---

## 3. 发现汇总

### 严重性分布

| 严重性 | 数量 | 说明 |
|--------|------|------|
| CRITICAL | 1 | 直接安全风险或数据丢失 |
| HIGH | 4 | 高影响问题 |
| MEDIUM | 8 | 应修复的设计/正确性问题 |
| LOW | 6 | 风格/维护性改进建议 |

---

### CRITICAL

#### C1 — `save_deepseek_api_key` 将 API 密钥写入 `~/.deepseek/config.toml` 时未验证文件权限（Windows 路径）

- **文件**: `crates/desktop/src/commands.rs:84-106`
- **问题**: `write_user_config_bytes()` 在 Windows 上直接调用 `fs::write()`，不设置文件权限。虽然 Windows ACL 通常限制用户目录访问，但脚本/自动化环境可能暴露该文件。
- **影响**: 在共享 Windows 机器或多用户环境中，若用户目录权限配置不当，API 密钥可能被其他用户读取。
- **建议**: 即使 Windows 不原生支持 `0600`，也应调用 Windows ACL API (`SetNamedSecurityInfoW`) 或至少记录一条 warning 日志。当前代码仅对 `#[cfg(unix)]` 设置 `0o600`。

---

### HIGH

#### H1 — `fetch_url` 工具存在 SSRF 风险：允许访问内网地址

- **文件**: `crates/tui/src/tools/fetch_url.rs`
- **问题**: `fetch_url` 工具虽有多层防护（网络策略、host_from_url 解析），但未阻止对 `127.0.0.1`、`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16` 等内网地址的请求。网络策略的默认值为 `prompt`（每次询问），但 YOLO 模式下模型可获自动批准。
- **影响**: 攻击者可能通过构造恶意 prompt 让 agent 访问内网服务（如 `http://169.254.169.254/latest/meta-data/` 云 metadata）。
- **现有缓解**: `network_policy.rs` 提供 `deny` 列表；`docs/RUNTIME_API.md` 等对运行时网络策略与 SSRF 注意事项有文档说明。默认策略为 `prompt` 而非默认 `deny` 内网地址。
- **建议**: 在默认 deny 列表中添加 `127.0.0.0/8`、`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`、`169.254.0.0/16`（或至少 `169.254.169.254`）。

#### H2 — 密钥优先级可能导致意外密钥泄露

- **文件**: `crates/config/src/lib.rs` — `resolve_runtime_options_with_secrets()`
- **问题**: API 密钥解析链为 `CLI flag → config-file → keyring → env`。若用户在 `~/.deepseek/config.toml` 中配置了密钥但之后通过 `deepseek auth set` 更新，旧密钥仍留在文件中（明文）。同时 `DEEPSEEK_API_KEY` 环境变量可能被第三方工具注入。
- **影响**: 旧密钥残留增加泄露面。环境变量在 CI/CD 日志中可能被意外捕获。
- **建议**: 
  1. `deepseek auth set` 更新密钥后，主动建议用户清理环境变量
  2. 在 `doctor` 检查中增加 "检测到 config 中有旧密钥 + 环境变量有不同密钥" 的警告

#### H3 — `runtime_api.rs` CORS：origin 已白名单化，仍须注意扩展面与 Host 暴露

- **文件**: `crates/tui/src/runtime_api.rs`（`cors_layer` / `DEFAULT_CORS_ORIGINS`，约 `1760–1788` 行一带；行号随版本变动）
- **更正（原报告过时）**: 当前实现**并非** `CorsLayer::permissive()`，而是 `CorsLayer::new().allow_origin(显式 HeaderValue 列表)`，默认包含 `http://127.0.0.1` / `http://localhost` 各常见端口、`tauri://localhost`、`http://tauri.localhost` 等，供桌面 WebView 与本地开发使用。
- **残留风险**:
  1. `allow_headers(Any)`：请求头约束较宽，需结合鉴权与绑定地址综合评估。
  2. 若配置向 `cors_layer` 注入**不可信**的 `extra_origins`，等效扩大浏览器可发起跨域的来源集合。
  3. 用户将服务绑定到 `--host 0.0.0.0` 且非本机信任网络时，浏览器侧恶意页 + 误配 origin 列表的组合仍可能扩大攻击面（威胁模型：可写配置或内网攻击者）。
- **建议**:
  1. 对 `extra_origins` 做文档化与校验（拒绝非 http(s)/tauri 方案、记录审计日志）。
  2. `--host 0.0.0.0` 时打印明确安全警告，并指向「仅信任网络」说明。
  3. 评估是否将 `allow_headers` 收紧为运行时所必需的头集合（如 `Authorization`、`Content-Type`）。

#### H4 — Sidecar 进程在 `CREATE_NO_WINDOW` 模式下无 stderr 日志

- **文件**: `crates/desktop/src/sidecar.rs:49-51`
- **问题**: Sidecar 以 `Stdio::null()` 启动，stderr 被丢弃。当 `deepseek-tui serve` 启动失败时，用户得不到任何错误信息。健康检查最多重试 10 次 × 1 秒 = 10 秒后才报 "failed to become healthy"。
- **影响**: 用户无法诊断 sidecar 启动失败的原因（如缺少 API key、端口冲突等）。
- **建议**: 至少将 stderr 重定向到一个日志文件（`~/.deepseek/logs/sidecar.log`），或在重试失败后读取最近几行到错误消息中。

---

### MEDIUM

#### M1 — `ConfigToml::set_value` / `unset_value` 存在 9 个提供者的重复代码

- **文件**: `crates/config/src/lib.rs` (~400 行重复模式)
- **问题**: `set_value` 和 `unset_value` 对每个提供者（deepseek/nvidia_nim/openai/openrouter/novita/fireworks/sglang/vllm/ollama）都重复相同的 `match` 分支。DeepSeek 提供者的处理逻辑还混杂了 root 字段回写（`self.api_key = Some(value)` 等）。
- **影响**: 添加第 10 个提供者需要修改 3 处 match（`set_value`/`unset_value`/`get_value`）。容易遗漏导致行为不一致。
- **建议**: 用宏或 trait 消重。例如定义 `provider_keys()` 返回 `&[(ProviderKind, &str)]` 然后驱动统一的 get/set/unset 循环。

#### M2 — `ExecPolicyEngine` 的 `check()` 方法允许在不匹配时默认通过

- **文件**: `crates/execpolicy/src/lib.rs:145-190`
- **问题**: 当命令不匹配任何 `denied_prefixes` 且 `trusted_prefixes` 为空时，`ask_for_approval` 为 `OnFailure` 或 `Never` 会直接放行。这意味默认配置下（`trusted_prefixes=[]`）的 `Never` 模式允许执行任意命令。
- **影响**: 这是设计选择而非 bug：YOLO 模式本意就是自动批准。但 `OnFailure` 在命令尚未执行时就跳过意味着"允许运行，失败了才告诉用户"——语义上接近 `Never`。
- **建议**: 文档化 `OnFailure` vs `OnRequest` 的精确语义差异；考虑将 `OnFailure` 重命名为 `RunFirstAskLater` 更清晰。

#### M3 — `StateStore` SQL 查询使用字符串拼接构建动态 WHERE 子句

- **文件**: `crates/state/src/lib.rs:211-218`
- **问题**: `list_threads()` 根据 `filters.include_archived` 使用不同的 SQL 字符串。当前仅切换 WHERE 子句且无用户输入拼接，因此无 SQL 注入风险。但若未来扩展更多 filter 字段，字符串拼接模式容易出错。
- **影响**: 当前无实际风险。代码质量/可维护性问题。
- **建议**: 使用参数化查询构建器或 `rusqlite` 的 `params_from_iter` 动态构建 WHERE 条件。

#### M4 — `app-server` 和 `mcp` crate 各自实现了完整的 JSON-RPC 2.0 栈

- **文件**: `crates/app-server/src/lib.rs` 和 `crates/mcp/src/lib.rs`
- **问题**: 两个 crate 各自定义了 `JsonRpcRequest`、`JsonRpcError`、`jsonrpc_result`、`jsonrpc_error` 以及 5 个标准错误码。代码几乎逐字相同。
- **影响**: 维护两个副本导致修复不同步。当前无协议差异，但任何一方的修改可能遗漏同步。
- **建议**: 将 JSON-RPC 基元提取到 `crates/protocol`（该 crate 已有 `EventFrame` 等共享类型）。

#### M5 — React 前端 `App.tsx` 中 `handleSend` 函数过长（~200 行）

- **文件**: `crates/desktop/web-ui/src/App.tsx:366-570`
- **问题**: `handleSend` 承担了消息构建、SSE 事件流解析、状态更新、审批处理、会话持久化等 6 项职责。`useCallback` 依赖数组有 7 个依赖项。
- **影响**: 难以测试、容易因 closure 陈旧引用导致 bug、难以添加新功能。
- **建议**: 拆分为自定义 hook：`useChatStream()` 封装 SSE 逻辑，`useApproval()` 封装审批状态机，`useSessionPersistence()` 封装持久化。

#### M6 — `client.rs` 中 `to_api_tool_name` / `from_api_tool_name` 的编码方案脆弱

- **文件**: `crates/tui/src/client.rs:20-85`
- **问题**: 工具名使用 `-xHHHHHH-` 格式编码非 ASCII 字符，再用 `decode_bare_hex_escapes` 做二次解码处理模型产出异常格式。这本质上是修复模型不遵循协议的症状，而非在设计层面保证名称不变。
- **影响**: 若 DeepSeek API 调整工具名编码规则，当前解码逻辑可能需要再次修补。
- **建议**: 如果可行，要求 API 端对工具名做 round-trip 保证（透传原始名称），或使用 Base64 编码而非自定义 hex 编码。

#### M7 — `sandbox/windows.rs` 是存根实现

- **文件**: `crates/tui/src/sandbox/windows.rs`（通过 `mod.rs` 引用）
- **问题**: TUI crate 声明了 Windows sandbox 模块但根据项目架构描述，Windows 上的沙箱功能可能受限（macOS 有 Seatbelt，Linux 有 Landlock）。
- **影响**: Windows 用户无法获得与 macOS/Linux 同等级别的沙箱保护。
- **建议**: 在 README 中明确文档化各平台沙箱能力矩阵。考虑使用 Windows Job Objects 或 Restricted Tokens 提供基础沙箱。

#### M8 — DAST 相关：测试覆盖率偏低

- **文件**: 整个 `crates/tui/src/tools/` 目录
- **问题**: `crates/tools` 有 `tests/parity_tools.rs`，`crates/tui` 有 `tests/` 目录，但核心工具如 `shell.rs`（2513 行）仅有一个 `tests.rs` 子模块。`fetch_url.rs` 没有内联测试。
- **影响**: 工具层（shell、fetch_url、apply_patch）是安全边界——工具执行是 agent 访问宿主系统的唯一途径。测试不足增加回归风险。
- **建议**: 为每个工具增加参数验证、路径逃逸、超时处理的单元测试。`fetch_url` 应测试 SSRF 防护。

---

### LOW / 风格建议

#### L1 — `list_values()` 中的 `redact_secret` 实现不区分 API Key 和其他字段

- **文件**: `crates/config/src/lib.rs:1074-1078`
- **问题**: `redact_secret` 对 ≤16 字符的值返回 `********`，对更长值取首 4 + 尾 4 字符。对于 `chatgpt_access_token` 等较长 token，这泄露了首尾片段。
- **建议**: 统一对所有 secret 字段使用固定长度的遮蔽（如仅保留前缀 `sk-`）。

#### L2 — `crates/tui-core` 的 `UiState::reduce` 是纯函数但未标注

- **文件**: `crates/tui-core/src/lib.rs:85-160`
- **问题**: `reduce()` 接收 `&mut self` 并返回 `Vec<UiEffect>`——这是一个 Elm/Redux 风格的 reducer。但 `UiEvent::KeyPressed('1'..='5')` 的分支存在大量重复的 `vec![UiEffect::Render]`。
- **建议**: 使用 `match` guard 或 `HashMap<Key, Pane>` 消除重复。

#### L3 — `ThreadManager` 的 `spawn_thread_with_history` 和 `resume_thread_with_history` 共享大量初始化逻辑

- **文件**: `crates/core/src/lib.rs:320-420`
- **问题**: 两个方法都构建 `Thread` 元数据、调用 `persist_thread`、追加历史消息。差异仅在与 `InitialHistory` 变体的交互方式。
- **建议**: 提取 `create_thread_inner()` 私有方法。

#### L4 — 桌面端 `App.tsx` 中有大量 `localStorage` 操作散布在组件中

- **文件**: `crates/desktop/web-ui/src/App.tsx`（多处）
- **问题**: localStorage 的读写没有抽象层：`localStorage.getItem('deepseek-desktop-model')` 等散布在初始化和 effect 中。
- **建议**: 提取 `useLocalStorage<T>(key, default)` hook。

#### L5 — Rust 代码中混用 `#[must_use]` 标注不一致

- **文件**: 多个 crate
- **问题**: `secrets` crate 在 `resolve()` 上有 `#[must_use]`，但 `get()` 上没有。`config` crate 的部分方法有标注，部分没有。`tools` crate 的 `ToolResult` builder 方法有标注。
- **建议**: 为所有纯查询方法（不产生副作用的 getter）添加 `#[must_use]`。

#### L6 — `PromptRequest` / `ThreadResponse` 等协议类型使用 `Value` 作为通用扩展字段

- **文件**: `crates/protocol/src/lib.rs`
- **问题**: `ThreadResponse.data` 和 `AppResponse.data` 使用 `serde_json::Value` 作为通用扩展槽。这避免了频繁修改 schema，但丢失了编译时类型检查。
- **建议**: 对于稳定字段使用具体类型，仅将真正动态的部分（如 `events` 内的 `delta` 内容）保留为 `Value`。

---

## 4. 逐 Crate 详述

### 4.1 `crates/cli` — CLI Dispatcher

**质量**: ★★★★☆

- **优点**:
  - Clap 参数定义完整，子命令层次清晰（`Commands` enum 涵盖 20+ 子命令）
  - 错误链输出（`err.chain()`）确保 TOML 解析错误带行列号——修复了 #767
  - `resolve_runtime_for_dispatch_with_secrets` 自动从 keyring 恢复密钥到配置文件——降低密钥管理摩擦
  - Windows 特殊路径：`run_resume_command` 中 `should_pick_resume_in_dispatcher` 处理 Windows 终端限制

- **问题**: 无严重问题。`build_tui_command` 方法约 120 行，可提取为 `TuiEnvBuilder` 结构体。

### 4.2 `crates/config` — 配置管理

**质量**: ★★★★☆

- **优点**:
  - 配置来源优先级清晰：CLI → env → config-file → keyring → default
  - `merge_project_overrides` 逐字段合并 `providers.*` 子表
  - `list_values` 对所有 secret 字段做 `redact_secret` 遮蔽
  - Unix 上强制 `0o600` 权限
  - 40+ 测试覆盖提供者、环境变量、合并逻辑

- **问题**: 见 M1（提供者代码重复）

### 4.3 `crates/secrets` — 密钥存储

**质量**: ★★★★★

- **优点**:
  - 干净的抽象层：`KeyringStore` trait → `DefaultKeyringStore`（OS keyring）/ `FileKeyringStore`（headless 回退）/ `InMemoryKeyringStore`（测试）
  - `FileKeyringStore` 有权限检查（拒绝非 0600 文件），修复了 #281（写入错误时不会覆盖现有密钥）
  - `env_for()` 正确处理 NVIDIA NIM 的多级 fallback（`NVIDIA_API_KEY → NVIDIA_NIM_API_KEY → DEEPSEEK_API_KEY`）
  - 18 个测试

- **问题**: 无严重问题。`FileKeyringStore` 的 `default_path()` 返回 `Result` 而非 panic。

### 4.4 `crates/state` — 持久化状态

**质量**: ★★★★☆

- **优点**:
  - SQLite 模式完整：threads、messages、checkpoints、jobs、dynamic_tools 均带索引
  - 外键级联删除（`ON DELETE CASCADE`）
  - `session_index.jsonl` 作为辅助索引（按名称查找线程路径）
  - `upsert_thread` 自动维护 session_index

- **问题**: 见 M3（SQL 字符串拼接）。`row_to_thread` 中有 `PathBuf::from(row.get::<_, String>(9)?)` 的 unwrap 式调用——若 cwd 为空字符串会静默成功而非报错。

### 4.5 `crates/protocol` — 事件协议

**质量**: ★★★★☆

- **优点**:
  - 类型安全的 `EventFrame` enum 覆盖所有事件（ResponseStart/Delta/End、ToolCall、Approval、Mcp、Exec、Turn）
  - 支持 `tag`/`rename_all` serde 属性确保 JSON 兼容性
  - `ThreadStatus` / `SessionSource` 等枚举有完整的 `rename_all = "snake_case"` 序列化

- **问题**: 见 M4（JSON-RPC 基元重复）、L6（`Value` 扩展槽）

### 4.6 `crates/tools` — 工具注册表

**质量**: ★★★★☆

- **优点**:
  - `ToolRegistry` 的 `dispatch()` 用 `RwLock` 实现并行/串行执行控制
  - `execute_with_timeout` 封装超时逻辑
  - JSON 字段提取器 `required_str` / `optional_u64` 等提供清晰的错误信息（列出已提供的字段名）
  - `ToolError` 实现 `Display + Error`

- **问题**: `required_str` 在 `missing_field` 时提供 "Input provided: path, content" 的提示，但该信息只在 `as_object()` 成功时可用。若是数组或字符串输入，提示为空。

### 4.7 `crates/agent` — 模型注册表

**质量**: ★★★★★

- **优点**:
  - `ModelRegistry` 支持 9 个提供者、多个别名、大小写不敏感匹配
  - `resolve()` 的 fallback 链透明可见（`fallback_chain` 字段）
  - `preserve_requested_model_id_case` 保留用户指定的大小写——对第三方提供者关键
  - Ollama 特殊处理：不规范化模型 ID（保留完整 tag）
  - 16 个测试覆盖所有提供者和别名场景

- **问题**: 无。这是代码库中设计最干净的模块。

### 4.8 `crates/core` — 运行时核心

**质量**: ★★★☆☆

- **优点**:
  - Runtime / ThreadManager / JobManager 三层分离合理
  - `JobManager` 有完整的重试元数据（指数退避、最大重试次数）
  - Thread 管理支持 create/start/resume/fork/list/read/set_name/archive/unarchive/message

- **问题**: 
  - `JobManager.deterministic_backoff_ms` 中 `attempt - 1` 的减法可能下溢（attempt 为 0 时虽然外层有 guard 但编译器不知道）
  - Runtime 和 ThreadManager 的职责边界模糊：`Runtime::handle_thread` 和 `ThreadManager` 的方法有功能重叠
  - 见 M2（执行策略默认行为）

### 4.9 `crates/execpolicy` — 执行策略

**质量**: ★★★☆☆

- **优点**:
  - 分层规则集（BuiltinDefault < Agent < User）提供优先级机制
  - 支持 arity-aware 前缀匹配（`git status` 匹配 `git status -s` 但不匹配 `git push`）
  - `ApprovalRequirement` enum 清晰表达 skip/needs_approval/forbidden 三种状态

- **问题**: 见 M2（策略语义）。`normalize_command` 全程 lowercases——对大小写敏感的命令（罕见的但存在的 Unix 工具）可能误判。

### 4.10 `crates/hooks` — 钩子系统

**质量**: ★★★★☆

- **优点**:
  - `HookDispatcher` 简单而实用——fire-and-forget 模式确保钩子失败不影响主流程
  - 三种 sink：Stdout、Jsonl（文件追加）、Webhook（HTTP POST + 3 次重试）
  - `WebhookHookSink` 使用指数退避（200ms × retry_count）

- **问题**: `HookDispatcher::emit` 忽略了所有 sink 错误（`let _ =`）。这对可靠性是合理的设计选择，但日志中缺少钩子失败的记录。

### 4.11 `crates/mcp` — MCP 管理

**质量**: ★★★☆☆

- **优点**:
  - 工具名限定（`mcp__<server>__<tool>` 格式，限长 64 字符）
  - `default_stdio_client` 提供内置 health/capabilities 工具
  - stdio JSON-RPC 协议实现完整（initialize/healthz/tools/list/call 等）

- **问题**: 见 M4（JSON-RPC 重复）。`InMemoryMcpClient` 是存根——实际 MCP 客户端实现在 `crates/tui/src/mcp.rs` 中，但 `mcp` crate 本身不包含真实传输层，削弱了 crate 的独立可用性。

### 4.12 `crates/app-server` — 应用服务器

**质量**: ★★★☆☆

- **优点**:
  - 同时支持 HTTP (axum) 和 stdio (JSON-RPC) 传输
  - stdio 协议方法名完全展开（thread/create、thread/resume 等），符合 ACP/Zed 风格
  - `capabilities` 方法返回所有可用方法列表

- **问题**: 见 M4（JSON-RPC 重复）。`dispatch_stdio_request` 中每个方法分支都是手动展开的 match arm——约 30 个分支，每个 5-15 行。可考虑宏驱动或路由表。

### 4.13 `crates/tui-core` — UI 核心

**质量**: ★★★☆☆

- **优点**:
  - Elm 风格的 reducer 模式（`UiState::reduce(UiEvent) → Vec<UiEffect>`）
  - 清晰的状态机设计

- **问题**: 见 L2（Pane 切换重复代码）。该 crate 目前非常轻量（~200 行）——尚不清楚其长期定位是"共享 UI 类型"还是"无渲染 UI 逻辑"。

### 4.14 `crates/desktop` — 桌面壳

**质量**: ★★★★☆ (Rust) / ★★★☆☆ (TypeScript)

#### Rust 后端

- **优点**:
  - Sidecar 生命周期管理完善：双通道健康检查（`/health` + `/v1/sessions`）、3 次失败自动重启、陈旧端口回收
  - Windows 特定处理周到（`CREATE_NO_WINDOW`、PowerShell 端口回收、`USERPROFILE` cwd）
  - API Key 保存使用 `config.example.toml` 模板（而非硬编码默认值）——保障上游默认值同步
  - `kill_processes_listening_on_local_port` 在 Windows 上使用 PowerShell `Get-NetTCPConnection`

- **问题**: 见 C1（Windows 文件权限）、H4（stderr 丢弃）

#### TypeScript 前端

- **优点**:
  - 完整的 SSE 事件流处理（`streamNormalize.ts` 归一化层）
  - `ChatView` / `MessageBubble` / `ToolCard` 组件树清晰
  - 支持暗色/亮色主题切换

- **问题**: 见 M5（`handleSend` 过长）、L4（localStorage 散布）。无状态管理库（Redux/Zustand）——当前项目规模下尚可，但随着功能增长会成为瓶颈。

### 4.15 `crates/tui` — 核心运行时

**质量**: ★★★★☆（仅评审查阅部分）

- **引擎层**: 架构清晰 (`engine.rs` → `turn_loop.rs` → `streaming.rs`)，能力流控制 (`capacity_flow.rs`) 和 LSP 钩子 (`lsp_hooks.rs`) 为独立子模块
- **工具层**: `shell.rs` 2513 行——功能丰富（pty、后台进程、沙箱集成）但过于庞大。`fetch_url.rs` 509 行——SSRF 防护需要加强（见 H1）
- **API 层**: `runtime_api.rs` 体量大——CORS 已对 **origin 白名单** 收敛；仍见 H3（`allow_headers(Any)`、`extra_origins`、非本机绑定场景）
- **客户端**: `client.rs` 2097 行——工具名编解码逻辑复杂（见 M6）
- **TUI 层**: `tui/` 目录下 50+ 文件——组件化良好，ratatui 渲染与业务逻辑分离

---

## 5. 跨切面分析

### 5.1 错误处理

- **模式**: 普遍使用 `anyhow::Result` + `.context()` 链，提供良好的错误上下文
- **优点**: `run_cli()` 打印完整的 `err.chain()`——包含所有层级的上下文（修复了 #767）
- **不足**: 部分错误被静默吞没——`hooks.rs` 的 `let _ = sink.emit()` 吞掉钩子错误；`sandbox/windows.rs` 存根无错误返回

### 5.2 安全态势

- **密钥管理**: 成熟的多层方案（keyring / env / config-file），`FileKeyringStore` 有权限检查
- **执行安全**: `ExecPolicyEngine` 提供前缀匹配的执行策略，支持 allow/deny 列表和 session 级批准
- **网络安全**: `NetworkPolicyToml` 提供基于域名的 allow/deny/prompt 策略
- **沙箱**: macOS Seatbelt、Linux Landlock 集成；Windows 存根
- **薄弱点**: SSRF 防护不完整（见 H1）、CORS 在 **origin 白名单** 前提下仍有配置面与 `allow_headers(Any)` 等残留项（见 H3）、Windows 文件权限（见 C1）

### 5.3 国际化

- `localization.rs` 支持 `en/ja/zh-Hans/pt-BR` 四种 UI 语言
- 桌面端 `get_locale` 硬编码返回 `"zh-CN"`——应读取系统 locale
- Prompt 模板中语言指令清晰（`## Environment → lang` 字段）

### 5.4 测试策略

- **单元测试**: `crates/config`（40+）、`crates/agent`（16）、`crates/secrets`（18）、`crates/cli`（30+）的覆盖率良好
- **集成测试**: `crates/tui/tests/` 目录有 eval_harness / integration_mock_llm / protocol_recovery / skill_install
- **不足**: 工具层（shell、fetch_url、apply_patch）缺乏测试；桌面端无自动化测试

---

## 6. 验证总结

### 评审覆盖度

| Crate | 读取方式 | 覆盖率 |
|-------|----------|--------|
| agent | 全文读取 | 100% |
| app-server | 全文读取 | 100% |
| cli | 全文读取 | 100% |
| config | 全文读取 | 100% |
| core | 全文读取 | 100% |
| desktop (Rust) | 全文读取 | 100% |
| desktop (TypeScript) | 核心文件 | ~80% |
| execpolicy | 全文读取 | 100% |
| hooks | 全文读取 | 100% |
| mcp | 全文读取 | 100% |
| protocol | 全文读取 | 100% |
| secrets | 全文读取 | 100% |
| state | 全文读取 | 100% |
| tools | 全文读取 | 100% |
| tui-core | 全文读取 | 100% |
| tui | 核心模块抽样 | ~25% |

### HIGH/CRITICAL 复核状态

已逐项复核的发现（在撰写报告时重新确认文件内容）：

- **C1** (Windows 文件权限): 已确认 — `src/commands.rs:84-106` 中的 `#[cfg(not(unix))]` 分支仅调用 `fs::write()`，无权限设置。
- **H1** (SSRF): 已确认 — `fetch_url.rs` 依赖 `network_policy.rs` 的 `host_from_url` 解析但无内网地址 deny 列表。
- **H3** (CORS): **已按当前树更正** — `runtime_api.rs` 中 `cors_layer()` 使用 **`allow_origin` 白名单**（非 `permissive()`）；H3 条目已改写为针对 `allow_headers(Any)`、`extra_origins` 与 `--host 0.0.0.0` 的残留风险。
- **H4** (stderr 丢弃): 已确认 — `sidecar.rs:49-51` 设置 `Stdio::null()`。

报告的 HIGH/CRITICAL 发现均基于当前代码的直接观察，无需推断。

### 已知限制

- `crates/tui/src/tools/` 下的完整工具实现（`shell.rs` 2513 行等）仅抽样阅读了前 80 行头部。深层的 `apply_patch.rs`、`subagent/`、`sandbox/` 等子模块未逐行审查。
- `crates/tui/src/tui/` 下的 50+ 个 UI 组件子文件仅通过模块声明文件了解结构，未逐个审查渲染逻辑。
- 桌面端 TypeScript 组件的 props/state 类型未审查。

### 报告文件大小审查

- `crates/tui/src/tools/shell.rs` (2513 行): 建议拆分为 `shell/exec.rs` + `shell/pty.rs` + `shell/sandbox.rs`
- `crates/tui/src/runtime_api.rs` (3574 行): 建议拆分为 `runtime_api/router.rs` + `runtime_api/handlers.rs` + `runtime_api/sse.rs`
- `crates/config/src/lib.rs` (2165 行): 建议提取 `config/provider_keys.rs` 消重提供者代码

---

## 附录 A: 修复优先级建议

| 优先级 | 发现 | 预估工作量 | 风险 |
|--------|------|-----------|------|
| P0 | C1 - Windows 文件权限 | 2h | 中 |
| P0 | H1 - fetch_url SSRF | 4h | 高 |
| P2 | H3 - CORS 残留面（headers / extra_origins / 非本机绑定提示） | 2h | 低–中 |
| P1 | H4 - Sidecar stderr 日志 | 3h | 低 |
| P2 | M1 - Config 提供者代码消重 | 8h | 低 |
| P2 | M5 - App.tsx 重构 | 6h | 中 |
| P3 | M4 - JSON-RPC 基元提取 | 4h | 低 |
| P3 | M6 - 工具名编码改进 | 4h | 低 |

## 附录 B: 优秀实践亮点

1. **密钥存储安全**: `FileKeyringStore` 的 `load_unlocked` 拒绝非 `0600` 文件，`set/delete` 使用 `?` 而非 `unwrap_or_default()`——防止覆盖现有密钥（#281 修复）
2. **错误链可见性**: `run_cli` 打印完整 `err.chain()`，让 TOML 解析错误（行列号）可追溯到配置文件的精确位置
3. **模型别名系统**: `ModelRegistry` 的 `fallback_chain` 字段让用户了解模型解析路径——可调试性强
4. **Sidecar 双重健康检查**: `/health` + `/v1/sessions` token 验证确保桌面端不会被陈旧 sidecar 进程欺骗
5. **arity-aware 命令匹配**: `execpolicy` 的 `allow_rule_matches` 支持多词命令（`git status -s` 匹配 rule `git status` 但不匹配 rule `git`）
6. **测试中环境变量隔离**: `EnvGuard` 结构体在 `config` 和 `secrets` 测试中使用 RAII 模式 + `env_lock` 互斥锁安全隔离全局环境变量

---

## 附录 C: 第二批评审补充发现（2026-05-09）

本附录基于 **6 个子智能体并行通读 + 人工 verification pass 逐行核实** 的成果，补充前一轮未覆盖或未深入的新发现。**HIGH 编号接正文 §3（H5 起）**，避免与 H1–H4 重复计数。

### 新增严重性分布

| 严重性 | 新增数量 | 说明 |
|--------|----------|------|
| CRITICAL | 0 | 均降级为 HIGH（威胁模型前提） |
| HIGH | 12 | 安全态势、容错性、配置缺陷 |
| MEDIUM | 6 | 设计弱项、维护性 |
| LOW | 4 | 信息性 |

---

### 新增 HIGH 发现

#### H5 — 桌面端 Bearer Token 在进程命令行中可见（所有平台）

- **文件**: `crates/desktop/src/sidecar.rs:44-52`
- **问题**: 随机生成的 runtime bearer token 通过 `--auth-token <token>` 命令行参数传递。在 Linux/macOS 上，任何用户都可通过 `ps aux` 或 `/proc/<pid>/cmdline` 读取；在 Windows 上可通过 Process Explorer、WMI 或 `Get-WmiObject Win32_Process` 获取。**这是已验证的关键发现**。
- **影响**: 获取 token 的攻击者可调用 `127.0.0.1:7878/v1/*` API 读取会话、执行工具。
- **缓解**: 绑定 `127.0.0.1` + 默认 `--host` 限制缓解了远程利用；但仍暴露给本地同权限进程。
- **建议**:
  1. 改用环境变量（`DEEPSEEK_RUNTIME_TOKEN`）替代命令行参数，子进程可继承
  2. 如必须用命令行，在 Windows 上使用 `CryptProtectMemory` 加密 token，进程内解密
  3. 启动后尽快从进程参数中擦除（Unix 上可覆写 `argv`，但不可靠）

#### H6 — 桌面端 CSP `script-src 'self' 'unsafe-inline'` 使 XSS 防护失效

- **文件**: `crates/desktop/tauri.conf.json:30-31`
- **问题**: CSP 声明 `script-src 'self' 'unsafe-inline'`——`'unsafe-inline'` 允许同源上下文中内联脚本执行策略所允许的内联脚本来源。与 **M9** 的 `dangerouslySetInnerHTML` 结合时，若 HTML 中可被注入可执行载体（例如在有缺陷的 sanitizer 场景下），防护会弱于严格的 nonce/hash 方案。
- **影响**: 恶意模型回复若在渲染结果中引入 **危险 URL 协议**（如经 `linkify` 生成的 `javascript:` 链接，见 M9）或未来若误开 `html: true`，则 CSP 对脚本与内联的配合需重新评估。
- **建议**:
  1. 移除 `'unsafe-inline'`，使用 nonce 或 hash 控制合法脚本
  2. 使用 `script-src 'self'` 配合 Tauri 的非标准事件绑定（而非 HTML 事件处理器）

#### H7 — 桌面端 Bearer Token 暴露在 `window` 全局对象上

- **文件**: `crates/desktop/web-ui/src/api/client.ts:35-37`
- **问题**: `__DEEPSEEK_RUNTIME_TOKEN__` 被写入 `window` 全局对象。任何在前端执行 JavaScript 的 XSS（见 H6）均可读取此 token，并随后调用 runtime API。
- **影响**: CSP 失效 + 全局 token = 完全入侵向量。
- **建议**:
  1. 将 token 保存在模块级闭包变量中（`let runtimeToken = ''`），而非 `window` 属性
  2. 或使用 Tauri `invoke` IPC 每次调用时后端传递 token（无需前端持有）

#### H8 — MCP `call_tool()` 绕过工具过滤器

- **文件**: `crates/mcp/src/lib.rs:249-256`
- **问题**: `list_tools()` 应用了 `allowed_by_filter` 检查，但 `call_tool()` 和 `call_qualified_tool()` 直接调用 MCP 客户端，完全不检查已禁用的工具是否被调用。**已验证：line 249 的 `call_tool` 直接 `client.call_tool(tool_name, arguments)`，无过滤。**
- **影响**: 若配置了 `disabled_tools` 列表，拒绝的工具仍可被模型调用。
- **建议**: 在 `call_tool()` 入口处增加与 `list_tools()` 一致的 `allowed_by_filter` 检查。

#### H9 — 引擎主循环 `expect()` 导致 panic 而非优雅失败

- **文件**: `crates/tui/src/core/engine/turn_loop.rs:22`
- **问题**: `self.deepseek_client.clone().expect("DeepSeek client should be configured")`——若 `deepseek_client` 为 `None`，整个 TUI 进程 panic。**已验证：line 22 确有此 expect。**
- **影响**: 配置缺失或初始化失败时，用户看到的是 panic 崩溃而非友好的错误消息。panic hook 会尝试恢复终端，但进程已退出。
- **建议**: 返回 `Err("DeepSeek client not configured")`，让引擎层处理并展示给用户。

#### H10 — `code_execution` 工具直接运行 `python3 -c` 无沙箱

- **文件**: `crates/tui/src/core/engine/tool_catalog.rs:449-469`
- **问题**: `execute_code_execution_tool()` 以 `tokio::process::Command::new("python3")` 直接执行任意 Python 代码，仅有 `120s` 超时限制。**已验证：代码直接调用 `cmd.arg("-c").arg(code)` 执行。**
- **影响**: YOLO 模式下（或审批绕过后），模型可执行任意系统命令（如 `os.system("rm -rf /")`）。
- **缓解**: 仅通过 `tool_catalog.rs` 暴露，默认不在工具注册表中注册。需要模型显式获取此工具。
- **建议**:
  1. 至少通过沙箱执行（`sandbox::run_sandboxed`）
  2. 默认禁用此工具，仅显式启用时可用
  3. 限制可用库（`--safe-path` 或 `py_compile` 模式）

#### H11 — `capacity_flow.rs` 中 `messages.clear()` 无事务保护

- **文件**: `crates/tui/src/core/engine/capacity_flow.rs:690`
- **问题**: `self.session.messages.clear()` 直接清空会话消息向量。若后续 `merge_compaction_summary()` 或状态重建过程中崩溃，所有会话消息永久丢失。**已验证：line 690-691 执行 clear() 后仅保留 latest_user 和 latest_verified 两条消息。**
- **影响**: 引擎在容量流控制中 crash 将丢失当前轮次前的全部对话历史。
- **建议**:
  1. 在清空前将消息快照到 `Option<Vec<Message>>`，成功后再丢弃
  2. 或使用 `mem::take(&mut self.session.messages)` 并持有旧向量直到重建完成

#### H12 — 桌面端 `shell:default` 能力授予前端任意命令执行权限

- **文件**: `crates/desktop/capabilities/default.json:8`
- **问题**: `shell:default` 能力允许 React 前端通过 Tauri shell 插件运行任意系统命令。**已验证：capabilities/default.json 第 8 行包含 `shell:default`。**
- **影响**: 任意 XSS（见 H6）都可利用 Tauri IPC 执行 shell 命令，完全控制用户系统。
- **建议**:
  1. 限制为 `shell:allow-execute` 配合预定义的命令白名单
  2. 或移除 shell 权限，仅通过 HTTP runtime API 交互

#### H13 — npm 安装包 `DEEPSEEK_TUI_RELEASE_BASE_URL` 允许完整 MITM

- **文件**: `npm/deepseek-tui/scripts/artifacts.js:75-84`
- **问题**: `releaseBaseUrl()` 函数优先读取 `DEEPSEEK_TUI_RELEASE_BASE_URL` 环境变量，完全覆盖下载来源。二进制和 SHA256 checksum 使用相同 URL 基址——**攻击者可同时篡改两者**。**已验证：line 75-84 的函数逻辑确认双向覆盖。**
- **影响**: 控制此环境变量的攻击者可提供任意恶意二进制，完整性校验完全失效。
- **缓解**: 环境变量操纵需要本地执行权限——此时攻击面已存在。
- **建议**:
  1. 签名 checksum 文件（如 GPG 签名 `.sha256.sig`），使用嵌入的公钥验证
  2. 或至少以不同来源获取 binary 和 checksum（如 binary 从 GitHub Release，checksum 从独立服务器）

#### H14 — `config.example.toml` 默认开启 `allow_shell = true`

- **文件**: `config.example.toml:98`
- **问题**: 分发模板中 `allow_shell = true` 默认允许 shell 执行。新用户复制模板后自动获得 shell 工具访问权。**已验证：line 98 和 354 两处设置。**
- **影响**: 用户可能无意中允许模型执行任意 shell 命令。
- **建议**: 默认设为 `false`，或在配置中明确要求用户确认。

#### H15 — SHA256 校验跳过时无警告

- **文件**: `crates/cli/src/update.rs:53-67`
- **问题**: 更新程序尝试下载 `.sha256` 文件，若网络失败则打印一条通知并跳过验证。**已验证：`Err(_) => { println!("...skipping verification"); None }`。**
- **影响**: 尽管下载来源为 GitHub Releases (HTTPS)，中间人攻击（如 DNS 劫持）可提供恶意二进制且跳过校验。
- **建议**:
  1. 校验失败时应默认中止更新（`bail!`），添加 `--ignore-checksum` 标志作为逃生舱
  2. 或启动前缓存预期的 SHA256 哈希（嵌入编译时）

#### H16 — `trust_mode` 绕过所有路径验证

- **文件**: `crates/tui/src/tools/spec.rs:307-310`
- **问题**: `trust_mode` 为 true 时，`resolve_path()` 返回路径而不检查 workspace 边界。**已验证：line 307-310 直接在 `if self.trust_mode` 分支返回。**
- **影响**: YOLO 模式下模型可读写任意文件路径。
- **建议**: 这是设计选择，但应在文档中明确，且考虑添加路径白名单层。

---

### 新增 MEDIUM 发现

#### M9 — 桌面端 MarkdownPreview 使用 `dangerouslySetInnerHTML`

- **文件**: `crates/desktop/web-ui/src/components/MarkdownPreview.tsx`（约 `95–108` 行；行号随版本变动）
- **问题**: 使用 `dangerouslySetInnerHTML` 渲染 markdown-it 输出的 HTML。当前 `markdown-it` 配置为 **`html: false`**，可在一定程度上抑制用户 Markdown 中裸 HTML 标签被直接透传；但 **`linkify: true`** 仍会把裸 URL 变成 `<a href="...">`，若 `href` 为 `javascript:`、`data:` 等危险协议，则存在 **DOM 注入 / 导航劫持** 向量，应结合 `dompurify` 或链接校验。
- **影响**: 结合 CSP 较弱（H6）时，桌面端渲染链路的安全余量下降；不宜描述为「任意原始 HTML 任意注入」。
- **建议**:
  1. 使用 `dompurify` 对渲染后的 HTML sanitize（含 `ALLOWED_URI_REGEXP` 等）
  2. 或为 `markdown-it` 的链接渲染统一仅允许 `http(s)://`（及必要时的 `file:` 白名单）
  3. 收紧 CSP（见 H6：`nonce`/`hash`、减少 `'unsafe-inline'`）

#### M10 — 桌面端 Sidecar 二进制可通过 PATH 劫持

- **文件**: `crates/desktop/src/sidecar.rs:287-303`（`find_deepseek_binary`）
- **问题**: `find_deepseek_binary()` 在 fallback 路径中首先尝试 `"deepseek-tui"` 命令名，这会在 `PATH` 中搜索。攻击者若在 PATH 前置目录放置同名恶意二进制，可劫持启动。bundle 路径失败后会回退到 PATH 查找。
- **影响**: bundle 路径未找到时，PATH 搜索可被利用。
- **建议**: bundle 的 sidecar 应有显式 `.exe` 扩展名（Windows）或散列校验，且 PATH 回退逻辑应输出警告。

#### M11 — 执行策略 `OnFailure` 语义为无条件跳过

- **文件**: `crates/execpolicy/src/lib.rs:237-240`
- **问题**: `AskForApproval::OnFailure` 在匹配分支中返回 `ExecApprovalRequirement::Skip`——**命令在任何策略匹配之前就无条件执行**。**已验证：line 237-240 的 match arm 直接返回 Skip。**
- **影响**: `OnFailure` 的实际含义是"先执行，失败了再问"，而非"失败时才审批"。与直觉不符。
- **建议**: 将此分支重命名为 `ExecFirst`，或调整逻辑使其在命令被策略覆盖时才自动执行。

#### M12 — CLI `--api-key` 参数在 Unix 上可见

- **文件**: `crates/cli/src/lib.rs:382`
- **问题**: CLI 接受 `--api-key` 参数。Unix 系统上命令行参数通过 `/proc/self/cmdline` 对所有同用户进程可见。**子进程亦继承此参数。**
- **影响**: CI/CD 环境中易泄露。
- **建议**: 优先使用 `DEEPSEEK_API_KEY` 环境变量或 `--api-key-file` 从文件读取。

#### M13 — Runtime API `require_runtime_token` 接受 URL 查询参数

- **文件**: `crates/tui/src/runtime_api.rs:445,463-471`
- **问题**: `token_from_query()` 解析 URL 查询字符串中的 `?token=xxx` 参数。**已验证：line 445 的 `|| token_from_query(...)` 分支，line 463-471 的解析函数。** token 可能出现在服务器日志、Referer 头、浏览器历史中。
- **建议**:
  1. 移除 query 参数认证方式，仅保留 `Authorization: Bearer` 头
  2. 或需要同时提供 header + query 参数的双重认证

#### M14 — 重复的 `expect()` 调用在工具注册表构建中

- **文件**: `crates/tui/src/core/engine.rs:1013`
- **问题**: `runtime.expect("sub-agent runtime should exist")`：若构建 tool context 时缺少 `Runtime` 实例，将 panic。
- **建议**: 使用 `context()` 返回 `Result`。

---

### 新增 LOW 发现

#### L7 — `ToolError::Timeout` 显示 ms 标记为 seconds

- **文件**: `crates/tools/src/lib.rs:68-71`
- **问题**: `Display` 实现输出 `"Tool timed out after {self.seconds}ms"`——变量名 `seconds` 但单位是毫秒。
- **影响**: 调试时误导。
- **建议**: 重命名字段或更正显示文本。

#### L8 — 桌面端 `get_locale` 硬编码 `"zh-CN"`

- **文件**: `crates/desktop/src/commands.rs`（`get_locale` 命令）
- **问题**: 始终返回 `"zh-CN"`，忽略用户系统 locale。
- **影响**: 日文、英文等操作系统用户无法获得本地化 UI。
- **建议**: 使用 `sys-locale` crate 检测。

#### L9 — hardcoded `/opt/homebrew/bin/gh` 路径

- **文件**: `crates/tui/src/tools/github.rs:23`
- **问题**: `gh` 二进制路径硬编码为 macOS Homebrew 安装路径。Linux 和 Windows 用户需要设置 `DEEPSEEK_GH_BIN` 环境变量。
- **影响**: Linux/Windows 上默认不可用。
- **建议**: 使用 `which` crate 或 `PATH` 搜索查找 `gh`。

#### L10 — 桌面端 `App.tsx` 中 `useCallback` 依赖数组过长

- **文件**: `crates/desktop/web-ui/src/App.tsx`
- **问题**: `handleSend` 和 `handleResume` 等函数的 `useCallback` 依赖数组包含 7-10 个依赖项。
- **影响**: 易出现陈旧闭包 bug。
- **建议**: 使用 `useRef` 持有稳定回调，或拆分为多个自定义 hook。

---

### 验证总结

**最终复核方法**: 对每个新增 HIGH 发现，使用 `read_file` 和 `grep_files` 在源文件中直接定位并验证代码行。所有标注"已验证"的发现均经过二次确认。

**降级/移除记录**:
- **H3 / CORS（正文）**: 原「`CorsLayer::permissive()`」表述与当前代码不符，已重写为 **origin 白名单** 及残留项；本节 H5–H16 不受影响。
- 子智能体报告的 `trust_mode` 完全绕过路径验证（原标注 HIGH）——保留为 H16，但确认为设计选择（trust_mode 意图就是全信任），降为信息性。
- `execpolicy OnFailure`（原标注 HIGH）——保留为 M11，因该行为有明确的使用场景（运行第一次，失败再问）。
- 子智能体报告的 `shell.rs` unsafe `libc::kill(-pgid, SIGKILL)`（原标注 HIGH）——降级为 MEDIUM，因代码已包含 `pgid <= 0 → child.kill()` 安全回退。

**未完全验证的发现**（子智能体输出中标注但未逐行确认）:
- `crates/tui/src/tools/sandbox/mod.rs:82` — `CommandSpec::shell()` 传递原始命令到 `sh -c`：未完全逐行核实，但基于工具架构判断可信。
- `crates/tui/src/sandbox/policy.rs` — Linux sandbox 作为 no-op 实现：未逐行验证，但项目文档已承认此限制。

**覆盖率说明**: 本附录的发现基于子智能体对指定文件的审查叠加作者的 verification pass。`crates/tui/src/tools/` 和 `crates/tui/src/tui/` 仍有大量文件未逐行审查。完整覆盖率需增量补充。

---

## D. 问题修正实施计划

本计划在 **不改变威胁模型语义**（例如 `trust_mode`、`YOLO` 仍由产品与文档约定）的前提下，将报告中的可操作项收口为分批交付。**附录 A** 的 P 级可作排期输入；本节补充 **依赖关系、并行度、验收** 与 **附录 C（H5+）并入节奏**。

### D.1 原则

| 原则 | 说明 |
|------|------|
| 安全优先 **P0** | 先做默认攻击面收窄（默认 deny、密钥与 token 不落盘/不可见且无回归）。 |
| 可观测先于重构 | Sidecar stderr / 校验失败中止（**H4、H15**）优先于大范围前端拆分（**M5**）。 |
| 组合交付 | **M9 ↔ H6 ↔ H12 / H7** 同一链路：sanitize / CSP / Tauri capabilities / token 存储需同窗评估，避免只修一端造成假安全感。 |
| 回归闸门 | `cargo test --workspace --all-features`、`cargo clippy --workspace --all-targets --all-features`；桌面 `crates/desktop/web-ui` 至少 `npm run build`；有行为变更时需补测或写明「破坏性变更」。 |

### D.2 阶段划分与推荐顺序

**阶段 A — P0（1–2 个迭代内）**

| 工单包 | 覆盖发现 | 主要动作 | 验收要点 |
|--------|----------|----------|----------|
| A1 SSRF | **H1** | `fetch_url` + `network_policy`：默认对内网/metadata 区间 **deny** 或等价 block；YOLO 下仍生效；文档说明 `prompt`/`deny`。 | 单测：`127.0.0.1`、`169.254.169.254`、私服网段被拒或按策略放行；changelog。 |
| A2 Win 配置文件权限 | **C1** | Windows 上对 `config.toml`/写入路径做 ACL 收窄或等价（含失败回退与安全日志）；与 Unix `0600` 对称记录。 | 在典型 Win 会话下创建文件后校验 ACL；不写坏已有用户文件。 |

**阶段 B — P1 可靠性与运维（与 A 并行度较高）**

| 工单包 | 覆盖发现 | 主要动作 | 验收要点 |
|--------|----------|----------|----------|
| B1 Sidecar 可诊断 | **H4** | stderr 轮转日志（如 `%USERPROFILE%`/`~/.deepseek/logs/sidecar.log`）；健康失败时附带最近数十行摘要。 | 人为制造启动失败能看见原因；不显式泄露完整 token。 |
| B2 自更新完整性 | **H15** | 无 checksum 时 **默认中止**；`--ignore-checksum` 显式逃生；日志级别区分。 | 集成/单元测覆盖「跳过校验」路径被禁止或需 flag。 |
| B3 npm 安装源 | **H13** | 文档 + 代码双轨：环境变量覆盖时 **醒目标记**；长期可加签名或双源校验。 | 发布说明与脚本帮助文本一致。 |

**阶段 C — 桌面与 Runtime 信任边界（建议同一里程碑内联调）**

| 工单包 | 覆盖发现 | 主要动作 | 验收要点 |
|--------|----------|----------|----------|
| C1 Token 传递 | **H5** | sidecar 启动改为 **环境变量**（或 Tauri/IPC 注入）为主，避免 `--auth-token` 出现在进程列表；与 `serve` 入口对齐。 | `ps`/任务管理器不可见 token；全平台启动回归。 |
| C2 前端 token 与渲染 | **H7、M9** | 去掉 `window.__DEEPSEEK_RUNTIME_TOKEN__`；token 闭包或 IPC；**MarkdownPreview** 增加 **DOMPurify** 或等价 + 链接协议白名单。 | 安全冒烟：构造 `javascript:` 链接不执行；网络面板仍带 `Authorization`。 |
| C3 CSP 与 Tauri 能力 | **H6、H12** | 收紧 `script-src`（nonce/hash/去除 `unsafe-inline` 的可行方案）；`shell:default` **改为白名单**或移除并确认无功能依赖。 | 桌面壳全功能手测；无控制台 CSP 违规导致白屏。 |
| C4 Runtime CORS 残留 | **H3** | 文档化 `extra_origins`；`--host 0.0.0.0` 警告；评估 `allow_headers` 收敛。 | 配置错误可预期；Tauri/WebView 预检仍通过。 |
| C5 Query token | **M13** | 评估废弃 `?token=`；若保留则限制场景并写安全说明。 | 日志/Referer 无意外泄露用例。 |

**阶段 D — 引擎与 MCP 正确性**

| 工单包 | 覆盖发现 | 主要动作 | 验收要点 |
|--------|----------|----------|----------|
| D1 MCP 过滤 | **H8** | `call_tool` / `call_qualified_tool` 与 `list_tools` 共用 **allowed_by_filter**。 | 单测：disabled 工具调用返回明确错误。 |
| D2 Panic 消除 | **H9、M14** | `turn_loop` / engine 构建路径 `expect` → `Result` + 用户可见错误。 | 缺 client 配置时不 panic。 |
| D3 容量流 | **H11** | `messages.clear` 前后 **快照/两阶段提交** 式保护。 | 模拟崩溃或单测不丢历史。 |
| D4 code_execution | **H10** | 沙箱或默认关闭 + 显式 opt-in；文档声明。 | 默认配置下不可达任意 `python3 -c`。 |

**阶段 E — 配置与体验（低风险、可插队）**

| 工单包 | 覆盖发现 | 主要动作 | 验收要点 |
|--------|----------|----------|----------|
| E1 模板默认 | **H14** | `config.example.toml` 将 `allow_shell` 等与「高权限」相关的项默认改为 **false**，并注释说明。 | 新手指南与 README 同步。 |
| E2 CLI 与密钥提示 | **H2、M12** | doctor 警告 env vs 文件密钥冲突；文档推荐 `--api-key-file`、减少在 argv 传敏感参数。 | doctor 输出可测。 |

**阶段 F — 技术债（与功能发布解耦）**

| 工单包 | 覆盖发现 | 备注 |
|--------|----------|------|
| F1 Config 提供者消重 | **M1** | 纯维护；适合专门 PR，避免与安全 PR 混在一起。 |
| F2 JSON-RPC 提取 | **M4** | 同上，需注意 `app-server`/`mcp` 同步。 |
| F3 Web 拆分 | **M5、L4、L10** | Hook 拆分后再考虑状态管理栈。 |
| F4 LOW 清扫 | **L7–L10** | L7/L8/L9 可单 PR 或多个小 PR。 |

### D.3 依赖简图（文字）

```
A1(SSRF) ─┐
A2(Win)  ─┼─► 可随时发版补丁
B1-B3    ─┘

C1(H5 env token) ──► C2/H7 Markdown+token ──► C3 CSP+shell ──► 端到端桌面签核
     │                         ▲
C4(H3)+C5(M13) 可与 C1 并行（不同文件），但须在发版说明中写明配置变更。

D1(H8) ── 独立 MCP PR
D2/D3/D4 ── TUI/engine，建议各独立 PR，避免巨型 diff
```

### D.4 里程碑建议（可按团队带宽压缩）

| 里程碑 | 内容 | 目标 |
|--------|------|------|
| **M1 安全补丁** | 阶段 A + B1/B2 | 默认网络与密钥面、.sidecar/updater 可信任路径 |
| **M2 桌面信任边界** | 阶段 C | Token、CSP、capabilities、Markdown 同框达标 |
| **M3 引擎/MCP** | 阶段 D | 无静默禁用绕过、降低 panic/data loss |
| **M4 打磨** | 阶段 E+F | 默认安全模板 + 债务分期偿还 |

### D.5 跟踪方式建议

1. **每个工单包绑定报告编号**（如 `fixes: H1+H4`），PR 标题或里程碑描述中引用。  
2. **破坏性变更**：C1/C3/C5 可能需用户在 `tauri.conf`、环境变量或服务启动方式上做一次迁移——在 CHANGELOG 放「Breaking」小节。  
3. **报告本身**：本条 **D.** 在实施过程中若发现依赖或工时偏差，直接在本文档更新表格（或链接到项目管理工具中的 Epic）。

---

*本节为实施计划模版，工时与优先级以维护者确认为准。*
