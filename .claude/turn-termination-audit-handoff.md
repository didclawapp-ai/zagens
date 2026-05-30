# 交接：LHT turn 终止出口审计 & 静默早停修复

**日期：** 2026-05-30
**前序上下文：** `.claude/stream-truncation-investigation-handoff.md`（流截断调查）→ 本文是其后续。
**主线：** 桌面端长程任务（LHT）"任务没干完却标 `Completed`"（假绿/静默早停）的系统性收口。

---

## 0. 背景一句话

`run.rs` 外层 turn loop 里**所有 `break` 最终都汇到同一个 `Completed` 落点**（除非 `turn_error` 置位走 `Failed`）。
判定标准：**任何「绕过 no-tool-uses LHT 续写闸门就 break、且没置 `turn_error`」的出口 = 假绿**。
LHT 续写闸门（`maybe_inject_incomplete_lht_continue`）**只挂在 no-tool-uses 这一条路径**上。

已识别的静默早停形态（同一类问题的不同出口）：
1. length 截断（finish_reason=length，无 tool call）— 已修（`MAX_LENGTH_CONTINUATIONS=8`）
2. prose 早停（progress-pass 漏放）— 已修
3. **step 耗尽**（打满 `max_steps=100`）— 已修（DEMO4，`MAX_STEP_LIMIT_CONTINUATIONS=3`）
4. **loop_guard 停机**（同工具连失 8 次 Halt）— ✅ **本次修复**

---

## 1. 本次已完成（已编译 + 测试通过 + 无 lint）

### G — loop_guard 停机型早停（第四种）
同一工具连续失败 `FAILURE_HALT_THRESHOLD=8` 次 → `LoopGuard::Halt` → `tool_phase` 直接 `break` → 原本绕过 LHT 闸门标 `Completed`。

修复链路：
- `tool_phase` outcome 新增 `loop_guard_halted: bool`（区分 plan-stop vs loop-guard-halt）
- `run.rs` 在 `phase.break_outer_loop` 分支：若 `loop_guard_halted && !plan && 续写次数 < 上限 && host.maybe_continue_after_loop_guard_halt(turn)` →
  - `loop_guard.reset_failures()`（清每工具失败计数，**identical-call 阻断保留**）
  - 注入「换方法/换工具/改参数/先读错误定位根因，别停」nudge（中英双语）
  - `turn.next_step(); continue;`（不 break）
- 上限 `MAX_LOOP_GUARD_CONTINUATIONS=2`
- 发 `Loop-guard halt; nudging long-horizon task to change approach (n/N)` 状态 + `long_horizon.loop_guard_continue` 事件
- plan 模式 / 非 LHT host：默认钩子返回 `false`，维持原 halt

### I — 放弃出口可观测（防御层）
根因：所有 break 都长成干净的 `Completed`，不区分真完成 vs 放弃。
- `run.rs` 最终 `Completed` 落点前新增 `host.note_incomplete_stop_if_lht().await`
- LHT 图仍 incomplete 时发 `long_horizon.incomplete_stop: {open_items:n}` 探针
- 经 `monitor.rs` 的 `[lht-probe]` 中央 tee 自动落 `sidecar.log`（两条新状态都以 `long_horizon.` 开头，**无需额外接线**）
- 纯观测，**不改 outcome 类型**

---

## 2. 改动文件清单

| 文件 | 改动 |
|------|------|
| `crates/core/src/engine/loop_guard.rs` | `+ pub fn reset_failures()` + 2 个单测 |
| `crates/core/src/engine/streaming.rs` | `+ MAX_LOOP_GUARD_CONTINUATIONS: u32 = 2` |
| `crates/core/src/engine/turn_loop/control.rs` | `TurnLoopToolPhaseOutcome + loop_guard_halted` |
| `crates/core/src/engine/turn_loop/tool_phase.rs` | 填 `loop_guard_halted: loop_guard_halt.is_some()` |
| `crates/core/src/engine/turn_loop/host.rs` | `+ maybe_continue_after_loop_guard_halt`（默认 false）、`+ note_incomplete_stop_if_lht`（默认 no-op） |
| `crates/core/src/engine/turn_loop/run.rs` | 导入常量 + `loop_guard_continuations` 计数 + break 分支续写 + 最终守卫 |
| `crates/runtime-server/src/core/engine/turn_loop/host_impl/mod.rs` | 实现两个钩子（紧接 `maybe_continue_at_step_limit`，~490 行后） |
| `CHANGELOG.md` | `[Unreleased] > Runtime` 顶部新条目 |
| `docs/harness/LONG_HORIZON_CODE_TASKS.md` | 审计表 + 第四种早停（~512 行）+ §4.6 两条 |

## 3. 验证状态

```
cargo check -p deepseek-core -p deepseek-runtime-server   # exit 0
cargo test  -p deepseek-core loop_guard                   # 7 passed
```
- ReadLints 5 个改动文件：无错误
- ⚠️ 已知坑：`cargo clippy` 会因 **无关** crate `topic-memory/src/extract.rs` 的正则语法（"unclosed character class"）报 exit 101——**不是本次改动引入**，验证请用 `cargo check`/`cargo test` 直跑，绕开 clippy。

## 4. 还没验证的（建议新会话做）
- **未做实跑复现**：G 的修复尚未用真实"卡循环"场景压测过。可构造一个让某工具必然连失 ≥8 次的 LHT 任务，看 `sidecar.log` 是否出现 `long_horizon.loop_guard_continue` 而非直接 Completed。
- sidecar 二进制需**重新构建并部署**到桌面端实际加载的位置（历史教训：改了不重建，跑的是旧二进制 → 看不到新行为/新探针）。部署后 grep `sidecar.log` 的 `[lht-probe]`。

---

## 5. 待办 backlog（C — 改动大，本次故意没做）

**context 溢出对长任务是硬失败，缺 LHT 感知的 cycle/交接。**
- 位置：`run.rs` ~134-145，`context_recovery_attempts >= MAX_CONTEXT_RECOVERY_ATTEMPTS` → `return (Failed, "请运行 /compact 或 /clear")`
- 问题：2W 行级长任务上下文涨过预算、自动恢复 N 次仍压不下去 → 直接 `Failed` 甩锅给用户手动 `/compact`
- 应有行为：触发 **LHT cycle/上下文交接**（摘要后同 thread 换脑继续，复用现有 cycle/`<carry_forward>`/`handoff.md` 机制），而不是硬打断
- 性质：这条会**上抛（非假绿）**，所以优先级低于 G/I，但对大任务体验是硬伤
- 建议：作为独立任务排期；可考虑落到 `docs/agent-reliability-craft-plan.md` §11.5「金矿」backlog
- 注意：现有 `run_capacity_pre_request_checkpoint`（run.rs ~123）/ `recover_context_overflow` / `run_auto_compaction` 已有上下文压缩，需先确认 cycle 是否本应在此之前触发、为何没拦住，再动手

## 6. 审计完整结论表（出口 × 判定）

| 出口 | 终止类型 | 判定 |
|------|---------|------|
| 顶部 cancel | Interrupted | ✅ 合理 |
| `at_max_steps` | 续写×3→Completed | ✅ 已修 |
| context 溢出耗尽 | Failed(/compact) | ⚠️ backlog C |
| 流内 duration/overflow/stream-error 耗尽 | Failed(turn_error) | ✅ 合理 |
| chunk_timeout 思维链空闲 | 走 no-tool-uses→LHT 续写 | ✅ 已兜住 |
| `stop_after_plan_tool` | break→Completed | ✅ 仅 plan 模式，非缺口 |
| **loop_guard 停机** | break→Completed | ✅ **本次修复** |
| LHT 闸门 Skip（nudge_max_reached/blocked） | break→Completed | ⚠️ 设计内放弃终态 → I 探针已可观测 |

## 7. 关键代码锚点（新会话快速定位）
- LHT 续写闸门主体：`runtime-server/.../host_impl/no_tool_uses.rs:446`（`maybe_inject_incomplete_lht_continue`）
- LHT gate 决策：`runtime-server/src/long_horizon/mod.rs:166`（`prepare_nudge` → Skip/Nudge/MaxReached/Blocked）
- `[lht-probe]` 中央 tee：`runtime-orchestrator/src/runtime_threads/monitor.rs`（`message.starts_with("long_horizon.")`）
- 两个新钩子实现：`runtime-server/.../host_impl/mod.rs`（`maybe_continue_at_step_limit` ~490 行起，紧随其后）
