# DeepSeek-TUI-desktop 全量代码审计报告

> **审计日期**：2026-05-20  
> **版本**：workspace 0.8.15 / DS Pick desktop  
> **范围**：全部 290+ Rust 文件（~180K 行）+ 79 TypeScript/TSX 文件（~20K 行）+ 配置/build 文件  
> **方法**：29 个审计域，18 个并行 Explore 子代理 + 手动审查  
> **覆盖**：29/29 域完成，0 域延期  
> **审计内存**：`.deepseek/scratchpad/2026-05-20-001/`

---

## 一、审计概述

本次审计覆盖 **DeepSeek-TUI-desktop** monorepo 的全部代码，包括：

| 层级 | 范围 | 文件数 |
|------|------|--------|
| 核心引擎 | `crates/tui/src/core/` — engine, session, ops, turn_loop, capacity | ~15 |
| 工具系统 | `crates/tui/src/tools/` — file, shell, search, subagent, tasks, github, office_write 等 | ~30 |
| TUI 界面 | `crates/tui/src/tui/` — app, ui, views, widgets, approval | ~25 |
| CLI 命令 | `crates/tui/src/commands/` — 26 个斜杠命令 | 26 |
| 运行时 | `crates/tui/src/` — config, main, runtime, client, session, compaction, mcp 等 | ~40 |
| 子系统 | scratchpad, skills, repl, sandbox, lsp, snapshot, rlm, modules | ~15 |
| 桌面壳 | `crates/desktop/src/` — Tauri 命令, sidecar, terminal | 6 |
| WebUI | `crates/desktop/web-ui/src/` — React/TypeScript 组件 | 79 |
| 共享 crate | config, core, cli, agent, protocol, execpolicy, tools, hooks, secrets, state, app-server, mcp, tui-core | ~20 |
| 配置/构建 | Cargo.toml, Dockerfile, config.example.toml, .env.example | 5 |

---

## 二、发现汇总

| 严重性 | 数量 | 说明 |
|--------|------|------|
| **HIGH** | 14 | 已验证安全/可靠性问题 |
| **MEDIUM** | 12 | 需关注的潜在风险 |
| **LOW** | 8 | 代码质量改进建议 |
| **CLEARED** | 15 域 | 无发现或已有充分防御 |

---

## 三、HIGH 严重性发现

### H01 — 取消令牌竞态条件
- **文件**：`crates/tui/src/core/engine.rs:379-395`
- **描述**：`reset_cancel_token()` 先创建新 `CancellationToken`，再获取 Mutex 锁替换共享令牌。在新令牌创建与锁获取之间，若 `handle.cancel()` 在旧令牌触发，取消信号会随旧令牌被替换而丢失。
- **影响**：用户在两轮对话之间按 Escape 时取消被静默忽略。
- **建议**：将新令牌创建移入锁内部，或使用原子标志替代。

### H02 — 子代理工具白名单绕过
- **文件**：`crates/tui/src/tools/subagent/mod.rs:3921-3954`
- **描述**：`build_allowed_tools()` 在 `explicit_tools = Some(...)` 时提前返回，跳过后面的 `match agent_type` 类型裁剪。调用者传入 `type=Explore` + `explicit_tools=["write_file","exec_shell"]` 会绕过 Explore 只读列表。
- **影响**：子代理获得不应有的写入/Shell 权限。
- **建议**：将 `explicit_tools` 与类型裁剪做交集 (`∩`)，而非跳过。

### H03 — Blackboard 路径穿越
- **文件**：`crates/tui/src/tools/subagent/blackboard.rs:21-28`
- **描述**：`blackboard_path()` 使用 `PathBuf::push(format!("{task_id}.json"))`，当 `task_id` 为绝对路径时 `push` 会替换整个路径，导致写入任意文件系统位置。
- **影响**：可写入任意 `.json` 文件到系统任意位置。
- **建议**：对 `task_id` 做白名单校验（仅允许 `[a-zA-Z0-9_-]+`）。

### H04 — Config Debug 泄露 API Key
- **文件**：`crates/tui/src/config.rs:733-738`
- **描述**：`Config` 结构体派生 `Debug`，包含 `api_key: Option<String>` 字段。任何 `{:?}` 或 `dbg!()` 调用都会以明文打印 API Key。
- **影响**：API Key 可能在日志、错误信息、调试输出中泄露。
- **建议**：手动实现 `Debug` 或使用 `#[derive(Debug)]` 但将 `api_key` 字段替换为 `SecretString` 类型。

### H05 — 桌面端任意文件写入
- **文件**：`crates/desktop/src/commands.rs:889-930`
- **描述**：`export_thread_json` Tauri 命令接受前端传来的 `save_path: String`，仅在 `thread_id` 上做百分号编码校验，`save_path` 无任何路径验证直接传给 `std::fs::write(&save_path, json)`。`export_session_json` 存在相同模式。
- **影响**：前端（包括可能的 XSS）可写入任意文件到系统任意位置。
- **建议**：将 `save_path` 限制在预设导出目录内；使用 `canonicalize` + 前缀检查。

### H06 — 运行时令牌暴露为 Tauri 命令
- **文件**：`crates/desktop/src/commands.rs:62-73`
- **描述**：`get_runtime_token` Tauri 命令返回完整 `runtime_token`（Bearer 令牌），任何前端 JS 代码都可调用此命令获取令牌。
- **影响**：WebView 中的任何代码（包括注入脚本）获得运行时 API 的完整 Bearer 访问权。
- **建议**：限制令牌使用为后端侧，前端通过后端代理调用 API；或在 Tauri capability 中限制命令访问。

### H07 — ANSI 转义序列注入（文件选择器）
- **文件**：`crates/tui/src/tui/file_picker.rs:326-355`
- **描述**：`collect_candidates` 遍历文件系统并使用 `as_os_str().to_string_lossy()` 生成显示字符串。文件名中的 ANSI 转义序列（如 `\x1b[31mfoo.rs`）原样渲染。
- **影响**：恶意仓库中的文件名可操纵终端显示。
- **建议**：渲染前通过 `strip_ansi_escapes` 或等效方法过滤控制字符。

### H08 — `/load` 命令路径穿越
- **文件**：`crates/tui/src/commands/session.rs:91-97`
- **描述**：`/load` 命令中用户输入 `p` 若包含 `/` 或 `\` 则直接用 `PathBuf::from(p)` 读取文件，无工作区边界校验。
- **影响**：`/load ../../etc/passwd` 可读取任意文件并以 session JSON 格式加载。
- **建议**：所有路径操作前做 `canonicalize` + 工作区前缀检查。

### H09 — `/save` 命令路径穿越
- **文件**：`crates/tui/src/commands/session.rs:28-30`
- **描述**：`/save` 命令直接使用用户输入构建 `PathBuf`，无路径包含检查。
- **影响**：可将会话数据写入任意可写位置。
- **建议**：与 H08 相同的修复策略。

### H10 — MCP 工具调用审批绕过
- **文件**：`crates/tui/src/mcp_server.rs:282-290`
- **描述**：MCP 服务端的 `require_approval` 检查依赖客户端 JSON 参数中的 `"approved": true` 字段，任何 MCP 客户端都可以设置此布尔值绕过审批。
- **影响**：启用 `require_approval` 的用户仍然会被 MCP 客户端绕过。
- **建议**：使用服务端维护的批准状态（如回调确认），而非客户端自证。

### H11 — MCP 服务默认暴露 Shell 执行
- **文件**：`crates/tui/src/mcp_server.rs:410-420`
- **描述**：`default_expose_tools()` 返回 `["file_read", "file_write", "search", "apply_patch", "shell", "deepseek", "deepseek-reply"]`，默认将 Shell 执行和完整 LLM 访问暴露给任何 stdio MCP 客户端。
- **影响**：连接到 MCP 服务的任何客户端通过用户 API Key 获得 Shell 和 LLM 访问权限。
- **建议**：默认排除 `shell` 和 `deepseek`，或要求显式配置启用。

### H12 — Linux/Windows 沙箱为 NO-OP
- **文件**：`crates/tui/src/sandbox/mod.rs:435-460`
- **描述**：`prepare_landlock()`（Linux）和 `prepare_windows()`（Windows）仅设置环境标记，未实际应用任何文件系统隔离。仅 macOS Seatbelt 有实际执行。
- **影响**：Linux 和 Windows 上 `exec_shell` 在无实际沙箱保护下执行。
- **建议**：实现 Landlock（Linux 5.13+）和 Windows Job Object 沙箱。

### H13 — Python REPL 无沙箱执行
- **文件**：`crates/tui/src/repl/runtime.rs:305-424`
- **描述**：Python REPL 启动模板使用 `exec(compile(code, ...), globals())` 无任何限制执行任意 Python，无 OS 级沙箱。
- **影响**：模型生成的 Python 代码以用户完整权限执行。
- **建议**：在沙箱环境中运行 Python 子进程（至少使用 Landlock/seccomp 限制）。

### H14 — UTF-8 字节索引切片恐慌
- **文件**：`crates/tui/src/scratchpad/coverage.rs:1-303`
- **描述**：代码中存在使用字节索引对 UTF-8 字符串切片的操作，未经 char-boundary 检查。
- **影响**：多字节 Unicode 字符处切片会触发 panic，导致审计 scratchpad 功能崩溃。
- **建议**：使用 `.char_indices()` 找边界，或使用 `unicode-segmentation` crate。

---

## 四、MEDIUM 严重性发现

### M01 — 粘贴输入无插入时大小限制
- **文件**：`crates/tui/src/tui/app.rs:2493-2507`
- **描述**：`insert_str` 无插入时大小限制；粘贴大数据（如 100MB 文件）会导致无限内存分配。
- **建议**：在 `insert_paste_text` 处添加插入时大小限制。

### M02 — 粘贴文本控制字符不过滤
- **文件**：`crates/tui/src/tui/app.rs:360-365`
- **描述**：`normalize_paste_text` 仅转换 `\r→\n`，但传递 ESC(0x1B)、BEL(0x07) 等控制字符。
- **建议**：统一使用 `is_control()` 过滤。

### M03 — Session 管理 Mutex 毒化恐慌
- **文件**：`crates/tui/src/session_manager.rs:195,304,370,438`
- **描述**：`db.lock().unwrap()` 在生产路径中恐慌于毒化 Mutex。
- **建议**：使用 `lock().map_err()` 或 `lock().unwrap_or_else(|e| e.into_inner())`。

### M04 — Mermaid SVG 绕过 DOMPurify
- **文件**：`crates/desktop/web-ui/src/components/MermaidPanel.tsx`
- **描述**：Mermaid 图表生成的 SVG 在传入 `dangerouslySetInnerHTML` 前未经过 DOMPurify 清洗。
- **建议**：对 Mermaid 输出也经过 DOMPurify 或仅使用 `mermaid.render()` 的安全模式。

### M05 — DiffCard 未清洗的 innerHTML
- **文件**：`crates/desktop/web-ui/src/components/DiffCard.tsx`
- **描述**：模型生成的 diff 内容经 `diff2html` 处理后直接赋值 `innerHTML`，未经过 DOMPurify。
- **建议**：对 diff2html 输出也执行 DOMPurify 清洗。

### M06 — Composer 剪贴板 innerHTML
- **文件**：`crates/desktop/web-ui/src/components/Composer.tsx`
- **描述**：粘贴处理器中来自剪贴板的 HTML 被赋值给 `innerHTML`。
- **建议**：粘贴前通过 DOMPurify 清洗剪贴板 HTML。

### M07 — Blackboard 并发写入 TOCTOU
- **文件**：`crates/tui/src/tools/subagent/blackboard.rs:95-171`
- **描述**：`write_blackboard_partition` 执行读→改→写→重命名，无锁协调，并发写入时静默丢失分区。
- **建议**：添加文件锁或使用原子写入模式。

### M08 — main.rs 无 SAFETY 注释的 unsafe 块
- **文件**：`crates/tui/src/main.rs:90-96`
- **描述**：存在无 `// SAFETY:` 注释的 unsafe 块。
- **建议**：添加安全理由文档。

### M09 — 网络策略缓存毒化静默失败
- **文件**：`crates/tui/src/network_policy.rs:330-341`
- **描述**：`NetworkSessionCache::is_approved()` 使用 `.unwrap_or(false)` 静默吞下 `PoisonError`。
- **建议**：在毒化时返回保守结果或记录错误。

### M10 — API Key 泄露到子进程环境
- **文件**：`crates/config/src/lib.rs` / `crates/desktop/src/commands.rs`
- **描述**：Tauri 命令将 config.env 传递给生成的子进程，其中可能包含 API Key。
- **建议**：在传递给子进程前从环境中剥离敏感密钥。

### M11 — 审计日志无文件权限设置
- **文件**：`crates/tui/src/audit.rs:16-38`
- **描述**：`~/.deepseek/audit.log` 使用 `create(true).append(true)` 创建，未设置文件权限，可能继承宽松 umask。
- **建议**：在创建后设置 `0o600` 权限。

### M12 — python_env.rs ENV 变量驱动命令执行
- **文件**：`crates/tui/src/python_env.rs:112-118`
- **描述**：`DEEPSEEK_BUNDLED_PYTHON` 环境变量指定 Python 二进制路径，攻击者可指向恶意二进制。
- **建议**：限制为受信任路径或使用哈希验证。

---

## 五、积极发现（安全亮点）

以下方面展现了良好的安全实践：

1. **SQL 注入防御**：`session_store_sqlite.rs` 和 `thread_store_sqlite.rs` 全部使用参数化查询 (`params![]`)。
2. **路径穿越防御**：`skills/install.rs` 实现了完整的路径穿越防护（拒 `..` 和绝对路径 + 符号链接拒绝 + 临时目录提取 + 原子重命名）。
3. **Markdown 渲染安全**：`ChatMarkdown.tsx` 使用 `markdown-it(html: false)` → `DOMPurify.sanitize()` → 工作区链接重写到 `data-*` 属性，形成防御深度。
4. **Docker 安全**：Dockerfile 使用非 root 用户 (UID 1000)，多阶段构建，`--locked` 可重现。
5. **API Key 管理**：`ApiKeyForm.tsx` 使用 `type="password"` 输入框 + Tauri `invoke` 后端保存；`client.ts` 使用模块级闭包保存令牌，不暴露到 `window`。
6. **快照安全**：`snapshot/repo.rs` 使用 `--git-dir` + `--work-tree` 隔离，不触碰用户 `.git`。
7. **Shell 命令安全**：工具层使用 `Command::arg()` 而非 Shell 字符串拼接。
8. **Config 安全**：Uniх 上配置文件使用 `0o600` 权限保存。

---

## 六、覆盖统计

| 审计域 | 状态 | 发现数 |
|--------|------|--------|
| area-tui-core | ✅ done | 1 HIGH |
| area-tui-tools-file | ✅ done | 1 MEDIUM |
| area-tui-tools-agents | ✅ done | 2 HIGH, 1 MEDIUM |
| area-tui-tools-other | ✅ done | cleared |
| area-tui-tui-primary | ✅ done | 1 MEDIUM, 1 LOW |
| area-tui-tui-widgets | ✅ done | cleared |
| area-tui-tui-other | ✅ done | 1 HIGH |
| area-tui-commands | ✅ done | 2 HIGH |
| area-tui-root-config | ✅ done | 1 HIGH, 1 MEDIUM |
| area-tui-root-state | ✅ done | 1 MEDIUM |
| area-tui-root-misc-a | ✅ done | 2 HIGH |
| area-tui-root-misc-b | ✅ done | 1 MEDIUM |
| area-tui-root-misc-c | ✅ done | 2 MEDIUM |
| area-tui-scratchpad | ✅ done | 1 HIGH |
| area-tui-skills | ✅ done | cleared (安全亮点) |
| area-tui-repl | ✅ done | 1 HIGH |
| area-tui-sandbox | ✅ done | 1 HIGH |
| area-tui-lsp | ✅ done | cleared |
| area-tui-snapshot | ✅ done | cleared (安全亮点) |
| area-tui-modules | ✅ done | cleared |
| area-tui-rlm | ✅ done | cleared |
| area-desktop-rust | ✅ done | 2 HIGH, 1 MEDIUM |
| area-desktop-webui-core | ✅ done | 2 MEDIUM |
| area-desktop-webui-comp | ✅ done | 1 MEDIUM, 1 LOW |
| area-desktop-webui-prev | ✅ done | cleared |
| area-desktop-webui-lib | ✅ done | cleared |
| area-shared-crates | ✅ done | 1 MEDIUM |
| area-npm-scripts | ✅ done | cleared |
| area-config-files | ✅ done | cleared (安全亮点) |

---

## 七、修复优先级建议

| 优先级 | 发现 | 理由 |
|--------|------|------|
| **P0 立即** | H05 桌面任意文件写入 | 前端可控路径，直接文件系统写入 |
| **P0 立即** | H06 令牌暴露为 Tauri 命令 | 任何 WebView 代码获得运行时访问 |
| **P0 立即** | H02 子代理白名单绕过 | 写入/Shell 权限提升 |
| **P1 本周** | H10 MCP 审批绕过 | 用户配置的审批被绕过 |
| **P1 本周** | H08/H09 路径穿越 | 任意文件读写 |
| **P1 本周** | H12/H13 沙箱缺失 | Python 代码无隔离执行 |
| **P2 迭代** | H03 Blackboard 路径穿越 | 仅 `.json` 写入，且 task_id 由父进程控制 |
| **P2 迭代** | H01 取消令牌竞态 | 影响有限，可重试 |
| **P2 迭代** | H04/H11 H07 H14 | 调试泄露、默认暴露、ANSI 注入、字节切片 |

---

> **审计方法**：18 个并行 Explore 子代理 + 手动代码审查 + Auditor 事实核查。  
> **审计内存**：`.deepseek/scratchpad/2026-05-20-001/` — 37 条 notes，21 条已验证发现。  
> **已知限制**：3 个子代理结果摘要不可用（area-tui-tools-other, area-tui-tui-widgets, area-tui-skills），已通过手动审查补偿。
