# TUI vs DS Pick：功能对标差距

本文档将 **DeepSeek TUI / `deepseek` CLI** 的能力（成熟、长期演进的产品）与 **DS Pick**（基于 Tauri 的桌面壳 + 运行在 RuntimeApi 上的 Web UI）进行对照。这是一份**规划性的盘点清单**，并非承诺实现每一项。

**参考：** [RUNTIME_API.md](../RUNTIME_API.md)、[ARCHITECTURE.md](../ARCHITECTURE.md)、[DESKTOP_IMPLEMENTATION_PLAN.md](./DESKTOP_IMPLEMENTATION_PLAN.md)。

状态标签：**✅ 已对齐** · **◐ 部分对齐**（标注缺项）· **❌ 未对齐**

---

## 已对齐（高层级）

| 领域 | TUI / runtime | DS Pick |
|------|----------------|---------|
| 会话与线程 | 多线程会话模型；切换 | 会话列表，新建，固定/重命名/归档，线程切换，删除（HTTP + 事件） |
| 对话流程 | 编写、流式输出、停止 | 编写器，停止，乐观草稿 + 回滚 |
| 历史 | 滚动、跳转至消息 | 可滚动的对话记录；从历史弹窗跳转至消息 |
| 消息类型 | 用户 / 助手 / 工具 / 元 | 按角色着色，工具调用以块展示，思考（流式），错误 |
| 附件 | 图片路径 / 粘贴（平台相关） | 粘贴图片；选择文件（图片 + 其他）；大小限制；编写器中二进制文件预览 |
| 工具交互 | 工具运行指示器，审批 | 工具运行徽章；审批门 + **✅ HTTP 交互式审批**（`POST /v1/threads/{id}/turns/{turn_id}/resolve-approval`，pending 队列，默认 120s 超时自动 deny） |
| 工作区 | 根目录，`read_file`，搜索，`stat_path`，目录树，二进制文件可选 | 浏览/搜索/展开目录树；读取文本 + 仓库根目录下的可选二进制文件；`stat_path`；**✅ 资源管理器中打开**（`open_in_shell`） |
| 模型 | 模型列表，参数，API 地址 | 运行时下拉列表（V4 Pro / V4 Flash）；健康检查；**✅ 模型参数**（Composer 齿轮 → `ModelParamsDialog` → runtime 透传） |
| 会话控制 | 清除/回退至消息；检查点 | ◐ 工作区级快照恢复；**✅ 编辑上一条用户消息**（`POST .../edit-last-turn` + MessageBubble） |
| 设置与文档 | `/config`，模型参数 | 健康检查，链接，打开用户数据目录；**✅ 导出会话 JSON** |
| 离线 / 重连 | 崩溃检查点 + `--resume` | **✅ 运行时连接检测**（8s 间隔 probe），fetch 退避重试（指数退避 ×5），sidecar 自动重启（5s 心跳 ×3 失败），启动时 `waitForRuntimeReady`（90s 超时） |
| 平台 | 终端界面 | 原生窗口，系统标题栏，通知（turn 完成 tab 隐藏可推送）；**✅ 系统托盘**（`tray-icon` feature） |
| 多窗口 / 多项目 | TUI 单终端 | **✅ 真多窗口**（2026-05-21）：`WebviewWindow` + 单实例 + 每窗 workspace/会话过滤 + 并行 turn；见 [multi-window-plan.md](multi-window-plan.md) |

---

## 明显差距（产品 / 编排）

以下功能在 TUI 中更完整，DS Pick **仍有差距**或仅为初级接入（2026-05-11 审核）。

- **子代理** — ◐ 已有 `AgentPanel` + `agentStates` SSE 聚合（`App.tsx`）；工具卡片内与子代理的**深度联动**、badge 等仍弱于 TUI。
- **任务 / 技能** — ◐ 侧栏「任务与技能」：`AutomationPanel.tsx`（仅任务 + 技能）。**定时自动化列表不展示**（见上文产品说明）。
- **MCP** — ✅ `McpPanel.tsx` + `fetchMcpServers` / `fetchMcpTools`；与 TUI 相比_enable/disable 等进阶操作以实际 API 为准。
- **用量 / 费用** — ✅ `UsageDashboard.tsx` + `fetchUsage`（recharts）；细粒度与 TUI `/cost` 对标程度未逐项验证。
- **自动模型路由** — **✅** `RoutingPanel` + `route_intent` 全链路透传（单测 `start_turn_applies_route_intent_routing_rule_to_model`）。
- **TUI 斜杠命令的深度交互** — 丰富的 `/` 菜单、面板、快捷键、文档内联支持（与终端产品对标）。
- **部分高级线程操作** — 例如仅导出线程、复制、合并、批量归档模式（TUI 或脚本已支持的）。

*注：* MCP、用量、任务列表、技能列表等已在 DS Pick 侧栏「设置」子菜单中接入。**定时自动化**（`GET /v1/automations`，cron 类）按产品决策**暂不展示**：后端与 `client.fetchAutomations()` 仍保留，面板仅含「任务」「技能」两标签（右栏 `view` 仍为 `automation`，标题为「任务与技能」）。

---

## 偏向「UI 打磨」的差距

以下是较小差异，无需新的后端约定即可提升功能对标度：

- **内联编辑**已发送的用户消息（TUI 支持编辑会话中的前序消息）。
- **键盘优先**导航（焦点环、侧边栏/编写器/历史的快捷键）。
- **智能粘贴** — **✅** `Composer.tsx` + `sanitizeHtml.ts`（HTML→纯文本、code fence）。
- **内联编辑** — ◐ `MessageBubble` 编辑 UI 已有；待 `POST .../edit-last-turn` + `App.tsx` 接线。
- **无障碍** — 屏幕阅读器标签、减少动画、高对比度模式（Tauri + Web）。
- **终端模拟器** — ✅ `TerminalCard.tsx` + xterm.js；`tool.progress` SSE 增量写入终端（F1a，2026-05-23）。
- **Diff 可视化** — ✅ `DiffCard.tsx` + diff2html；`edit_file`/`apply_patch`/`write_file` 在 turn 进行中也可预览 diff（F1b）。
- **子代理状态面板** — 与「明显差距」中的子代理条目相同，归类于此阶段作为纯前端工作。
- **资源管理器中打开** — **✅** `open_in_shell` Tauri command + `WorkspaceFilesPanel`。
- **导出会话 JSON** — **✅** Composer 菜单 + `export_session_json` / `export_thread_json`。

---

## 已完成的排期项

以下 `TUI_DS_PICK_GAP.md` 旧版建议排期项已在代码中落地。

| 原条目 | 落地位置 | 摘要 |
|--------|----------|------|
| **交互式 HTTP 审批**（原 Phase 2a） | `runtime_api.rs` `POST .../resolve-approval`；`runtime_threads.rs` pending 队列 + timeout guard（默认 120s，`DEEPSEEK_RUNTIME_APPROVAL_TIMEOUT_SECS` 可覆盖）；`ApprovalDialog.tsx` | 文档原列为 Phase 2a 待实现，实际代码已完成 |
| **SSE 断线重连** | `client.ts:waitForRuntimeReady()`、`useEffect` 8s probe、`fetchResponseWithBackoff`（指数退避 ×5） | `since_seq` 补读 + `getThreadEvents` 重连 |
| **Sidecar 自动重启** | `sidecar.rs` 健康检查 5s 间隔 ×3 失败 → 重启；端口冲突检测与回收 | 启动时 10 次重试 ×1s |
| **二进制文件预览** | `commands.rs:read_thread_workspace_binary` + MIME 嗅探 | Tauri command 绕过 runtime API 的 UTF-8 only 限制 |
| **CORS 配置** | `--cors-origin` + `DEEPSEEK_CORS_ORIGINS` + `[runtime_api] cors_origins` | 三层叠加，built-in 默认已包含 `tauri://localhost` 和 `http://tauri.localhost` |
| **Token 安全注入** | Tauri `setup` 生成 UUID token → 环境变量传给 sidecar → WebView `invoke('get_runtime_token')` → 闭包持有，不暴露到 `window` | |

---

## 审核说明（2026-05-10 代码交叉验证；2026-05-11 补充 Web UI）

| 声明 | 验证源 | 结论 |
|------|--------|------|
| HTTP 交互式审批已实现 | `runtime_api.rs`、`runtime_threads.rs`、`ApprovalDialog.tsx` | ✅ |
| SSE 断线重连 | `client.ts` `waitForRuntimeReady`，`App.tsx` probe | ✅ |
| Sidecar 重启 | `sidecar.rs` supervisor loop | ✅ |
| MCP / Usage 端点 | `runtime_api.rs` | ✅ |
| DS Pick：MCP / 用量 / 任务技能 / 子代理 / 路由 / 模型参数 UI | `McpPanel`、`UsageDashboard`、`AutomationPanel`、`AgentPanel`、`RoutingPanel`、`ModelParamsDialog` | ◐–✅ 见上文「实施步骤」总表 |
| 定时自动化 UI | 产品不展示；`fetchAutomations` 仍保留 | ⏸ 暂缓 |
| 通知 plugin | `main.rs`；无前端触发 | ◐ |
| 托盘 | `Cargo.toml` 无 `tray-icon` | ❌ |
| 资源管理器中打开 / 导出会话 JSON | `commands.rs`、菜单 | ❌ |

---

## 建议排期（剩余 / 验证项，2026-05-11）

文档原「10 项」中，多项已在 Web UI 落地；下列为**仍建议投入**或**需验证**的工作：

1. ~~**Terminal 集成（xterm.js）**~~ — ✅ `TerminalCard` + 增量 `tool.progress`（F1a）。
2. ~~**Diff 可视化（diff2html）**~~ — ✅ `DiffCard` + 右栏 Diff 面板（F1b）；运行中 diff 预览 2026-05-23。
3. **快捷键 & 无障碍** — ✅ Skip link、`#main-content`、工具/diff aria、focus-visible、reduced-motion、Composer Tab 顺序、roving tablist；**G2 §8 手测已签**（2026-05-24）。
4. ~~**资源管理器中打开工作区**~~ — ✅ `open_in_shell` Tauri command。
5. ~~**导出会话 JSON**~~ — ✅ Composer 菜单 + `export_session_json` / `export_thread_json`。
6. **内联编辑 / 智能粘贴** — 与后端「改历史消息」能力对齐后再做。
7. **自动模型路由** — 核对 `RoutingPanel` 与 runtime `start_thread_turn` 是否完全一致。
8. **定时自动化 UI** — 需要时再打开「自动化」标签（`fetchAutomations` 已存在于 `client.ts`）。

---

## 实施步骤计划（归档 — 对照代码）

下列为 2026-05-10 编写的分解任务；**2026-05-11 审核**后状态如下。详细子任务仍以本节表格为准，实施时以仓库源码为准。

| # | 主题 | 状态 | 实现要点 |
|---|------|------|----------|
| 1 | MCP 管理面板 | **✅** | `McpPanel.tsx`、`client.ts` |
| 2 | 用量仪表盘 | **✅** | `UsageDashboard.tsx`、`fetchUsage` |
| 3 | 任务 / ~~自动化~~ / 技能 | **◐** | `AutomationPanel.tsx`：任务 + 技能；**定时自动化不展示** |
| 4 | 子代理面板 | **✅** | `AgentPanel.tsx` + `AgentSpawnInline` 工具卡联动 |
| 5 | Terminal（xterm） | **✅** | `TerminalCard.tsx`；F1a 增量 progress |
| 6 | Diff（diff2html） | **✅** | `DiffCard.tsx` + `DiffPanel`；运行中预览 |
| 7 | 快捷键 & a11y | **✅** | Skip link、roving tablist、reduced-motion、`ModelParamsDialog` i18n + dialog 语义；**G2 §8 手测已签**（2026-05-24） |
| 8 | 模型参数 + 资源管理器 + 导出 JSON | **✅** | 8a/8b/8c 均已落地 |
| 9 | 智能粘贴 & 内联编辑 & 回溯分支 | **✅** | 智能粘贴 **✅**；编辑上一条 **✅**；历史用户消息「分支」→ `fork-at-user-message` **✅** |
| 10 | 自动模型路由 | **✅** | `RoutingPanel` + runtime 单测 |

**侧边栏**：`Sidebar.tsx`「设置」折叠下：`API Key`、`MCP 服务器`、`用量仪表盘`、`任务与技能`、`子代理`、`模型路由`。演示布局见 `TUI_DS_PICK_GAP_DEMO.html`。

以下为 **2026-05-10** 保留的原始子任务分解（便于查文件级目标），**不计为未完成任务清单**：

### 1. MCP 管理面板（前端 1-2 周）

**后端就绪**: `GET /v1/apps/mcp/servers` · `GET /v1/apps/mcp/tools`（`runtime_api.rs`）

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 1.1 API 层封装 | `crates/desktop/web-ui/src/api/client.ts` | 新增 `fetchMcpServers()` / `fetchMcpTools()` 函数，复用 `fetchJson<T>()` |
| 1.2 MCP 服务器列表组件 | `crates/desktop/web-ui/src/components/McpPanel.tsx`（新建） | 展示服务器名、传输类型、状态（connected/disconnected）；支持 enable/disable 切换 |
| 1.3 工具清单子组件 | `crates/desktop/web-ui/src/components/McpToolsList.tsx`（新建） | 按服务器分组展示 tool name + description，只读 |
| 1.4 路由与侧边栏入口 | `App.tsx` + `Sidebar.tsx` | 在「设置」可折叠子树中新增 "MCP 服务器" 入口（`.sub-nav-item`），点击展开设置并打开 `McpPanel` |
| 1.5 空状态 & 错误态 | 上图组件 | 无服务器时的引导文案；API 错误时的重试按钮 |

**验证**: 启动 runtime → 配置至少一个 MCP 服务器 → 面板可见工具列表 → 切换 enable/disable 即时生效。

---

### 2. 用量 / 费用仪表盘（前端 1 周）

**后端就绪**: `GET /v1/usage`，支持 `group_by=day|model|provider|thread`（`runtime_api.rs`）

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 2.1 API 层封装 | `client.ts` | 新增 `fetchUsage(params)` → `GET /v1/usage`，透传 `group_by`、`from`/`to` 时间范围 |
| 2.2 Usage Dashboard 页面 | `components/UsageDashboard.tsx`（新建） | 顶部：时间范围选择器（7d / 30d / 自定义）；图表区：recharts 柱状图/折线图 |
| 2.3 分组维度切换 | 同上 | 下拉切换 `group_by`（按日/模型/Provider/线程） |
| 2.4 路由与入口 | `App.tsx` + `Sidebar.tsx` | 在「设置」可折叠子树中新增 "用量仪表盘" 入口 |

**依赖**: `package.json` 需添加 `recharts`（或项目已有图表库则复用）。

**验证**: 产生若干 API 调用后，仪表盘按天展示 token 消耗柱状图 → 切换分组维度图形正确变化。

---

### 3. 任务 / ~~自动化~~ / 技能入口（前端 1.5 周）

**后端就绪**: `GET /v1/tasks` · `GET /v1/automations` · `GET /v1/skills`（`runtime_api.rs`）

**2026-05-11**：**不展示**定时自动化（`automations` 标签与列表已移除）；侧栏文案为「**任务与技能**」。`fetchAutomations()` 保留在 `client.ts` 供后续开启。

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 3.1 API 层封装 | `client.ts` | `fetchTasks()` / `fetchSkills()`；`fetchAutomations()` 保留 |
| 3.2 Tasks 面板 | `components/AutomationPanel.tsx` | 任务列表 |
| 3.3 ~~Automations 面板~~ | — | **暂缓** |
| 3.4 Skills 面板 | `components/AutomationPanel.tsx` | 技能列表 |
| 3.5 统一入口 | `App.tsx` + `Sidebar.tsx` | 「任务与技能」→ `view === 'automation'` |

**验证**: 任务与技能列表可加载；自动化 UI 不可见。

---

### 4. 子代理面板（前端 2 周）

**后端就绪**: SSE 事件 `agent.list` · `agent.spawn` · `agent.completed`（`runtime_threads.rs`）

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 4.1 事件流解析 | `App.tsx`（`onSseEvent` handler） | 在现有 SSE 事件处理中捕获 `agent.*` 事件，存入 React state |
| 4.2 AgentPanel 组件 | `components/AgentPanel.tsx`（新建） | 顶部摘要条：Running / Completed / Interrupted 计数；列表项：agent_id、状态徽标、spawn 时间 |
| 4.3 折叠式详情 | 同上 | 点击展开 → 显示 agent 的 tool calls 序列、最终 result 摘要 |
| 4.4 与 ToolCard 联动 | `components/ToolCard.tsx` | 当 `agent_spawn` 工具卡出现时，下方显示子代理状态追踪块 |
| 4.5 侧边栏入口 | `App.tsx` + `Sidebar.tsx` | 在「设置」可折叠子树中新增 "子代理" 入口，显示运行中计数 badge |

**依赖**: 条目 3 不阻塞此项；可以是独立的前端分支。

**验证**: 触发 `agent_spawn` → 面板实时显示 Running 状态 → agent 完成后状态变为 Completed → 可展开查看详情。

---

### 5. Terminal 集成（xterm.js，1.5 周）

**后端就绪**: `tool.progress` SSE 事件携带实时 Shell 输出（`runtime_threads.rs`）

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 5.1 依赖安装 | `package.json` | 添加 `@xterm/xterm` + `@xterm/addon-fit` |
| 5.2 TerminalCard 组件 | `components/TerminalCard.tsx`（新建） | 嵌入 xterm.js Terminal 实例；从 `tool.progress` 事件流 feed 数据到 `terminal.write()` |
| 5.3 ToolCard 路由改造 | `components/ToolCard.tsx` | 当 tool 类型为 `exec_shell` / `task_shell_start` 时渲染 `TerminalCard` 而非纯文本 `<pre>` |
| 5.4 回退行为 | 同上 | 若 xterm.js 加载失败或不可用，降级为现有的纯文本 `<pre>` 输出 |

**依赖**: `package.json` 需 `@xterm/xterm`、`@xterm/addon-fit`。

**验证**: 触发 `exec_shell "ping -c 3 localhost"` → TerminalCard 实时逐行渲染输出，保留 ANSI 颜色。

---

### 6. Diff 可视化（diff2html，1 周）

**后端就绪**: `edit_file` / `apply_patch` 结果已在 ToolCard 中展示文本 diff。

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 6.1 依赖安装 | `package.json` | 添加 `diff2html` |
| 6.2 DiffCard 组件 | `components/DiffCard.tsx`（新建） | 接受 unified diff 字符串 → 调用 `Diff2Html.html()` → 渲染 |
| 6.3 ToolCard 路由改造 | `components/ToolCard.tsx` | 当 tool 名称为 `edit_file` / `apply_patch` 时，将 result 以 `DiffCard` 渲染 |
| 6.4 样式 | `diff2html/bundles/css/diff2html.min.css` | 确保 side-by-side / line-by-line 样式在 Tauri WebView 中正确加载 |

**依赖**: `diff2html` npm 包。

**验证**: 触发 `edit_file` 修改已知文件 → 结果以 side-by-side diff 展示，增删行着色正确。

---

### 7. 快捷键 & 无障碍（2 周）

**无后端依赖**。纯 Web UI 改造。

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 7.1 焦点环 | 全局 CSS + 各组件 | 所有可交互元素（按钮、输入框、列表项）增加 `:focus-visible` 样式，Tab 键导航顺序合理 |
| 7.2 全局快捷键 | `App.tsx` + `hooks/useKeyboardShortcuts.ts`（新建） | `Ctrl+K` 打开命令面板；`Ctrl+N` 新会话；`Ctrl+Shift+F` 聚焦搜索；`Escape` 关闭弹窗/面板 |
| 7.3 aria 标签 | 遍历所有组件 | 为侧边栏按钮、对话列表、工具卡片、审批对话框添加 `aria-label` / `role` |
| 7.4 减少动画 | CSS 媒体查询或状态 | `prefers-reduced-motion` 检测 → 禁用过渡动画 |
| 7.5 高对比度 | CSS 变量覆盖 | `prefers-contrast: more` → 切换到高对比色板 |

**验证**: Tab 键可遍历全部交互元素 → VoiceOver / NVDA 可读出每个按钮用途 → 减少动画模式下无闪烁过渡。

---

### 8. 模型参数 + 资源管理器 + 导出 JSON（1 周）

三个独立的小缺口，可并行开发。

#### 8a. 模型参数对话框

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 8a.1 ModelParamsDialog | `components/ModelParamsDialog.tsx`（新建） | 弹出对话框：temperature slider、top_p slider、max_tokens 输入框 |
| 8a.2 触发入口 | 模型选择器下拉旁新增齿轮图标 | 点击打开 ModelParamsDialog |
| 8a.3 参数注入 | `App.tsx` | 将参数合并到 `POST /v1/threads/{id}/turns` 的请求体中 |

#### 8b. 资源管理器中打开

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 8b.1 Tauri command | `commands.rs` | 新增 `#[tauri::command] fn open_in_shell(path: String)`，调用 `open::that()` 或平台 shell |
| 8b.2 前端触发 | 工作区面包屑旁新增按钮 | 点击调用 `invoke('open_in_shell', { path })` |

#### 8c. 导出会话 JSON

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 8c.1 Tauri command | `commands.rs` | 新增 `#[tauri::command] fn export_thread_json(thread_id: String)` → 调用 Tauri save dialog → 写入文件 |
| 8c.2 前端触发 | 线程操作菜单 | "导出 JSON" 菜单项 → `invoke('export_thread_json', { threadId })` |

**验证**: 模型参数对话框 → 调滑块 → 下一次 turn 携带新参数；资源管理器按钮 → 系统文件管理器弹出；导出 JSON → 保存对话框出现 → 文件内容完整。

---

### 9. 智能粘贴 & 内联编辑（1 周）

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 9.1 粘贴处理器 | 编写器组件 | 监听 `paste` 事件 → 检测 HTML 内容 → `text/html` → `text/plain` 降级 → 注入纯文本 |
| 9.2 代码块检测 | 同上 | 粘贴内容含缩进/花括号时自动包裹为 Markdown code fence |
| 9.3 内联编辑入口 | `components/MessageBubble.tsx` | 用户消息气泡增加 "编辑" 按钮 → 点击切换为编辑模式 |
| 9.4 编辑提交 | `App.tsx` | 编辑完成后发送修正请求（或仅在本地更新 UI 状态，取决于后端是否支持消息编辑） |

**注意**: 条目 9.3-9.4 需确认后端是否支持编辑已发送消息；若不支持，编辑后为本地乐观更新。

**验证**: 从 VS Code 复制高亮代码 → 粘贴到编写器 → 自动包裹为 ```...```；右键用户消息 → 编辑 → 修改后提交。

---

### 10. 自动模型路由（后端 + 前端 2 周）

**与前述纯前端条目不同，此项需要后端新端点。**

| 子任务 | 目标文件 | 说明 |
|--------|----------|------|
| 10.1 路由规则定义 | `runtime_api.rs` | 新增 `POST /v1/apps/routing/rules` 端点，接受规则 JSON（意图 → 模型映射） |
| 10.2 规则持久化 | 配置层 | 规则持久化到用户配置目录 |
| 10.3 路由决策点 | `runtime_threads.rs` | 在 `start_thread_turn` 路径中插入路由决策：根据用户消息意图匹配规则 → 选择模型 |
| 10.4 前端配置面板 + 入口 | `components/RoutingPanel.tsx`（新建）+ `Sidebar.tsx` | 规则列表 CRUD；意图下拉（chat / code / research / custom）；模型选择器。入口在「设置」可折叠子树中 |
| 10.5 路由日志 | 同上 | 每次路由决策记录日志（意图 → 选择模型 → 置信度），面板可查看 |

**验证**: 创建规则「code 意图 → V4 Pro」→ 发送编程问题 → 请求实际使用 V4 Pro 模型 → 路由日志可见。

**2026-05-11**：前端 `RoutingPanel.tsx` 已存在；**10.1–10.3、10.5** 是否全部在当期 `runtime` 中实现需以 `runtime_api.rs` 与集成测试为准。

---

### 全局优先级与依赖图（更新）

```
已完成 / 部分完成（DS Pick）
  ├─ 1. MCP 管理面板 ✅
  ├─ 2. 用量仪表盘 ✅
  ├─ 3. 任务 + 技能 ◐（自动化 UI 关闭）
  ├─ 4. 子代理面板 ◐
  ├─ 8a. 模型参数 ◐
  └─ 10. 路由配置 UI ◐（引擎全链路待核对）

仍待排期
  ├─ 5. Terminal（xterm）
  ├─ 6. Diff（diff2html）
  ├─ 7. 快捷键 & 无障碍（系统化）
  ├─ 8b / 8c 资源管理器 / 导出 JSON
  ├─ 9. 智能粘贴 & 内联编辑
  └─ 3b. 定时自动化（产品需要时再开）
```

---

## 如何使用本文档

- **产品：** 按「明显差距」和「UI 打磨」对表格行进行排期优先级排序；**自动化**定时任务默认不进入 DS Pick UI。
- **工程：** 针对每个差距，先对照 **RUNTIME_API**（已有的 vs 需要的新端点）进行追溯，再构建仅 UI 层面的方案。

*最后更新：2026-05-11（实施审核收尾 + 自动化不展示）*

