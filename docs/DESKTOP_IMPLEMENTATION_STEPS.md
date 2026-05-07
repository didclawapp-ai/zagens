# DeepSeek Desktop 详细实施步骤计划

> **关联文档**：[DESKTOP_IMPLEMENTATION_PLAN.md](./DESKTOP_IMPLEMENTATION_PLAN.md)（架构、风险、模块划分）  
> **阶段顺序**：Phase 1 → Phase 2 → **Phase 2a** → Phase 3 → Phase 4 → Phase 5  

本文把各阶段拆成**可执行步骤**：输入假设、操作顺序、产出物、验收方式。实施时按勾选推进；任一阶段验收失败则不得进入下一阶段。

---

## 零、开始前（环境与约束）

### 0.1 工具链

| 项 | 要求 |
|----|------|
| Rust | **1.88+**（与 workspace `rust-version` 一致） |
| Cargo | stable，勿引入 nightly `#![feature]` |
| Node.js | **20 LTS** 或以上（前端 / Tauri CLI） |
| 平台 | Windows 10+/macOS/Linux（与 Tauri 2 支持矩阵一致） |

### 0.2 仓库内验证命令（贯穿全程）

任一 Rust 改动合并前建议在本地运行（与 `AGENTS.md` 一致）：

```bash
cargo build
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features
cargo fmt --all
```

Sidecar / HTTP 的手工验证：`deepseek-tui serve --http --port 7878 --auth-token <TOKEN>`（或 `deepseek serve --http`，与安装方式一致）。

### 0.3 依赖关系简图

```text
Phase 1（runtime 稳健性）
    └─► Phase 2（壳 + 流式对话）
            └─► Phase 2a（HTTP 审批）────┐
            └─►（可与 2 并行收尾）       │
                                          ▼
                              Phase 3（工具/UI，含 ApprovalDialog）
                                          │
                              Phase 4（配置/会话）
                                          │
                              Phase 5（打包发布）
```

**硬规则**：未通过 Phase 2a 验收前，不要将 Phase 3 的「交互式 ApprovalDialog + 非 YOLO Agent」标为完成。

---

## Phase 1：runtime_api / SSE / 广播（约 5 个工作日）

### 目标

Sidecar HTTP 在长 turn、重连场景下事件不无故丢失；桌面端可用的游标策略明确（`since_seq`，可选 SSE `id`）。

### 步骤

1. **`EVENT_CHANNEL_CAPACITY`**
   - 打开 [`crates/tui/src/runtime_threads.rs`](../crates/tui/src/runtime_threads.rs)，将常量由 `1024` 调至 `4096`（或与压测结论一致的上限）。
   - 备注：仅能缓解瞬时洪峰；慢消费者仍需 `events_since` 回补。

2. **`GET /v1/threads/{id}/events` 与消费者行为**
   - 阅读 [`stream_thread_events`](../crates/tui/src/runtime_api.rs) 与 `events_since`、`ThreadEventsQuery::since_seq`。
   - 确认集成测试已覆盖 `since_seq` 游标（已有则补缺场景：空 backlog、直播中切换 `since_seq`）。

3. **（可选）SSE `id:` 字段**
   - 在组装 `SseEvent` 时为每条事件设置 `id` 为单调 `seq`（与 JSON payload 一致），便于原生 `Last-Event-ID`。
   - 验收：Chrome/Safari `EventSource` 重连时若发送 `Last-Event-ID`，服务端行为符合预期（或与文档声明「仅以 query 为准」一致）。

4. **POST `/v1/stream` 语义冻结**
   - 对照 [`map_compat_stream_event`](../crates/tui/src/runtime_api.rs) 列出对外事件名表（如 `message.delta`、`tool.started`），写入团队可见的短文或 codegen 注释，供前端生成 TS 类型。

5. **`EventFrame` 对齐策略：默认选 B（保留 compat 名 + 前端映射）**
   - 理由：`map_compat_stream_event` 已在生产运行，改序列化层有回归风险；前端维护映射成本极低（一个 `switch`）。
   - 前端任务：基于 compat 事件名生成 TS 类型，维护 `compat → UI` 映射表。

### Phase 1 验收清单

- [ ] `cargo test` / 相关模块测试绿。
- [ ] `curl`/集成测试：`POST /v1/stream` 在有效 token 下可收到 `turn.started` … `done`。
- [ ] `GET …/events?since_seq=N` 重放连续，无倒挂 `seq`。

---

## Phase 2：Tauri 壳 + Web 前端 MVP（约 10 个工作日）

### 目标

独立窗口启动 sidecar，`fetch` 消费 `POST /v1/stream` SSE，会话列表可读，能进行一轮流式对话（Thinking 可分阶段后续做）。

### 步骤

1. **Workspace 与 crate**
   - 根 [`Cargo.toml`](../Cargo.toml) 增加 `crates/desktop` member。
   - 新建 `crates/desktop/`：Tauri 2 应用 scaffold，`web-ui/` 使用 Vite + React + TS（与总体规划一致）。

2. **Sidecar 启动命令（禁止臆造 flag）**
   - 调用：`deepseek-tui serve --http --host 127.0.0.1 --port <P> --auth-token <T>`。
   - 端口：默认可与配置一致或可探测空闲端口；与 `ServeArgs`（[`main.rs`](../crates/tui/src/main.rs)）对齐。
   - 将 token 写入子进程环境或参数；**同源**传给 Web（见下）。

   2a. **API Key 注入**：sidecar 启动时需确保 API Key 可用。若用户已在 `~/.deepseek/config.toml` 中配置，sidecar 自动读取；若首次使用，前端通过 `POST /v1/stream` 返回的错误码判断缺 Key，引导进入 onboarding。

3. **Token 传递到前端**
   - Tauri：`window.__DEEPSEEK_RUNTIME_TOKEN__` 或 `invoke('get_runtime_token')`（二选一统一）。
   - 所有 `/v1/*` 请求：`Authorization: Bearer …`（或运行时支持的 header/query，与 [`require_runtime_token`](../crates/tui/src/runtime_api.rs) 一致）。

4. **HTTP + SSE 客户端**
   - 实现 **`postStreamSse`**：`fetch` + `ReadableStream` 解析 SSE 帧（**不用** `EventSource` POST）。
   - 实现 **`getThreadEvents`**：`EventSource` 或 fetch 长轮询式读 `GET /v1/threads/{id}/events?since_seq=…`，维护本地 `since_seq`。

5. **最小 UI**
   - `ChatView` + `Composer`：发送触发 `POST /v1/stream`（body：`prompt`、`workspace`、`mode`、`model?`、`auto_approve` 等与现有 JSON 对齐）。
   - 按事件增量更新消息区（先文本即可）。

6. **会话列表（只读即可）**
   - `GET /v1/sessions`，侧边栏或单独页展示；点击后 `GET /v1/sessions/{id}/resume-thread` 解析并渲染历史（字段以实际 API 为准）。

7. **健康检查与重启**
   - 周期 `GET /health`；连续失败触发 sidecar 重启；限制重试避免死循环。

### Phase 2 验收清单

- [ ] 无 API Key 时错误提示清晰（来自 runtime 或配置层）。
- [ ] 流式正文可见（`message.delta` 等）。
- [ ] Token 错误时 401，不白屏。
- [ ] `cargo build` 含 desktop member 成功（若 CI 尚未加，至少本地通过）。

---

## Phase 2a：HTTP 交互式审批（约 5～8 个工作日）

### 目标

在 `auto_approve: false` 的 Agent turn 中，危险工具执行**阻塞**直到 HTTP 批准/拒绝；行为与 TUI「用户决策」语义一致（含超时策略文档化）。

### 步骤（建议顺序）

1. **现状走读（半日）**
   - [`RuntimeThreadManager` 中 `EngineEvent::ApprovalRequired`](../crates/tui/src/runtime_threads.rs) 分支。
   - `Engine`：[`approve_tool_call` / `deny_tool_call`](../crates/tui/src/core/engine.rs) 与 turn 循环如何等待。

2. **设计文档（半日，以下结论直接固化，避免实施时讨论）**

   | 问题 | 结论 |
   |------|------|
   | pending 唯一键 | `(thread_id, turn_id, tool_call_id)` 三元组；`tool_call_id` 由 Engine 生成，全局唯一 |
   | 同 turn 多个待批 | **不可能**——Engine 的 turn loop 是顺序执行工具调用的，一次只一个待审批工具 |
   | 阻塞/唤醒机制 | `tokio::sync::oneshot`——Engine 端发 `tx`，`ApprovalRequired` 处理时 `await rx`；HTTP approve 路由拿到 `tx` 后 `send(Decision)` |
   | 超时策略 | 与 TUI 对齐：超时默认 **deny**，释放 Engine；超时时长通过 `config.toml` 的 `[exec] approval_timeout_secs` 配置，默认 120s |

3. **运行时改造**
   - `ApprovalRequired`：**当需要用户介入时**不立即 `deny`；登记 pending 并向 Engine 侧发送「等待信号」。
   - Engine/turn：在继续执行工具前阻塞在 await 上；HTTP 路由收到决策后调用已有 `approve`/`deny` 并完成 notify。

4. **HTTP 路由（[`runtime_api.rs`](../crates/tui/src/runtime_api.rs)）**
   - 注册 `POST …/approve` 与 `…/deny`（或单一 `decision` 端点）；路径与总体规划一致以便前端缓存。
   - 鉴权：沿用 `/v1` 层 middleware。

5. **SSE**
   - 保持 `approval.required` 先于用户操作到达前端；批准后后续 `tool.*` 事件与原流一致。

6. **测试**
   - 集成测试：mock 或小场景触发单次需批工具 → 断言未调用前 stalled → POST approve → 断言工具完成事件出现。

### Phase 2a 验收清单

- [ ] `auto_approve: false` + 触发 shell/write 等：流在 `approval.required` 后可恢复。
- [ ] deny 路径：模型/工具收到拒绝语义，无前缀死锁。
- [ ] 无审批 API 的旧客户端：行为明确（破坏性变更则写 CHANGELOG）。
- [ ] `cargo test` 覆盖新增路径。

---

## Phase 3：工具可视化（约 15 个工作日）

### 目标

桌面端对等展示 TUI 已支持的主要工具轨迹；**ApprovalDialog** 接 Phase 2a 的真 API。

### 建议实施顺序

1. **通用 `ToolCard` 骨架**（状态：`running` / `done` / `error`）。
2. **Terminal 类**：对接 `tool.progress`、`tool.completed` 中 shell/command 语义。
3. **文件/diff**：`read_file`、`write_file`、`edit_file`、`apply_patch`（以实际 payload 为准）。
4. **Git / Web**：只读摘要 → 按需展开全文。
5. **Sub-agent / Todo**：卡片 + 轮询或 SSE 子事件。
6. **ThinkingBlock**：当前 `map_compat_stream_event` 对 reason 与正文统一打 `message.delta`。实现时二选一：**(a)** 前端根据 `tool.started` 之前的 `message.delta` 序列推断为 reasoning 阶段；**(b)** 后端在 `message.delta` payload 中添加 `kind: "reasoning"` 字段。优先选 (a) 快速落地。
7. **ApprovalDialog**：绑定 Phase 2a 的 approve/deny；支持「单次 / 会话内始终」若 runtime 暴露策略或使用本地偏好 + `auto_approve` patch（与 API 对齐).

### Phase 3 验收清单

- [ ] Phase 3 所列工具族在端到端跑一次可见（或记录明确除外项）。
- [ ] 交互式审批在桌面完整闭环。
- [ ] 大消息列表滚动性能可接受（虚拟列表可选）。

---

## Phase 4：配置与会话（约 5 个工作日）

### 步骤摘要

1. Sidebar：会话 CRUD（以 `GET/PATCH/DELETE` 等与现有 routes 对齐为准）。
2. ConfigPanel：API Key / 模型 / profile；写入路径与 [`deepseek-config`](../crates/config) 约定一致。
3. 主题与语言：Tauri OS 钩子 + 本地存储。
4. Onboarding：首次缺 Key 引导。

### 验收清单

- [ ] 改配置后新 turn 生效策略明确（reload sidecar vs 热读）。
- [ ] 不破坏 CLI/TUI 共享配置文件。

---

## Phase 5：打包与发布（约 5 个工作日）

### 步骤摘要

1. Tauri `bundle`：`externalBin` 或 resources 嵌入 `deepseek-tui`（Windows/macOS/Linux 各矩阵）。
2. 代码签名与公证（按平台）。
3. CI：`matrix` 构建 artifact；checksum / release notes。
4. （可选）Tauri updater，注意更新包与 sidecar 版本同步策略。

### 验收清单

- [ ] 干净机器离线安装后能启动并完成 Phase 2 最小对话。
- [ ] Sidecar 与壳版本兼容性写入发布说明。

---

## 附录 A：常见问题与对策

| 现象 | 可能原因 | 对策 |
|------|----------|------|
| POST SSE 无法用 EventSource | 浏览器限制 GET | 使用 fetch 流解析 |
| 事件「跳号」或重复 | broadcast `Lagged` | `since_seq` 全量回补 |
| 审批立即 deny | Phase 2a 未上或仍走旧分支 | 查 `ApprovalRequired` 路径 |
| sidecar 启动后马上退出 | API Key 未配置或无效 | 检查 `~/.deepseek/config.toml` 或环境变量 `DEEPSEEK_API_KEY` |
| `POST /v1/stream` 返回非 SSE 的 JSON 错误 | prompt 字段缺失或 workspace 路径不存在 | 对照 `StreamTurnRequest` 结构补齐必填字段 |

---

## 附录 B：与总体规划的序号对照

| 本文 | DESKTOP_IMPLEMENTATION_PLAN.md |
|------|-------------------------------|
| Phase 1 | §十 Phase 1 |
| Phase 2 | §十 Phase 2 |
| Phase 2a | §十 Phase 2a |
| Phase 3–5 | §十 Phase 3–5 |

实施过程中若架构决策变更（例如 Engine 上移 core），先回写总体规划再同步本文步骤。
