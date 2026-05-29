# 交接：LHT 强制续写「进展放行」漏洞修复 + DEMO2 复测

**日期：** 2026-05-29
**涉及版本：** `deepseek-runtime-server` 0.8.15
**一句话：** LHT 强制续写在编码任务里没拉回早停的模型，根因已用诊断事件精确定位并修复，**待重新打包 + 复测验证**。

---

## 1. 背景 / 问题

在 `F:\DEMO2`（mini-jq Python 项目，提示词刻意诱导认知早停）测试时，模型「写了点代码 → prose 收尾 → 会话结束」，清单停在 0%，LHT 长程续写**没有把它拉回**。这正是 LHT 要解决的「认知早停」，却失效了。

之前在 `F:\DEMO1`（Electron→Tauri）也观察到类似现象，但那次是 `Interrupted`（进程重启打断），不算 LHT 失效。DEMO2 是 `Completed`（模型主动停），是真正的 LHT 漏网。

## 2. 定位过程（已完成）

1. **免费 DB 复核排除了两条假设：**
   - 清单快照完好（`in_progress_id` 正确、项数齐、非空）→ 不是快照/数据问题。
   - 引擎配置与面板同源（`engine_spawn.rs:87` 与 task_graph 都走 `self.config.long_horizon_config()`），面板 `lht_enabled=true` → 引擎侧 `enabled` 也必为 true。
2. **加诊断事件**（已合入）：门禁 `maybe_continue_incomplete_code_task` 改为返回 `LhtGateOutcome::{Nudge, Skip(reason)}`，`no_tool_uses.rs` 在跳过时发 `long_horizon.gate_skip` 状态事件，带 `reason/enabled/app_mode/code_surface/empty/incomplete/trivial/in_progress_id/open_items`。
3. **重打包跑 DEMO2 → 一发命中。** 线程 `thr_0eda7dcc`、turn `turn_4e1cff9a`（`Completed`）：

   ```
   long_horizon.gate_skip: {"reason":"nudge_skip_progress_reset","enabled":true,
   "app_mode":"Agent","code_surface":true,"empty":false,"incomplete":true,
   "in_progress_id":1,"open_items":3}
   ```

   门禁全部前置 guard 通过，栽在 `prepare_nudge` 返回 `SkipProgressReset`。

## 3. 根因（设计漏洞，非 bug）

`prepare_nudge` 里 `had_progress=true` 会直接 `return SkipProgressReset`（不 nudge）。但 gate **只在「模型不调工具、prose 收尾、任务未完成」时触发**，而「先干了点活、再中途撒手」这一轮 `had_progress` 几乎必然为真——于是模型恰好在它早停的那一轮被放行。`had_progress` 把「有进展」和「不需要催」错误地划了等号。

## 4. 修复（已完成，编译 + 20 测试全过）

**语义收窄：进展只清零 `no_progress_streak`（防误判 `blocked`），不再跳过 nudge。**

- `crates/runtime-server/src/long_horizon/nudge.rs` `prepare_nudge`：
  - `had_progress` 分支只 `no_progress_streak.remove(&id)`，不再 return。
  - 落到硬上限检查 + 发 nudge；`max_nudges_per_item`（默认 5）兜底防无限催。
  - streak 累加（趋近 `blocked`）只在 `!had_progress` 时进行——正在干活的模型永不被误判放弃。
  - 删除已失效的 `NudgeDecision::SkipProgressReset` 变体。
- `crates/runtime-server/src/long_horizon/mod.rs`：移除 gate 里对应的 match 分支。
- 测试：旧 `max_cap_reached_despite_intermittent_progress`（写死旧语义）→ 换成 `progress_nudges_but_never_blocks` + `progress_resets_no_progress_streak`。
- `CHANGELOG.md`：记了修复，带 `thr_0eda7dcc` 实证。
- `docs/harness/LONG_HORIZON_CODE_TASKS.md` §4.3.1：补「`had_progress` 作用边界」一段。

## 5. 当前状态

- ✅ 代码改完，`cargo test -p deepseek-runtime-server long_horizon` = 20 passed。
- ✅ 诊断事件 `long_horizon.gate_skip` 保留（下次复测仍可观测）。
- ⏳ **未重新打包**（上一版打包是带诊断、未带本修复的二进制）。

## 6. 下一步（待办）

1. **全量打包**（覆盖旧 sidecar）。
2. **重跑 DEMO2 同款任务**（mini-jq 或同类诱导早停的提示词）。
3. **查事件库验证**（`C:\Users\Administrator\.zagens\tasks\runtime\runtime.db`，表 `events`，列 `event` / `payload_json`）：
   - 期望看到 `long_horizon.continue_injected`（成功续写、把模型拉回）。
   - 若仍 `gate_skip`，看新的 `reason` 定位下一个卡点。
   - 用 `node` + `node:sqlite` 的 `DatabaseSync` 只读查询即可（注意临时脚本用完即删）。

## 7. 关键文件速查

| 文件 | 作用 |
|------|------|
| `crates/runtime-server/src/long_horizon/nudge.rs` | `NudgeTracker::prepare_nudge`（本次核心改动） |
| `crates/runtime-server/src/long_horizon/mod.rs` | `maybe_continue_incomplete_code_task` 门禁 + `LhtGateOutcome` |
| `crates/runtime-server/src/core/engine/turn_loop/host_impl/no_tool_uses.rs` | gate 调用方 + `gate_skip`/`continue_injected` 事件发射 |
| `docs/harness/LONG_HORIZON_CODE_TASKS.md` | LHT 设计文档（§4.3 NudgeTracker、§4.3.1 qualified progress） |
| `C:\Users\Administrator\.zagens\config.toml` | `[long_horizon] enabled=true` 已配置 |

## 8. 复测时直接发给新会话的摘要

> LHT「进展放行」漏洞已修（`nudge.rs` `prepare_nudge`：进展只清 streak 不跳 nudge，删了 `SkipProgressReset`），编译+测试过、CHANGELOG/文档已记。诊断事件 `long_horizon.gate_skip` 保留。待办=全量打包 → 重跑 DEMO2 → 查 `runtime.db` 的 `events` 表确认 `long_horizon.continue_injected` 是否发出、模型是否被拉回。上次实证线程 `thr_0eda7dcc`。
