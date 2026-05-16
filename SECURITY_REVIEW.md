# Security Review: DeepSeek TUI Desktop（DS Pick）

**日期**：2026-05-17  
**范围**：全仓库代码级安全审查 — 15 个 Rust crate + desktop web-ui  
**审查方式**：直接源码阅读 + 子代理并行审查（5 个，4 个因 API 超时未完成）  
**排除**：`target/`、`.git/`、`vendor/`、`docs/`、`assets/`

---

## 架构概述

代码库是一个 Rust workspace，结构如下：

| 层级 | 组件 | 说明 |
|------|------|------|
| 入口 | `crates/tui` | 单体核心：CLI/TUI 入口、Agent 引擎、工具实现、沙箱、MCP、Runtime HTTP/SSE API。约 130+ 源文件 |
| 桌面 | `crates/desktop` | Tauri 壳（DS Pick），以 sidecar 方式启动 `deepseek serve --http`。前端为 React/TypeScript |
| 配置 | `crates/config` | TOML 配置解析、多 Provider 路由、Secrets 集成 |
| 密钥 | `crates/secrets` | OS 密钥链抽象（macOS Keychain、Windows Credential Manager、Linux libsecret） |
| 支撑 | `agent` `app-server` `cli` `core` `execpolicy` `hooks` `mcp` `protocol` `state` `tools` `tui-core` | 薄封装库层 |

整体安全态势**较为成熟**——路径校验、`fetch_url` 的 SSRF 防护、Runtime API 的 Bearer Token 认证、密钥脱敏、沙箱机制均已实现且质量良好。发现的缺口集中在 `web_run` 工具和 CSP 配置。

---

## 发现清单

### 🔴 HIGH

#### H1：`web_run` 工具缺少 SSRF 防护（与 `fetch_url` 对比明显）

- **文件**：`crates/tui/src/tools/web_run.rs`
- **行号**：`fetch_page` 约 1067 行，`resolve_or_fetch_page` 约 692 行
- **描述**：`web_run` 的 "open" 命令通过 `fetch_page()` 获取任意 URL，该函数使用裸 `reqwest::Client`，无任何 IP 校验。

  对比 `fetch_url`（`crates/tui/src/tools/fetch_url.rs:169-205`）拥有全面的 SSRF 防护：
  - 拒绝 localhost
  - 对字面 IP 进行 restricted range 检查
  - DNS 解析后逐一检查所有解析出的 IP
  - 将验证通过的 IP 锁定到客户端（防止 DNS rebinding）
  - 覆盖 IPv4-mapped IPv6 绕过

  而 `web_run` 仅有基于域名字符串的 `check_network_policy` 检查（第 1048 行），若未显式配置网络策略，LLM 可以指示工具成功访问 `http://169.254.169.254/latest/meta-data/` 或 `http://localhost:8080/`。

- **建议**：将 `fetch_url` 中的 `is_restricted_ip()` 逻辑复用到 `resolve_or_fetch_page` 或 `fetch_page` 中。至少应拒绝 localhost、RFC 1918、link-local 和云元数据 IP。建议同时加入 DNS rebinding 防护（IP 锁定）。

---

### 🟡 MEDIUM

#### M1：Tauri CSP `connect-src` 允许所有 localhost 端口

- **文件**：`crates/desktop/tauri.conf.json`，第 30 行
- **描述**：CSP 指令 `connect-src 'self' ipc: http://ipc.localhost http://ipc.localhost:* http://127.0.0.1:* http://localhost:* ...` 允许 WebView 向本机任意端口发起 HTTP/WebSocket 请求。虽然 Runtime API（端口 7878）需要此权限，但也同时放行了用户机器上运行的所有其他本地服务。
- **建议**：将 `:*` 收窄为已知端口，如 `http://127.0.0.1:7878 http://localhost:7878`。如需额外端口，通过 Tauri 的 CSP builder 在运行时动态添加。

#### M2：Tauri CSP `img-src blob:` — SVG XSS 向量

- **文件**：`crates/desktop/tauri.conf.json`，第 30 行
- **描述**：`img-src 'self' data: blob:` 允许 blob: URL 用作图片来源。如果用户提供的 SVG 内容通过 blob: URL 渲染，包含 `<script>` 标签的恶意 SVG 可能在 WebView 上下文中执行。结合不受限的 `connect-src`，可实现数据外泄。
- **建议**：添加 `frame-src 'none'`，并考虑将 `img-src` 限制为 `'self' data:`，或对 blob SVG 内容进行清洗。

#### M3：Runtime Auth Token 可从 WebView JavaScript 访问

- **文件**：`crates/desktop/src/main.rs`（Token 生成）、`crates/desktop/src/commands.rs`（`get_runtime_token`，第 66 行）
- **描述**：桌面应用为 Runtime API 生成一个随机 UUID Bearer Token，并通过 `get_runtime_token` Tauri 命令将其暴露给 WebView。若 WebView 发生 XSS，攻击者可提取此 Token 并对 Runtime API 进行认证调用。
- **缓解**：Runtime API 默认绑定 `127.0.0.1`，限制了外部访问。威胁模型已假定 WebView 上下文受信任。
- **建议**：考虑按会话轮换 Token，并在 Runtime API 侧添加按来源的访问限制。

#### M4：`web_search` HTML 抓取脆弱性及 UA 冒充

- **文件**：`crates/tui/src/tools/web_search.rs`
- **描述**：该工具使用正则表达式（`result__a`、`result__snippet`）抓取 DuckDuckGo HTML 搜索结果。DuckDuckGo 任何布局变更都会导致搜索失败。失败后回退到 Bing 同样使用正则抓取。User-Agent 字符串冒充 Safari（`Version/17.0 Safari/605.1.15`）。
- **建议**：考虑使用 DuckDuckGo Lite/API 端点或正规搜索 API。将 Safari UA 替换为诚实的 Bot UA，标明工具身份。

#### M5：Tauri CSP 缺少 `frame-src` / `frame-ancestors`

- **文件**：`crates/desktop/tauri.conf.json`，第 30 行
- **描述**：CSP 缺少 `frame-src` 和 `frame-ancestors` 指令。如有组件渲染 iframe（如 Mermaid 图表、HTML 预览），将不受任何限制。配合 `connect-src: *`（针对 localhost），恶意 iframe 可访问本地服务。
- **建议**：添加 `frame-src 'none'`（如需 iframe 则使用 `frame-src 'self'`）和 `frame-ancestors 'none'`。

---

### 🟢 LOW

#### L1：`read_file` 100MB 限制可能导致资源耗尽

- **文件**：`crates/tui/src/tools/file.rs`，`MAX_FILE_SIZE = 100 * 1024 * 1024`
- **描述**：模型可请求读取 100MB 文件，消耗 Agent 进程大量内存。多个并发的 `read_file` 调用可能导致 OOM。
- **建议**：考虑降低纯文本读取上限（如 10MB），或添加按会话的总字节读取预算。

#### L2：PDF/DOCX/XLSX/PPTX 解析依赖外部工具或正则

- **文件**：`crates/tui/src/tools/file.rs`（PDF 通过 `pdftotext`、DOCX/XLSX 通过 ZIP + 正则）
- **描述**：PDF 提取通过 shell 调用 `pdftotext` 或 `pdf-extract`。畸形 PDF 可能利用这些工具的解析漏洞。DOCX/XLSX 解析使用 XML 正则匹配——脆弱且可能受到 billion-laughs 类 XML 攻击。
- **建议**：对 PDF 提取进程进行沙箱化。对 OOXML 文件使用正规 XML 解析器并设置实体展开限制。

#### L3：Shell 命令使用 `sh -c` — 元字符注入面

- **文件**：`crates/tui/src/sandbox/mod.rs`，`CommandSpec::shell()`
- **描述**：命令通过 `sh -c "<command>"`（Windows 下为 `cmd /C`）执行。若工具输入来源于不可信源（如 MCP 服务端输出、文件内容），Shell 元字符可能被注入。
- **缓解**：当前命令文本来自 LLM（在此威胁模型下可信任）。execpolicy 系统提供了额外的校验层。
- **建议**：确保 Agent 模式下默认启用 execpolicy 校验。

#### L4：`web_search`/`web_run` 的 Safari UA 冒充

- **文件**：`crates/tui/src/tools/web_search.rs` 和 `crates/tui/src/tools/web_run.rs`
- **描述**：两个工具均使用 `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15` 作为 User-Agent。这不诚实，可能被 Web 服务器视为恶意行为。
- **建议**：使用诚实、可识别的 UA，如 `deepseek-tui/0.8 (https://github.com/Hmbown/DeepSeek-TUI)`（`fetch_url` 已采用此模式）。

---

## 显著优点

1. **路径逃逸防护**（`crates/tui/src/tools/spec.rs:312-416`）  
   `resolve_path` 使用 `canonicalize()` + `normalize_path()` + 多层回退。处理符号链接、不存在的路径（向上查找最深已存在祖先目录），并支持用户信任的外部路径。

2. **`fetch_url` SSRF 防护**（`crates/tui/src/tools/fetch_url.rs:169-205`）  
   全面——拒绝 localhost、解析字面 IP 检查受限范围、DNS 解析后逐一检查所有 IP、将验证通过的 IP 锁定以防止 DNS rebinding。覆盖 IPv4-mapped IPv6 绕过。配有详尽测试（428-478 行）。

3. **密钥优先级链**（`crates/secrets/src/lib.rs`）  
   清晰链路：OS 密钥链 → 环境变量 → 配置文件。桌面应用先写入密钥链，再清除配置文件中的明文。

4. **URL/Token 脱敏**（`crates/tui/src/mcp.rs:54-72`）  
   `mask_url_secrets` 和 `redact_body_preview` 在错误消息中脱敏凭证信息。

5. **Runtime API 认证**（`crates/tui/src/runtime_api.rs:689-718`）  
   对 `/v1/*` 路由使用 Bearer Token 中间件。同时支持 `x-deepseek-runtime-token` 标头。拒绝 URL 查询参数中的 Token 以防止日志泄露。使用 SHA-256 指纹用于日志记录而不暴露 Token。

6. **父进程死亡信号**（`crates/tui/src/tools/shell.rs:195-215`）  
   在 Linux 上，`PR_SET_PDEATHSIG(SIGTERM)` 确保 TUI 异常终止时子进程被回收。

7. **进程组清理**（`crates/tui/src/tools/shell.rs:218-230`）  
   `kill_child_process_group` 在 Unix 上向整个进程组发送 SIGKILL。

8. **网络策略系统**（`crates/network_policy`）  
   按域名的 allow/deny/prompt 决策引擎，可按会话配置。

9. **沙箱策略系统**（`crates/tui/src/sandbox/policy.rs`）  
   四级策略（DangerFullAccess、ReadOnly、ExternalSandbox、WorkspaceWrite），各模式有对应默认值（Agent = WorkspaceWrite+network，YOLO = DangerFullAccess）。

10. **交互式终端守卫**（`crates/tui/src/core/engine/tool_execution.rs:28-60`）  
    RAII 守卫在交互式工具执行期间暂停/恢复终端状态。使用 `try_send` 在 `Drop` 中处理取消而不死锁。

---

## 验证摘要

| 验证项 | 方法 | 结果 |
|--------|------|------|
| H1 `web_run` SSRF 缺口 | 重新读取 `fetch_page()`（1067 行）和 `check_network_policy()`（1048 行） | ✅ 确认 |
| M1-M5 CSP 配置 | 直接读取 `tauri.conf.json` | ✅ 确认 |
| 路径解析 | 完整读取 `resolve_path()`（312-416 行）和 `normalize_path()`（516-565 行） | ✅ 安全 |
| `fetch_url` SSRF | 完整读取 169-205 行，含 DNS pinning | ✅ 全面 |
| Runtime API 认证 | 完整读取 `require_runtime_token` 中间件（689-718 行） | ✅ 安全 |

**已知缺口**：
- 5 个子代理中 4 个因 API 超时（120s）未完成（覆盖 web-ui 组件层和部分小 crate）
- Desktop web-ui `.tsx` 组件文件大部分未经审查
- `crates/tui/src/tui/app.rs`（大型文件）仅部分阅读
- 测试文件基本未覆盖

---

## 总体评估

代码库在关键安全路径上展现出扎实的安全工程能力——路径校验、密钥处理、`fetch_url` SSRF、沙箱和 API 认证均实现良好。主要可操作发现是 `web_run` 的 SSRF 缺口，应通过复用 `fetch_url` 的 IP 校验逻辑来解决。CSP 加固建议是对桌面应用的纵深防御性改进。
