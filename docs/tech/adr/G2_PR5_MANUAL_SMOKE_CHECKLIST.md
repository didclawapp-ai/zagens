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

**两套控制（桌面手测必看）：**

| 层级 | 字段 | 作用 |
|------|------|------|
| 设置 → 审批策略 | `approval_policy` → `config.toml` | 主要驱动 **TUI** 与 **系统提示词**；桌面 **start turn 时未读取** |
| Composer →「自动批准工具调用」 | HTTP `auto_approve` | 驱动 **runtime 是否立刻 approve**；默认 **勾选 = true** |
| `turn_lifecycle`（sidecar） | `auto_approve: false` 时 | 固定 `ApprovalMode::Suggest`，**不**读 config 的 `never` |

因此：设置选「仅不受信任」+ config 已保存，**若 Composer 仍勾选自动批准**，模型会看到 **Auto 提示词 + 运行时自动放行** —— 这是 **接线缺口**，不是手测操作错误。

**手测 §2 有效操作：**

1. 运行模式 **Agent**（非 YOLO）
2. **取消勾选** Composer「自动批准工具调用」
3. （可选）设置改为「按需审批」并确认 `config.toml` 为 `on-request` —— 影响提示词语义，**不能代替** 第 2 步

| # | 步骤 | 期望 | 通过 |
|---|------|------|------|
| 2.1 | **Agent 模式** + **取消勾选** Composer「自动批准工具调用」（勿 YOLO） | 输入栏上方复选框为 **未勾选** | [ ] |
| 2.2 | 发 prompt 触发**写/执行类工具**（如「用 write_file 在工作区创建 `_approval_test.txt`，内容 ok」） | 出现 **ApprovalDialog**（非静默直接执行） | [ ] |
| 2.3 | 点 **批准** | 工具继续；turn 完成；文件或工具结果可见 | [ ] |
| 2.4 | 再开一轮，同样触发审批，点 **拒绝** | turn 结束或明确报错；未批准时不执行 | [ ] |

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
| 日期 | 2026-05-23 |
| 构建 | `cargo tauri dev` |
| Git | `e1c4841` 及之后 |
| **§0.4 health / event_schema_version** | ✅ 通过 |
| **G2 §12.2 #3 单窗全链路** | ✅ 通过（§1） |
| **G2 A+.7 审批 UI** | ⏸ 暂缓（Auto 已验证；弹窗待接线后复测） |
| **PR5 双窗并行 turn（§3）** | ✅ 通过 |
| **§5.1 Stop** | ✅ 通过 |
| **备注** | `approval_policy` ↔ `auto_approve` 产品债另开修复 |

**通过后建议：**

1. 在 [G2_GATE_ACCEPTANCE.md](./G2_GATE_ACCEPTANCE.md) 将「桌面全链路冒烟」「DS Pick 多窗口手测」改为 ✅，并贴上表 §6 一行摘要。  
2. 维护者单独处理 **G3**（§11.0 ADR 签收）与 **§12.3 #1**（`Engine` 留 tui vs 继续迁 core）— 与手测独立。

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
