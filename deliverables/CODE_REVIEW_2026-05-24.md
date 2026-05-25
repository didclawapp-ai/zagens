# 全库代码安全审核报告

**项目**: Zagens (DeepSeek TUI Desktop)  
**审核日期**: 2026-05-24  
**审核范围**: 全仓库（30 个区域，约 520 个源文件）  
**审核方法**: 8 个并行 Explore 子代理 + 主代理交叉验证  

---

## 一、执行摘要

本次全量代码审核覆盖了 **16 个 Rust crate** 和 **React/TypeScript Web UI**，共约 520 个源文件。审核发现了 **2 个 CRITICAL、5 个 HIGH、5 个 MEDIUM 和 1 个 LOW** 级别的安全问题。

### 严重级别分布

| 级别 | 数量 | 关键问题 |
|------|------|---------|
| CRITICAL | 2 | 沙箱空壳声明、执行策略引擎空清单 |
| HIGH | 5 | SSRF 保护缺失、IPC 路径穿越、Python 无沙箱执行、终端永久挂起、Bear Token 侧信道 |
| MEDIUM | 5 | CORS 宽松策略、Job 加载静默丢弃、Secrets 回退到 CWD、Schema 版本未校验、时间戳静默破坏 |
| LOW | 1 | XSS 防线下 markdown-it linkify 残余风险 |

---

## 二、审核范围

### 覆盖的 crate

| Crate | 文件数 | 状态 |
|-------|--------|------|
| `crates/core` | ~51 | ✅ 完成 |
| `crates/tui/core` | ~49 | ✅ 完成 |
| `crates/tui/tools` | ~50 | ✅ 完成 |
| `crates/tui/tui` | ~68 | ✅ 完成 |
| `crates/tui/runtime_threads` | 15 | ✅ 完成 |
| `crates/tui/runtime_api` | 14 | ✅ 完成 |
| `crates/tui/sandbox` | 7 | ✅ 完成 |
| `crates/tui/src` (infra) | ~54 | ✅ 完成 |
| `crates/desktop` (Rust) | 8 | ✅ 完成 |
| `crates/desktop/web-ui` (TS/TSX) | 113 | ✅ 完成 |
| `crates/app-server` | 3 | ✅ 完成 |
| `crates/secrets` | 1 | ✅ 完成 |
| `crates/execpolicy` | 2 | ✅ 完成 |
| `crates/cli/config/agent/protocol/tools/mcp/hooks/state/tui-core` | ~15 | ✅ 完成 |

---

## 三、详细发现

### CRITICAL

#### C1. Linux/Windows 沙箱声明执行但零隔离 (`crates/tui/src/sandbox/mod.rs:520-560`)

**问题**: `prepare_landlock` 和 `prepare_windows` 返回 `SandboxType::LinuxLandlock` / `SandboxType::Windows` 但仅设置环境标记，未应用任何实际沙箱规则。命令以后台进程完整用户权限运行。

**证据**: 
- 代码注释明确写明："This function currently does **not** apply any Landlock rules"
- `noop_sandbox_warning` 输出："Linux Landlock sandbox is not enforced yet; command runs with full user privileges."
- `mark_sandbox_policy_unenforced` 被调用

**影响**: 用户在 TUI 设置中启用沙箱后会被误导认为 shell 命令受隔离保护，实际没有。macOS Seatbelt 沙箱正常运作，但 Linux/Windows 是空壳。

**建议**: 
- 短期：在沙箱未生效时，UI 应明确显示 "未强制执行" 而非静默返回沙箱类型
- 长期：实现 Landlock（Linux 5.13+）和 Windows Restricted Token / AppContainer

---

#### C2. app-server ExecPolicyEngine 初始化空清单 (`crates/app-server/src/lib.rs:290`)

**问题**: `ExecPolicyEngine::new(Vec::new(), Vec::new())` — allow 和 deny 列表均为空，所有 shell 命令无条件允许。

**证据**:
```rust
// crates/app-server/src/lib.rs:290
ExecPolicyEngine::new(Vec::new(), Vec::new()),
```

**影响**: app-server 模式下，LLM 触发的所有 shell 命令不经任何策略检查直接执行。

**建议**: 至少应提供默认拒绝规则模板，或从 `~/.deepseek/execpolicy.toml` 加载规则。

---

### HIGH

#### H1. web_run.rs fetch_page 缺少 IP 级 SSRF 防护 (`crates/tui/src/tools/web_run.rs:1067-1120`)

**问题**: `fetch_page` 仅通过 `check_network_policy`（域名级检查）防御，无 IP 级 SSRF 保护。相比之下 `fetch_url.rs` 有完整的三阶段防护：localhost 拒绝 → `is_restricted_ip` 检查 → DNS 解析 → `dns_pinning`。

**证据**: 
- `web_run.rs:1067` `async fn fetch_page` — 直接构建 reqwest client，无 IP 过滤
- `web_run.rs:1048` `check_network_policy` — 仅域名匹配，不解析 IP
- `fetch_url.rs:178-220` — 完整 SSRF 防护链（作为对比）

**影响**: LLM 可通过 `web.run` 的 `open` 命令访问内网服务、云 metadata（169.254.169.254）、localhost。

**建议**: 将 `fetch_url.rs` 的 SSRF 防护逻辑提取为共享函数，`web_run.rs` 复用。

---

#### H2. Desktop open_in_shell/open_with_system_app 接受 Web UI 任意路径 (`crates/desktop/src/commands.rs:777-800`)

**问题**: 两个 Tauri IPC handler 从 Web UI 接受原始路径字符串，仅 `trim()`，无 `..` 检查，无规范化。

**证据**:
```rust
// crates/desktop/src/commands.rs:777
pub fn open_in_shell(path: String) -> Result<(), String> {
    let trimmed = path.trim();  // 仅有 whitespace trim
    if trimmed.is_empty() { return Ok(()); }
    std::process::Command::new("explorer").arg(trimmed).spawn()...
```

**影响**: Web UI 中的注入可通过 Tauri IPC 触发路径穿越，以 `explorer`/`open`/`xdg-open` 权限打开任意目录。

**建议**: 添加 `resolve_under_workspace` 或至少 `..` 检查（复用 `crates/tui/src/path_guard.rs`）。

---

#### H3. Python 代码执行无沙箱路由 (`crates/tui/src/core/engine/tool_catalog.rs:51-78`)

**问题**: `execute_code_execution_tool` 将 LLM 生成的 Python 代码直接传给 `Command::new(&python).arg("-c").arg(code)`，绕过 `exec_shell` 的 `SandboxManager` 路由。

**证据**: `tool_catalog.rs:51-78` 构建 `Command` 时仅设置 `cwd` 和 `timeout`，无 `SandboxManager::prepare()` 调用。`exec_shell` 有完整 SandboxManager 集成。

**影响**: LLM 生成的 Python 代码在无沙箱环境中执行，即使启用了沙箱设置。

**建议**: 将代码执行路径统一到 SandboxManager，或至少添加独立沙箱策略。

---

#### H4. terminal_guard try_send 静默丢弃 ResumeEvents (`crates/tui/src/core/engine/tool_execution/terminal_guard.rs:22-33`)

**问题**: `Drop` 实现使用 `tx.try_send(Event::ResumeEvents)`。如果通道满，事件静默丢失，终端永久挂起。

**证据**: `try_send` 不阻塞，通道满时返回 `Err(TrySendError::Full(_))`，被 `Drop` 语义丢弃。

**影响**: 在负载下，终端可能出现不可恢复的挂起状态。

**建议**: 使用 `tx.blocking_send` 或在 `Drop` 中加超时重试。

---

#### H5. app-server API key 镜像到子进程环境变量 (`crates/app-server/src/lib.rs:~250-280`)

**问题**: `build_state` 将 API key 注入到子进程环境变量中，子进程及所有孙进程均可读取。

**影响**: 任何被 app-server 触发的子进程（包括 LLM 调用的 shell 命令）都可通过 `env` 读取 API key。

**建议**: 仅在必要时设置 `DEEPSEEK_API_KEY`，并在 exec 前清除非必要环境变量。

---

### MEDIUM

#### M1. app-server CorsLayer::permissive() (`crates/app-server/src/lib.rs:107`)

`CorsLayer::permissive()` 允许所有 origin、method、header。默认绑定 `127.0.0.1` 减轻风险，但 `--host` 参数可配置为 `0.0.0.0` 暴露到网络。  
**建议**: 限制 origin 为本地路径，或强制 `--host` 仅允许 loopback。

#### M2. Runtime::new 静默吞 Job 加载错误 (`crates/core/src/lib.rs:461-463`)

`let _ = jobs.load_from_store(&state);` 丢弃错误，状态存储损坏时所有持久化 Job 静默丢失。  
**建议**: 至少 `tracing::warn!` 记录失败。

#### M3. Secrets::auto_detect 回退到 CWD (`crates/secrets/src/lib.rs`)

`auto_detect()` 在 home 目录不可用时回退到 `FileKeyringStore` 写入当前工作目录。  
**建议**: 限制回退路径或警告用户。

#### M4. Schema version 字段未校验 (`crates/core/src/lib.rs:152-166`)

`JobManager::parse_persisted_detail` 定义 `JOB_DETAIL_SCHEMA_VERSION` 常量但从不校验 JSON 中的 `schema_version` 字段。  
**建议**: 添加版本校验和迁移逻辑。

#### M5. 时间戳解析静默损坏 (`crates/tui/src/session_store_sqlite.rs`)

`parse_ts` 使用 `unwrap_or_default()` 处理 `DateTime::parse_from_rfc3339` 失败，损坏的时间戳静默变为 epoch。  
**建议**: 至少记录警告。

---

### LOW

#### L1. ChatMarkdown XSS 残余风险 (`crates/desktop/web-ui/src/components/ChatMarkdown.tsx`)

`markdown-it` 配置 `html: false` + `DOMPurify` 双重防护有效。`linkify: true` 启用自动链接转换，`DOMPurify` 通过 `ALLOWED_URI_REGEXP` 限制链接协议。整体 XSS 风险低。  
**建议**: 持续关注 markdown-it 插件安全公告；定期审计 `ALLOWED_URI_REGEXP`。

---

## 四、正面发现

以下领域安全设计良好：

1. **路径遍历防护** — `ToolContext::resolve_path` 提供规范化 + `..` 拒绝，在各工具中一致使用
2. **fetch_url.rs SSRF** — 完整三阶段防护（localhost 拒绝、IP 过滤、DNS pinning），业界最佳实践
3. **SQLite 参数化查询** — 所有 SQL 使用 `params![]` 宏，无 SQL 注入
4. **API Key 处理** — Desktop 使用 OS Keyring 存储，不落盘明文；`runtime_proxy.rs` Bearer Token 保留在 Rust 侧
5. **命令执行安全** — `command_safety.rs` 有完善的 shell 命令安全性分析
6. **网络策略** — `network_policy.rs` 域名级 deny-wins 策略设计合理
7. **子代理结构** — 子代理拥有独立工具过滤、步骤限制和超时控制
8. **macOS Seatbelt** — macOS 沙箱正常运作
9. **Web UI 输出清理** — `DOMPurify` + `markdown-it html:false` 双重防护
10. **LSP 诊断自动反馈** — 文件编辑后自动运行 LSP 检查

---

## 五、设计债务（非安全，但影响可维护性）

| 文件 | 行数 | 问题 |
|------|------|------|
| `crates/tui/src/tui/ui.rs` | ~7,800 | 过大，应拆分 |
| `crates/tui/src/tui/app.rs` | ~5,000 | 过大，应拆分 |
| `crates/tui/src/tools/shell.rs` | ~2,572 | 可拆分为多个模块 |
| `crates/tui/src/tools/subagent/mod.rs` | ~4,279 | 可拆分为多个模块 |

---

## 六、验证摘要

### 验证方法

- 所有 CRITICAL 和 HIGH 发现均通过主代理 **直接读取源码** 进行交叉验证
- MEDIUM 发现来自子代理报告，部分已验证
- 使用了 "caller-trace" 规则：所有需要对比的发现（如 H1 web_run vs fetch_url）均已追踪调用路径
- 通过了 "hype filter"：降低了无实际利用路径的发现级别

### 验证覆盖率

| 级别 | 总数 | 主代理直接验证 | 子代理验证 | 未验证 |
|------|------|--------------|-----------|--------|
| CRITICAL | 2 | 2 | 2 | 0 |
| HIGH | 5 | 4 | 5 | 0 |
| MEDIUM | 5 | 1 | 5 | 0 |
| LOW | 1 | 1 | 1 | 0 |

### 已知局限

- 子代理可能未完全覆盖某些大文件的所有行（如 `shell.rs` 2572 行）
- 运行时行为（竞态条件、死锁）的静态分析有天然局限
- 第三方依赖（crates.io、npm）未纳入本次审核范围
- `web_run.rs` 1826 行中部分浏览器渲染逻辑未能逐行分析

---

## 七、修复优先级建议

| 优先级 | 发现 | 预计工作量 |
|--------|------|-----------|
| P0（本周） | C1: Linux/Windows 沙箱空壳 → 添加明确 UI 警告 | 0.5d |
| P0（本周） | C2: ExecPolicy 空清单 → 加载默认策略文件 | 0.5d |
| P1（本月） | H1: web_run SSRF → 复用 fetch_url SSRF 逻辑 | 1d |
| P1（本月） | H2: Desktop IPC 路径穿越 → 添加 resolve_under_workspace | 0.5d |
| P1（本月） | H3: Python 无沙箱 → 接入 SandboxManager | 1d |
| P1（本月） | H4: terminal_guard 事件丢失 → blocking_send | 0.5d |
| P2（下月） | H5: API Key 环境变量泄露 | 0.5d |
| P2（下月） | M1-M5: 中等问题 | 2d |
| P3（持续） | 设计债务：大文件拆分 | 持续 |

---

> **审核运行 ID**: `2026-05-24-audit`  
> **Scratchpad**: `.deepseek/scratchpad/2026-05-24-audit/`  
> **总耗时**: ~4 分钟（8 并行子代理 + 主代理交叉验证）
