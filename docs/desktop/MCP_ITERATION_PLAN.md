# Zagens MCP 功能迭代方案

> 状态：草案（Draft） · 适用产品：Zagens 桌面（`crates/desktop`） · 最后更新：2026-06-04
>
> 本文针对 Zagens 作为 **MCP 客户端** 连接外部 MCP 服务器这条主链路（`crates/runtime-adapters/src/mcp/` → `crates/runtime-server` engine → `web-ui/McpPanel`），给出现状审核结论与分阶段迭代计划。不涉及未接入主链的独立服务端 crate `crates/mcp`（见 §6）。

---

## 1. 范围与目标

- **范围**：MCP 客户端的传输、连接生命周期、工具/资源/提示词的发现与调用、认证、配置管理、桌面 UI 与可观测性。
- **目标**：
  1. 消除影响 agent 正确性的缺陷；
  2. 补齐与较新 MCP 服务器互通所需的传输与认证能力；
  3. 改善连接生命周期管理与桌面侧使用体验；
  4. 提升可观测性与测试覆盖。

---

## 2. 现状概览

### 2.1 关键代码位置

| 层 | 路径 | 职责 |
|----|------|------|
| 协议/连接 | `crates/runtime-adapters/src/mcp/transport.rs` | `StdioTransport` / `SseTransport` / `McpTransport` trait |
| 协议/连接 | `crates/runtime-adapters/src/mcp/connection.rs` | `initialize` → 发现 → RPC 调用 |
| 连接池 | `crates/runtime-adapters/src/mcp/pool.rs` | 连接池、工具名前缀、合成工具、`to_api_tools` |
| 配置 | `crates/runtime-adapters/src/mcp/config.rs` / `config_io.rs` | `mcp.json` 解析、超时、工具开关、CRUD |
| 结果格式化 | `crates/runtime-adapters/src/mcp/format.rs` | `format_tool_result()`（**当前未被执行路径调用**） |
| Engine 集成 | `crates/runtime-server/src/core/engine/tool_context.rs` | `ensure_mcp_pool` / `mcp_tools` |
| 工具执行 | `crates/runtime-server/src/core/engine/tool_execution/mcp.rs` | `execute_mcp_tool_with_pool` |
| HTTP API | `crates/runtime-server/src/runtime_api/mcp.rs` | 服务器 CRUD、工具列表、合并配置 |
| UI | `crates/desktop/web-ui/src/components/McpPanel.tsx` | 服务器/工具管理面板 |
| 抽象 | `crates/core/src/engine/hosts/mcp.rs` | `McpHost` trait（并发/只读/审批策略） |

### 2.2 能力矩阵

| 能力 | 现状 |
|------|------|
| stdio 传输 | 支持（含 SIGTERM + grace 优雅关闭） |
| SSE 传输 | 支持（GET 流 + POST endpoint 回写） |
| Streamable HTTP 传输 | 支持（阶段 1：单 endpoint POST + SSE 响应解析 + `Mcp-Session-Id`；需配置 `"transport": "http"`） |
| 协议版本 | 通告 `2025-06-18`，解析服务器返回做协商/降级（阶段 1） |
| tools 发现/调用 | 支持，工具名 `mcp_{server}_{tool}` |
| resources / prompts | 协议支持，经合成工具暴露给模型；UI 不展示 |
| 远程认证 / OAuth | 静态头已支持（`headers` + `auth` bearer/apiKey，`${ENV}` 占位，API 脱敏）；OAuth 2.1 **未实现** |
| 网络策略门控 | 支持（HTTP/SSE，stdio 不受控） |
| 配置热重载 | 支持（阶段 3：`POST /v1/apps/mcp/reload` + 共享池；UI「应用配置」） |
| 自动重连 | 热重载时 `connect_all`；无独立后台健康探测循环 |
| 分层超时 | 支持 connect/execute/read 三级，可按服务器覆盖 |
| 测试 | 以 `runtime-adapters` 单元测试为主，无真实服务器 E2E |

---

## 3. 问题清单

### P0 — 正确性缺陷

#### 3.1 MCP 工具 `isError` 未被识别为失败
`tool_execution/mcp.rs` 将整个 `tools/call` 结果 `to_string_pretty` 当作成功返回：

```rust
let content = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
Ok(ToolResult::success(content))
```

按 MCP 规范，工具失败通过 `result.isError == true` 表达。当前实现会把失败也当成功，并把含 `content`/`isError`/`meta` 的原始 JSON 噪声塞给模型。已有的 `format.rs::format_tool_result()` 能正确处理 `isError` 与提取 text 块，**但从未被调用**。

- **影响**：模型误判工具结果、上下文噪声大。
- **修复**：执行路径改为提取 `content[]`，并在 `isError` 时返回 `ToolResult::error`。

#### 3.2 工具名解析对含下划线的服务器名错误
拼接为 `mcp_{server}_{tool}`，但 `pool.rs::parse_prefixed_name` 用 `split_once('_')`：

```rust
let rest = &prefixed_name[4..];
let Some((server, tool)) = rest.split_once('_') else { ... };
```

若服务器名含下划线（如 `github_mcp`），`mcp_github_mcp_search` 会被错误拆为 server=`github`、tool=`mcp_search`。

- **影响**：含下划线服务器名的工具调用全部失败。
- **修复**：基于已知 server 列表做最长前缀匹配，或采用可逆编码；补含下划线服务器名的单测。

### P1 — 能力缺口

#### 3.3 缺少 Streamable HTTP 传输
仅有老式 SSE 实现，缺 2025 规范的 Streamable HTTP（单 endpoint POST + 可选 SSE 升级 + `Mcp-Session-Id`）。越来越多远程服务器只提供该传输，当前无法连接。

#### 3.4 协议版本固定且不协商
`connection.rs` 硬编码 `"protocolVersion": "2024-11-05"`，未读取服务器返回版本做协商/降级。

#### 3.5 无远程认证 / OAuth
SSE 用裸 `reqwest::Client`，无法注入 `Authorization` 头，也无 OAuth 流程。私有远程 MCP 仅能依赖 URL 内嵌凭证或 stdio 环境变量，无法刷新 token。

#### 3.6 配置改动必须重启 sidecar
运行中的 Engine 池不重载 `mcp.json`，UI 只能提示重启 sidecar；无健康探测与后台自动重连。

### P2 — 体验与可观测性

- **3.7** UI 不展示 resources/prompts；`enabled_tools`/`disabled_tools` 为裸文本框，无逐工具开关。
- **3.8** 无 MCP 专用 trace span；无连接事件、调用耗时、错误的可视化。
- **3.9** 缺真实 MCP 服务器进程的 E2E 测试。
- **3.10** `crates/mcp`（`deepseek-mcp`）为悬挂模块：无依赖方、命名约定（`mcp__server__tool`）与主链（`mcp_server_tool`）不一致。

---

## 4. 分阶段迭代计划

### 阶段 0：缺陷修复（P0）— 已完成（2026-06-04）
- [x] 执行路径改用 `extract_tool_content()` + `is_tool_error()`，`isError` 映射为 `ToolResult::error`。
- [x] 修复 `parse_prefixed_name` 含下划线服务器名问题（最长前缀匹配，未知名回退）；与拼接逻辑对称。
- [x] 补单元测试：含下划线服务器名、`isError=true`、含非 text content 块。
- **范围**：纯后端（`runtime-adapters` + `runtime-server`）。
- **风险**：低。 **预估**：~0.5 天。

### 阶段 1：传输与协议现代化（P1）— 已完成（2026-06-04）
- [x] 新增 `StreamableHttpTransport`（POST JSON-RPC + `application/json`/`text/event-stream` 响应解析 + `Mcp-Session-Id` 维护 + `MCP-Protocol-Version` 头）。
- [x] `connection.rs` 传输选择按配置 `transport`（别名 `type`：`stdio`/`sse`/`http`）分派，url 无显式 type 时默认 SSE 保留兼容。
- [x] `initialize` 通告 `2025-06-18` 并解析服务器返回值做协商/降级（支持版本表 + 未知版本尽力兼容）。
- [x] 配置与类型同步（`config.rs` `transport_kind()`、`config_io` 快照标签、`runtime_api/mcp.rs` 回填 + `list_mcp_tools` 解析修复、`web-ui/types/mcp.ts` + `McpPanel`）。OpenAPI 暂未覆盖 MCP 端点，无需同步。
- **未做（留待后续）**：远程服务器 type 省略时未做 HTTP↔SSE 自动探测（需显式 `"transport": "http"`）；未开启 Streamable HTTP 的独立 GET 服务端推送流。
- **风险**：中。 **预估**：~2–3 天（实际后端 + 类型同步在本次完成，待真实服务器冒烟）。

### 阶段 2：远程认证（P1）— 静态头已完成（2026-06-04）
- [x] 服务器配置新增 `headers` / `auth`（`bearer` / `apiKey`），连接时注入 SSE/Streamable HTTP 的 reqwest 默认头；支持 `${ENV_VAR}` 占位，避免在 `mcp.json` 写死明文。
- [x] API/UI 脱敏：`redacted_for_display` + `merge_preserved_secrets` 防止编辑回写清空密钥。
- [x] `McpPanel` 编辑表单支持 headers 与 auth 类型/token。
- [ ] （可选、较重）MCP OAuth 2.1 授权码 + PKCE + token 刷新 + 桌面浏览器回调。
- [ ] （可选）OS keyring 引用（如 `secret:mcp/<server>`），与 `deepseek-secrets` 深度集成。
- **风险**：静态头低 / OAuth 中高。 **预估**：OAuth ~3–5 天（未启动）。

### 阶段 3：生命周期与热重载（P1/P2）— 热重载已完成（2026-06-04）
- [x] 进程级共享 `McpPool` + `POST /v1/apps/mcp/reload`（`reload_config` diff → 断开移除/变更/禁用项 → 可选 `connect_all`）。
- [x] Engine `ensure_mcp_pool` 复用共享池；`McpPanel` 增删改/合并后自动热重载 +「应用配置」按钮，不再弹出「重启 sidecar」对话框。
- [ ] （可选）后台健康探测 + 指数退避重连；连接状态通过 SSE/事件推送 UI。
- **风险**：中。 **预估**：~2 天（核心热重载已落地；后台探测未做）。

### 阶段 4：UI 与可观测性（P2）— 已完成（2026-06-04）
- [x] `McpPanel` 增加 resources/prompts 标签页、逐工具开关、连接状态 20s 轮询刷新、最近调用/错误日志（`McpServerDetail` + `GET /v1/apps/mcp/discover`）。
- [x] 后端 `mcp` 调用记录 + `tracing::info_span!`（server/method/耗时/结果大小），错误体经 `redact_body_preview` 脱敏后在 UI 展示。
- [x] `crates/mcp` 标注弃用（`README.md`）；生产客户端路径为 `runtime-adapters/src/mcp/`。
- [ ] （可选）真实 MCP 服务器进程 E2E 测试（方案 §3.9，未启动）。
- **风险**：中。 **预估**：~3 天（核心 UI/可观测性已落地）。

---

## 5. 优先级与排序建议

1. **先做阶段 0**：P0 直接影响 agent 正确性，改动小、可独立合入并补测试。
2. 视远程 MCP 需求，在 **阶段 1（Streamable HTTP）** 与 **阶段 2（认证）** 之间排序——两者通常需要配合（远程私有服务器既要新传输又要认证）。
3. **阶段 3 / 4** 作为体验与可维护性提升，可与上面并行或随后推进。

---

## 6. 附注：`crates/mcp` 与主链的关系

- `crates/runtime-adapters/src/mcp/`：Zagens **作为 MCP 客户端**连接外部服务器，桌面实际使用路径。
- `crates/mcp`（`deepseek-mcp`）：内置 **MCP 服务端** stdio 循环与 `McpManager`，**无任何 crate 依赖**，且工具命名（`mcp__server__tool`）与客户端主链（`mcp_server_tool`）不一致，属遗留/实验模块。阶段 4 需对其定位做决策，避免误导。

---

## 7. 验收与测试

- 阶段 0：新增单元测试覆盖 `isError` 与含下划线服务器名；现有 `runtime-adapters` 测试全绿。
- 阶段 1/2：对接至少一个真实 Streamable HTTP / 需认证的 MCP 服务器做手动冒烟，并记录到本文档。
- 全程：每个阶段在同一 PR 内更新 `CHANGELOG.md`（`[Unreleased]`），遵循仓库变更记录规范。
