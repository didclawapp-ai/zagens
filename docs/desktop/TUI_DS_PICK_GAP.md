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
| 工作区 | 根目录，`read_file`，搜索，`stat_path`，目录树，二进制文件可选 | 浏览/搜索/展开目录树；读取文本 + 仓库根目录下的可选二进制文件；`stat_path`；◐ **资源管理器中打开**待实现 |
| 模型 | 模型列表，参数，API 地址 | 运行时下拉列表（V4 Pro / V4 Flash）；健康检查；◐ **模型参数对话框**（temperature / top_p 等）待实现 |
| 会话控制 | 清除/回退至消息；检查点 | ◐ 工作区级快照恢复（`GET .../snapshots` + `POST .../restore`）；单条消息粒度「清除到此位置」待实现 |
| 设置与文档 | `/config`，模型参数 | 健康检查，链接，打开用户数据目录；◐ **导出会话 JSON** 待实现 |
| 离线 / 重连 | 崩溃检查点 + `--resume` | **✅ 运行时连接检测**（8s 间隔 probe），fetch 退避重试（指数退避 ×5），sidecar 自动重启（5s 心跳 ×3 失败），启动时 `waitForRuntimeReady`（90s 超时） |
| 平台 | 终端界面 | 原生窗口，系统标题栏，通知（plugin 已注册）；◐ **系统托盘**（`Cargo.toml` 未启用 `tray-icon` feature） |

---

## 明显差距（产品 / 编排）

以下功能存在于 TUI / 调度器领域，但目前在 DS Pick **不是**一等公民。

- **子代理** — `agent_spawn` / `agent_wait` / 编排界面。Web UI 无专门面板展示子代理状态、进度和结果；工具卡片中未区分普通工具与子代理。
- **任务 / 自动化 / 技能** — 保存的任务流程，自动化钩子，可移植的技能配置。后端有 `GET /v1/tasks`、`GET /v1/automations`、`GET /v1/skills`，但 Web UI 无入口。
- **MCP** — 进程内服务端配置及工具展示（如 TUI 中的做法）。**◐ 后端已就绪**（`GET /v1/apps/mcp/servers`、`GET /v1/apps/mcp/tools`），但 Web UI 无专用管理面板。
- **用量 / 费用 / Token 统计图表** — 基础健康检查之外的仪表盘。**◐ 后端已就绪**（`GET /v1/usage`，支持 `group_by=day|model|provider|thread`），Web UI 无图表展示。
- **自动模型路由** — 基于意图或策略的模型选择。
- **TUI 斜杠命令的深度交互** — 丰富的 `/` 菜单、面板、快捷键、文档内联支持（与终端产品对标）。
- **部分高级线程操作** — 例如仅导出线程、复制、合并、批量归档模式（TUI 或脚本已支持的）。

*注：* 后端已就绪的项目（MCP、用量）只需 Web UI 前端开发即可上线，不再依赖 runtime 改动。

---

## 偏向「UI 打磨」的差距

以下是较小差异，无需新的后端约定即可提升功能对标度：

- **内联编辑**已发送的用户消息（TUI 支持编辑会话中的前序消息）。
- **键盘优先**导航（焦点环、侧边栏/编写器/历史的快捷键）。
- **智能粘贴**代码块 / HTML → 纯文本规范化（对标 TUI 的粘贴行为）。
- **无障碍** — 屏幕阅读器标签、减少动画、高对比度模式（Tauri + Web）。
- **终端模拟器** — TUI 中 Shell 命令输出实时滚动显示；DS Pick 的 ToolCard 仅显示纯文本 output，无 xterm.js 集成。后端已通过 `tool.progress` SSE 事件推送实时输出流。
- **Diff 可视化** — TUI 中 `edit_file`/`apply_patch` 结果以 diff 形式展示；DS Pick 无 diff2html 集成。
- **子代理状态面板** — 与「明显差距」中的子代理条目相同，归类于此阶段作为纯前端工作。
- **资源管理器中打开** — TUI 支持在系统文件管理器中打开工作区路径；DS Pick 无对应 Tauri command。
- **导出会话 JSON** — TUI 支持将会话导出为 JSON 文件；DS Pick 仅自动 persist，无用户可见的导出入口。

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

## 建议排期（供路线图讨论）

根据代码现状重新排序：

1. **MCP 管理面板**（前端 1-2w）— 后端已就绪（`GET /v1/apps/mcp/servers`、`GET /v1/apps/mcp/tools`），需 Web UI 添加服务器列表、启用/禁用、工具展示。工作量：纯前端，低风险。
2. **用量 / 费用仪表盘**（前端 1w）— 后端 `GET /v1/usage` 已实现（v0.8.10, #564），支持 `group_by=day|model|provider|thread`。需 Web UI 添加图表（recharts）与时间范围选择器。
3. **任务 / 自动化 / 技能入口**（前端 1.5w）— 后端三个端点均已就绪，需 Web UI 侧边栏或独立面板展示任务列表、自动化定时任务、已安装技能。
4. **子代理面板**（前端 2w）— `agent_spawn`/`agent_wait` 状态追踪 + 进度展示 + 结果摘要。后端通过 `item.*` + `agent.list` 事件推送子代理状态。
5. **Terminal 集成**（xterm.js，1.5w）— 将 `tool.progress` 中的 Shell 实时输出渲染到终端模拟器，替代当前的纯文本 ToolCard。
6. **Diff 可视化**（diff2html，1w）— `edit_file`/`apply_patch` 结果的 diff 友好展示。
7. **快捷键 & 无障碍**（2w）— 键盘导航焦点环、快捷键、屏幕阅读器标签、高对比度模式。
8. **模型参数对话框 + 资源管理器 + 导出会话 JSON**（1w）— 三个较小的 UI 缺口，不影响核心对话流程。
9. **智能粘贴 & 内联编辑**（1w）— HTML → 纯文本规范化、编辑已发送消息。
10. **自动模型路由**（后端+前端 2w）— TUI 已有按意图分派；桌面端需新增 UI 配置入口。

---

## 审核说明

本版文档经 2026-05-10 代码级交叉验证。每条关键声明均对照实际源码确认：

| 声明 | 验证源 | 结论 |
|------|--------|------|
| HTTP 交互式审批已实现 | `runtime_api.rs:383`，`runtime_threads.rs:2572/2846`，`ApprovalDialog.tsx` | ✅ |
| SSE 断线重连 | `client.ts:117` `waitForRuntimeReady`，`App.tsx:370` 8s probe | ✅ |
| Sidecar 重启 | `sidecar.rs:13-16` 常量，`start_and_monitor` supervisor loop | ✅ |
| MCP 端点 | `runtime_api.rs:413-414` | ✅ |
| Usage 端点 | `runtime_api.rs:429`，测试覆盖 `group_by=day\|model\|provider\|thread` | ✅ |
| CORS built-in | `runtime_api.rs:2187-2191` `DEFAULT_CORS_ORIGINS` | ✅ |
| Agent list 事件 | `engine.rs:692` `Event::AgentList`，`runtime_threads.rs:2531` | ✅ |
| 审批超时默认 120s | `runtime_threads.rs:487`，`DEEPSEEK_RUNTIME_APPROVAL_TIMEOUT_SECS` | ✅ |
| 通知 plugin 已注册 | `main.rs:16` `tauri_plugin_notification`；**无前端触发入口** | ◐ 部分 |
| 托盘未实现 | `Cargo.toml` 无 `tray-icon` feature | ❌ |
| 资源管理器中打开 | `commands.rs` 无对应 Tauri command | ❌ |
| 导出会话 JSON | Web UI 无导出入口 | ❌ |
| 模型参数对话框 | Web UI 无 temperature/top_p 等参数 UI | ❌ |

---

## 如何使用本文档

- **产品：** 按「明显差距」和「UI 打磨」对表格行进行排期优先级排序。
- **工程：** 针对每个差距，先对照 **RUNTIME_API**（已有的 vs 需要的新端点）进行追溯，再构建仅 UI 层面的方案。

*最后更新：2026-05-10（代码级交叉验证版）*
