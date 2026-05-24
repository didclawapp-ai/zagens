# G2 / PR5 手测清单（DS Pick + Sidecar）

> **用途：** 补全 [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md) 中「桌面全链路 / 多窗口 / 审批」人工项。  
> **预计：** 核心路径 **15–20 分钟**；全表 **~35 分钟**。  
> **记录：** 每项勾 `[x]`，失败写「步骤 # + 现象 + 截图/日志路径」。

**自动化已绿（提交前无需重复）：** `cargo test -p deepseek-tui --lib`、`runtime_event_replay_fixture`、`sidecar_contract_full_lifecycle`、`sidecar_parallel_turns_on_two_threads`。

---

## 0. 环境准备

| # | 检查项 | 通过 |
|---|--------|------|
| 0.1 | `~/.deepseek/config.toml` 或项目 `.env` 已配置 **DeepSeek API Key**（能正常对话） | [ ] |
| 0.2 | 无其它进程占用 **7878**（旧 sidecar 已退出） | [ ] |
| 0.3 | 使用 **debug 或 release 桌面** 均可；长跑/基线才强制 release | [ ] |

**启动方式（二选一）：**

```powershell
# A) 开发（推荐手测）— 在 crates/desktop 目录执行
cd F:\DeepSeek-TUI-desktop\crates\desktop

# 首次：安装 Tauri CLI 2（每台机器一次）
# cargo install tauri-cli --version "^2"

# 启动 DS Pick（会拉起 sidecar + WebView）
cargo tauri dev

# 勿用 npm run tauri dev — crates/desktop/package.json 无该 script

# B) 已安装包 / release 产物
# npm run bundle:prepare 后 cargo tauri build，或从安装目录启动
```

**可选 — 仅验证 runtime 健康（不打开 UI）：**

```powershell
# sidecar 起来后（桌面会自动拉）
curl -s http://127.0.0.1:7878/health
# 期望 JSON 含 "status":"ok" 且 "event_schema_version":2
```

| # | 检查项 | 通过 |
|---|--------|------|
| 0.4 | `GET /health` 返回 `event_schema_version: 2` | [x] |

---

## 1. G2 — 单窗全链路对话（§12.2 #3）

**目标：** 不改 `engine.rs` 也能完成 thread → turn → 流式回复 → 结束。

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 1.1 | 打开 DS Pick，选/新建一个**工作区文件夹** | 侧栏、标题栏正常，无「runtime 不可用」常驻红条 | [ 通过] |
| 1.2 | 新建会话（或选空会话），**Agent 模式**，发一句简单 prompt（如「用一句话介绍当前目录」） | 出现流式正文；最终 turn 结束（可继续输入） | [ 通过] |
| 1.3 | 刷新页面或关掉再开**同一窗**（不杀进程），恢复同一会话 | 历史仍在；可再发一条消息 | [通过 ] |
| 1.4 | 开发者工具 Network（可选）：`/v1/threads/.../events` 或 stream 为 **SSE**，能看到 `turn.started` / `message.delta` 或 `turn.completed` 类事件 | 与 [API_DESIGN.md](../API_DESIGN.md) §3.2.1 一致 | [ 通过] |

---

## 2. G2 — 交互式审批（A+.7）

**目标：** `auto_approve: false` 时工具挂起 → 用户 resolve → turn 继续或按策略结束。

### 2.0 审批配置对照（为何「设置改了仍像 Auto」）

**配置文件 SSOT：** `config.example.toml` → `approval_policy`；环境变量 `DEEPSEEK_APPROVAL_POLICY`；优先级 **CLI > 环境变量 > 配置文件**（与 TUI 一致）。

**代码解析**（`deepseek-core::approval::ApprovalMode::from_config_value`）：

| `approval_policy` 配置值 | `ApprovalMode` | 系统提示词模板 | 模板标题 |
|--------------------------|----------------|----------------|----------|
| `auto` | Auto | `prompts/approvals/auto.md` | Approval Policy: **Auto** |
| `on-request`、`untrusted`、`suggest` | Suggest | `prompts/approvals/suggest.md` | Approval Policy: **Suggest** |
| `never`、`deny` | Never | `prompts/approvals/never.md` | Approval Policy: **Never** |

> **注意：** `config.example.toml` 注释只写 `on-request \| untrusted \| never`，**未写** `auto` / `suggest`，但运行时接受。  
> **`on-request` 与 `untrusted` 在代码里是同一档（Suggest）**，不是 Auto。

**DS Pick 审批 UI（2026-05-24）：** 系统设置含 **按需审批 / 仅不受信任 / 从不 / 自动批准** 四档。非 `auto` 时 Composer 显示只读「审批：按需审批」等（无灰掉复选框）；`auto` 时显示可勾选的「自动批准工具调用」。

**两套控制（桌面手测必看）：**

| 层级 | 字段 | 作用 |
|------|------|------|
| 设置 → 审批策略 | `approval_policy` → `config.toml` | 系统提示词 + sidecar `turn_lifecycle` 在 `auto_approve: false` 时读 config |
| Composer「自动批准工具调用」 | HTTP `auto_approve` | **仅**在设置选 **自动批准** 时出现；勾选则 runtime 立刻 approve |

**手测 §2 有效操作（按需审批）：**

1. 运行模式 **Agent**（非 YOLO）
2. 系统设置 → **按需审批** → **保存**
3. Composer 应显示 **「审批：按需审批」**（无复选框）

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 2.1 | **Agent** + 设置 **按需审批** 已保存 | Composer 显示「审批：按需审批」（无自动批准复选框） | [x] |
| 2.2 | 发 prompt 触发**写/执行类工具**（如「用 write_file 在工作区创建 `_approval_test.txt`，内容 ok」） | 出现 **ApprovalDialog**（非静默直接执行） | [x] |
| 2.3 | 点 **批准** | 工具继续；turn 完成；文件或工具结果可见 | [x] |
| 2.4 | 再开一轮，同样触发审批，点 **拒绝** | turn 结束或明确报错；未批准时不执行 | [x] |

> 若模型未调用工具：换更直接的 write/shell prompt。  
> 测 `never` 策略：TUI/Plan 模式更可靠；桌面当前 **不会** 把 settings 的 `never` 接到 `auto_approve`。

---

## 3. PR5 — 双窗并行 Turn（多窗口）

**目标：** 对齐 [multi-window-plan.md](../../desktop/multi-window-plan.md) **T2** + PR5 `sidecar_parallel_turns_on_two_threads`。

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 3.1 | **新建窗口**（TitleBar / 菜单 / Ctrl+Shift+N） | 出现第二个独立窗口，标题/工作区可区分 | [x] |
| 3.2 | **窗 A**：绑定工作区 A（或默认），开 **会话 A**，发一条**较长** prompt（如「逐步分析 README 前三段」） | A 开始 streaming | [x] |
| 3.3 | **不中断 A**，切到 **窗 B**，另一工作区/会话 B，发一条短 prompt | B **能正常**收流式回复；A **仍在跑**或正常结束（B 发送时 A 未被掐断） | [x] |
| 3.4 | 两窗都等 turn 结束 | 两窗各自会话状态正确；无串台（A 的正文出现在 B） | [x] |
| 3.5 | **窗 A** 开内置终端，执行 `echo window-a` | **仅 A** 终端有输出（回归 **T4**） | [x] |

---

## 4. PR5 — 审批仅 owner 窗（可选，~5 min）

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 4.1 | 双窗；**窗 A** 关 auto_approve，触发审批；焦点在 **窗 B** | **B 不出现** 挡屏审批模态（或仅有非阻塞提示） | [ ] |
| 4.2 | 焦点回到 **窗 A** | A 弹出 `ApprovalDialog`，可批准/拒绝 | [ ] |

---

## 5. Sidecar / Stop 一致性（可选）

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 5.1 | 长 turn 中途点 **Stop** | turn 变为 interrupted/canceled；可再发消息 | [x] |
| 5.2 | 托盘/任务管理器：仅 **一个** `deepseek-tui` sidecar（或等价子进程） | 无第二个 7878 监听 | [ ] |

---

## 6. 签收区（手测完成后填写）

| 项 | 结果 |
|----|------|
| 测试人 | 维护者 |
| 日期 | 2026-05-24 |
| 构建 | `cargo tauri dev` |
| Git | `9ee2c34` 及之后 |
| **§0.4 health / event_schema_version** | ✅ 通过 |
| **G2 §12.2 #3 单窗全链路** | ✅ 通过（§1） |
| **G2 A+.7 审批 UI** | ✅ 通过（2026-05-24，§2 + §9.2.1） |
| **PR5 双窗并行 turn（§3）** | ✅ 通过 |
| **§5.1 Stop** | ✅ 通过 |
| **F3 §8 键盘 a11y（8.1–8.5）** | ✅ 通过（2026-05-24） |
| **§12.4 #2（Stop + 长跑 + 审批）** | ✅ 通过（2026-05-24，§9 全项） |
| **B-L1 CRAFT（§10 全项）** | ✅ 通过（2026-05-24） |
| **备注** | §12.4 #2 已闭合；审批 UX：设置四档 + Composer 只读/可勾；**B-L1 CRAFT 手测全量通过**（2026-05-24） |

**通过后建议：**

1. 在 [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md) 将「桌面全链路冒烟」「DS Pick 多窗口手测」改为 ✅，并贴上表 §6 一行摘要。  
2. 维护者单独处理 **G3**（§11.0 ADR 签收）与 **§12.3 #1**（`Engine` 留 tui vs 继续迁 core）— 与手测独立。

---

## 8. F3 — 键盘 a11y（~5 min，非 G2 阻塞）

**目标：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §10.3 F3 — Tab 顺序、skip link、Escape 停生成、focus-visible。

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 8.1 | **Tab** 从页面顶部开始，前两个可聚焦项为 skip link（「跳到主要内容」「跳到输入框」） | 顺序在侧栏/Composer 控件之前 | [x] |
| 8.2 | 激活 **「跳到输入框」** skip link | 焦点落到 Composer `#composer-input`；可立即输入 | [x] |
| 8.3 | Composer 内 **Tab**：textarea → actions toolbar（发送/停止）→ options toolbar（工作区/模型等） | DOM 顺序与视觉一致（options 在 textarea 下方但 Tab 在 actions 之后） | [x] |
| 8.4 | 长 turn 进行中，焦点在**非** input/textarea 处按 **Escape** | 生成停止；可再发消息 | [x] |
| 8.5 | 键盘 **Tab/Shift+Tab** 到侧栏、Composer 芯片、发送按钮 | 可见 **focus ring**（accent 色 outline），非仅鼠标点击态 | [x] |

> 自动化：`npm run build` + `npm run test:f3`（web-ui）；Rust 侧契约测含 interrupt。`prefers-reduced-motion` 可在 OS 设置开启后确认侧栏/Composer 动画减弱（可选）。

---

## 9. §12.4 #2 — TUI vs DS Pick 同 thread 行为抽样

**目标：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §12.4 #2 — 同一 `deepseek-tui` runtime 上，终端 TUI 与桌面 DS Pick 对 **同一 thread** 的 stop / 审批 / 长跑语义一致（抽样，非全矩阵）。

### 9.1 Stop / Interrupt — ✅ 已签（2026-05-24）

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 9.1.1 | **DS Pick**：长 turn 中途点 **Stop** | `interrupted`；可再发消息 | [x] |
| 9.1.2 | **DS Pick**：长 turn 中焦点在非输入区按 **Escape** | 同 §8.4；生成停止 | [x] |
| 9.1.3 | **同一 thread** 在 **TUI** 中长 turn 按 **Esc** 停止 | 同样 `interrupted`；可继续对话 | [x] |
| 9.1.4 | 自动化：`sidecar_contract_full_lifecycle` 使用 `POST .../interrupt` | CI 绿 | [x] |

> Stop 维度 **§12.4 #2 已闭合**（2026-05-24）。

### 9.2 审批 — ✅ 已签（2026-05-24）

**桌面：** §2（按需审批 → ApprovalDialog → 批准 / 拒绝各一轮）。

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 9.2.1 | **DS Pick** §2.2–2.4 批准 + 拒绝 | ApprovalDialog；行为符合 A+.7 | [x] |
| 9.2.2 | **同一 thread** 在 **TUI** 关自动批准，同样 prompt（可选） | 挂起 → resolve 语义与桌面一致 | ⏸ 未测 | §12.4 抽样以 DS Pick 为准已闭合 |

### 9.3 长跑 — ✅ 双壳已签（2026-05-24）

**代表性抽样（DS Pick，2026-05-24）：** 对仓库发起 **全量审核** turn — `agent_spawn` 并行 Explore 子代理、审计 scratchpad（`.deepseek/scratchpad/2026-05-24-audit`）、checklist P0–P4、多轮 tool + 长上下文。比 R-015 脚本更贴近真实 **多 turn / 多工具 / 子代理** 生产负载；**auto_approve 开启** 属长跑场景（与 §9.2 审批手测分开）。

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 9.3.1 | 多轮 turn + 重负载（本审核 **或** R-015 `-Gate`） | 无 sidecar 崩溃/卡死；RSS/落盘无异常劣化 | [x] |
| 9.3.2 | **DS Pick** 上述全量审核 **跑完**（或主动 Stop 后 thread 仍可用） | scratchpad/checklist 有进展；可再发消息；连接正常 | [x] |
| 9.3.3 | 抽样：**TUI** 同工作区类似多轮（不必复刻 29 区，数轮 tool 即可） | 与桌面侧无「一边崩一边正常」分裂 | [x] |

**已签收抽样（2026-05-24）：**
- **DS Pick：** 全量审核 — 30 区、8 Explore 子代理、scratchpad `.deepseek/scratchpad/2026-05-24-audit` → [deliverables/CODE_REVIEW_2026-05-24.md](../../../deliverables/CODE_REVIEW_2026-05-24.md)
- **TUI：** 同工作区 `F:\DeepSeek-TUI-desktop` 多轮 tool（list_dir / read_file / glob 等）；turn 正常、无 sidecar 分裂

**审核结束时勾选 9.3.1–9.3.2 条件：** P4 报告产出或 checklist 收尾；7878 仍健康；同 thread 能继续对话。若中途 Stop，须确认 §9.1 仍成立且 thread 可恢复。

```powershell
# 可选：R-015 数值门控（与审核抽样互补，release 推荐）
cd F:\DeepSeek-TUI-desktop
.\scripts\runtime-longrun-baseline.ps1 -Gate
```

---

## 10. B-L1 / CRAFT 手测（§12.5 #1 抽样）

**目标：** [RUNTIME_EVOLUTION_ROADMAP.md](../RUNTIME_EVOLUTION_ROADMAP.md) §9.1 B1 — 角色工具、黑板、fix-loop 提示、`craft.*` SSE、AgentPanel 对接；对应 [craft-implementation-issues.md](../../craft-implementation-issues.md) Issue 0–5。

**前置：** `cargo tauri dev`；sidecar `7878` 健康；工作区含可写文件；模型可 spawn 子代理并调用工具。

| # | 步骤 | 预期 | 结果 |
|---|------|------|------|
| 10.1 | `GET /v1/blackboards` | 200；列表含当前 workspace 下 task（或空数组） | ✅ |
| 10.2 | 触发 CRAFT 任务后轮询 `GET /v1/blackboards/{id}` | JSON 含 findings / observed / blockers / verdict 字段 | ✅ |
| 10.3 | explore 子代理尝试写盘工具 | 被拒绝或不可用（角色白名单） | ✅ |
| 10.4 | reviewer 子代理尝试写盘工具 | 同上 | ✅ |
| 10.5 | Verifier 输出 BLOCKER/FAIL | 黑板写入 blockers；主线程可见 `<deepseek:craft.fix_loop>` 提示 | ✅ |
| 10.6 | SSE 订阅 | 收到 `craft.verdict` / `craft.board_updated`（或 compat 映射） | ✅ |
| 10.7 | DS Pick **AgentPanel → CRAFT 任务** | 展示 task 列表/详情；SSE 或轮询刷新 | ✅ |
| 10.8 | **一轮闭环**（explorer → 实现 → review → verify → fix 提示） | 黑板状态随轮次更新；fix-loop 可继续下一轮 | ✅ |

> **签收：** 维护者 **2026-05-24** 全项通过。§12.5 **#1**（CRAFT + 黑板 + 一轮闭环）可标 ✅；**#2 记忆地图**、**#3 GAP 表** 仍属 B 阶段余项。

---

## 7. 失败时快速采集

```powershell
# 健康与 schema
curl -s http://127.0.0.1:7878/health

# 最近 runtime 日志目录（视本机配置）
# %USERPROFILE%\.deepseek\ 或 DEEPSEEK_RUNTIME_DIR 所指路径

# 桌面/Rust 日志：tauri dev 终端输出截屏即可
```

常见问题：

| 现象 | 先查 |
|------|------|
| 7878 连接失败 | sidecar 是否被防火墙拦；是否多开实例抢端口 |
| 审批从不弹出 | 是否仍开 YOLO/auto_approve；模型是否未调用工具 |
| B 窗抢 A 的流 | 回归 M2/M3；是否用了纯 `npm run dev` 无 Tauri |
| debug sidecar stack overflow | 手测可用 dev 桌面；**长跑基线**用 release |
