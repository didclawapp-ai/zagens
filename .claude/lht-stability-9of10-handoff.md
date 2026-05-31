# 交接：LHT 稳定性死磕 → 目标 90%（10 次 ≥9 成）

**日期：** 2026-05-30
**涉及版本：** `deepseek-runtime-server` 0.8.15 / Zagens 桌面（装于 `C:\Users\Administrator\AppData\Local\Zagens\`）
**一句话：** LHT 端到端通过率实测 ~62.5%（8 次挂 3 次），**失败签名是单一根因 = DEMO3 假绿**；治本改动（B 软门禁 + A UI）已写完、已提交、已全量打包重装，**DEMO3 连跑 5 次验证完成 = 5/5 真绿、B 全员上场、旧假绿出口归零 → B 治本成功，收尾结束**（详见 §3）。下一步测 CodeCrafters Redis。

---

## 0. 目标

LHT 长程任务端到端**稳定到 90%**——同一 DEMO 跑 10 次至少 9 次成功。判定**只能靠客观 oracle**，不看模型 prose（DeepSeek V4 思考模式无 `temperature`/`seed`，非确定性压不掉，见 `docs/harness/LHT_TEST_SUITE.md` §5.1）。

## 1. 关键诊断（已用日志坐实）

离线复盘 `C:\Users\Administrator\.zagens\logs\sidecar.log`，最近这批失败**100% 是同一签名**：

```
[lht-probe] verify_gate ... verdict=unverified_acceptance   ← 可运行验收漏标 [verify:]
[lht-probe] long_horizon.gate_skip: {"reason":"graph_complete",...,"open_items":0}  ← 直接放行收尾
```

- 出事的项如 `"go build / go vet / gofmt 全绿 + 全部 8 个示例跑通"`、`"验证: go build + vet + gofmt + go test + run_examples 全绿"`——是可运行验收却**没带 `[verify:]` 前缀** → 判 `unverified_acceptance` → 旧逻辑只追加软提示、不阻断 → `graph_complete` 收尾 = **DEMO3 假绿**。
- 涉及 thread：`thr_70f9664a`（item 11/13）、`thr_61d5bfa7`（item 12/14）。
- **没有**任何其它失败出口：`incomplete_stop` / `blocked` / `Failed` / `step_limit_continue` / `loop_guard` 一个都没出现。失败高度集中 = 好打。
- 这批日志**无 `unverified_acceptance_nudge` 事件** → 当时跑的是旧二进制，B 尚未生效。

## 2. 本会话已完成（commit `452a259`，11 文件 +196/-13）

**B（根治 · runtime）—— `unverified_acceptance` 软提示 → 软门禁：**
- `crates/runtime-server/src/long_horizon/mod.rs`：续写 gate `maybe_continue_incomplete_code_task` 在 `!graph.incomplete()`（graph_complete）处**旁路加检查**：扫已完成 checklist，若仍有「读起来像可运行验收、却既无 `[verify:]` 又无匹配近期执行」的项（复用 `verify::verify_gate_verdict(...).0 == "unverified_acceptance"`），则**不放行**，返回新变体 `LhtGateOutcome::NudgeUnverifiedAcceptance`，注入聚焦续写。
- **刻意不改** `completion_pct` / `graph.incomplete()`——进度条仍 100%，不回退 DEMO5 #1。
- `crates/runtime-server/src/long_horizon/nudge.rs`：新增 `unverified_acceptance_nudges: u32`（session 级、跨用户消息持久）、常量 `MAX_UNVERIFIED_ACCEPTANCE_NUDGES=2`（防模型死活不加 `[verify:]` 空转）、双语文案 `build_unverified_acceptance_nudge()`（+单测）。
- `crates/runtime-server/src/core/engine/turn_loop/host_impl/no_tool_uses.rs`：处理新变体——注入续写 + 发独立事件 `long_horizon.unverified_acceptance_nudge: {"count":n}`（不混进常规 `continue_injected` 遥测）；自动入 Nodes ring（manager.rs 通用记录，无需改）。

**A（UI · desktop）—— plan display-only 大纲淡化：**
- `crates/desktop/web-ui/src/components/LongHorizonPanel.tsx`：checklist 为完成权威（非空）且任务 100% 时，plan 仍 `pending` 的阶段是展示用大纲——Plan 标题加注记 + 这些阶段**淡化/删除线**，消解「进度 100% 但清单看着没关闭」错位；Nodes Tab 给 `unverified_acceptance_nudge` 配橙色（与 verify mismatch 同色系）。
- `crates/desktop/web-ui/src/i18n/locales/{en,zh-Hans,ja,pt-BR}.ts`：新 key `longHorizon.planOutlineNote`。

**docs：** `CHANGELOG.md`（Runtime + Desktop 两条）、`docs/harness/test-cases/DEMO3-monkey-interpreter.md` §6（复跑结论：真绿但闭环未阻断 → 已根治）、`docs/harness/LHT_TEST_SUITE.md`。

**自验：** `cargo check -p deepseek-runtime-server` ✅；`long_horizon` 模块 27 单测全过；web-ui `npm run build`（tsc+vite）✅。
> 注：`cargo clippy` 全量会在**无关 crate** `deepseek-topic-memory` 报错（一处 `Regex::new` 字面量 "unclosed character class"，clippy `invalid_regex` lint）——预先存在、与本次无关，本次改动文件 clippy 干净。可顺手修。

## 3. 当前状态（接手时确认）

- ✅ 改动已提交 `452a259`，**未 push**。工作树 clean（文档固化为后续提交）。
- ✅ 用户已**全量打包 + 重装**，新二进制（含 B）已生效。
- ✅ **DEMO3 连跑 5 次验证完成（2026-05-30）：5/5 真绿，B 治本成功，收尾结束。** 客观核验（产物 `F:\DEMO3\1..5` + `sidecar.log`，不看 prose）：
  - 产物 `go build/vet/test` 全 exit 0；两个钓鱼特性（`10 % 3 == 1`、`counter1` 数字标识符不截断）**5/5 全过**。
  - 日志：`long_horizon.unverified_acceptance_nudge` 每 run 各 1 次（共 5，旧批 0）；旧假绿出口 `graph_complete`/`gate_skip` **归零**；`verify_gate verdict=verified` 16 条。B 因果链坐实（漏标→nudge→补 `[verify:]` 真跑→verified）。
  - 对比基线 ~62.5% 显著提升。详见 `docs/harness/test-cases/DEMO3-monkey-interpreter.md` §7。
- ⏳ **下一步（用户已确认）：测 CodeCrafters Redis**（见 `docs/harness/test-cases/codecrafters-redis.md`），结果出来后再综合判断。
- 🔧 **残留洞（下一锤候选，非阻断）：** `mismatch` 4 次 = 模型贴 `[verify:]` 但匹配器未关联到执行；B 只阻断 `unverified_acceptance` 不阻断 `mismatch` → 理论上"只贴标签不真跑"可降级逃逸（本次未触发，命令均真跑）。治法：B 对 mismatch 也做克制阻断，或放宽匹配器对复合命令的关联（呼应 §5 第 2 条预判）。

## 4. 怎么判定一次跑成功（客观，别看 prose）

1. **B 是否上场**：Nodes Tab 出现橙色 `unverified_acceptance_nudge` / 日志有 `long_horizon.unverified_acceptance_nudge`。若关键验收漏标 `[verify:]` 却**没看到此事件、直接 `graph_complete` 收尾** → 二进制没换成功，先停。
2. **终态链路 grep**：
   ```powershell
   Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern 'unverified_acceptance|graph_complete|verify_gate|continue_injected'
   ```
   理想：`unverified_acceptance` → `unverified_acceptance_nudge` → 模型补 `[verify:]` 真跑 → `verify_gate verdict=verified` → 干净收尾。
3. **最终裁判 = 产物实跑**（DEMO3 命门，`go.exe` 在 `C:\Program Files\Go\bin\go.exe`，环境无 bash）：
   ```powershell
   # 产物根目录
   go build ./...; go vet ./...; go test ./...
   # 两个钓鱼特性：取模 10 % 3 == 1；带数字标识符 counter1 不被词法器截断
   ```

## 5. 下一锤候选（按 5 次结果定）

- **若 B 把通过率显著拉高（≥9/10）** → 验证完成，把这条编入回归集，攻 DEMO6 暴露的**语义阈值边界**（`go test -cover ≥80%` exit 0 但阈值没达 = 闸门拦不住）。
- **若模型躲过 B**（两种预判，出现即印证）：
  1. **关键词绕过**：B 检测靠 `EN/ZH_ACCEPTANCE_HINTS` 关键词（`verify.rs`）。模型改写措辞（"完成示例"替"跑通示例"）又不加 `[verify:]` → 检测不到。更稳的检测：「整任务 `recent_verification_cmds` 空却标完成」这类无关键词信号。
  2. **续写 2 次不听话**（`MAX_UNVERIFIED_ACCEPTANCE_NUDGES=2`）→ 第 3 次仍放行。治本是让模型**一开始就写 `[verify:]`**：`crates/runtime-server/src/prompts/base.md` 的 checklist 纪律（DEMO3 已加教学但显然不够），可加强；以及 backlog ②「`目标↔实现↔验证` 可追溯矩阵」（见 `docs/harness/LONG_HORIZON_CODE_TASKS.md` 第 24 行 0.8+ backlog）。
- **测量基建缺口**（要冲 10 次/N 次统计迟早要补）：目前无 headless 批量跑 + 自动判 pass/fail。可考虑 Cursor SDK 或脚本驱动；2W 行 Monkey 一跑 ~45 分钟，10 次 ~7.5h，需要更快的 proxy 任务。

## 6. 环境 / 踩坑速记

- **部署方式**：装好的桌面应用，runtime 二进制 = `AppData\Local\Zagens\zagens-runtime.exe`（`find_runtime_binary` 优先取桌面 exe 旁的 `zagens-runtime-<target>.exe`）。改 runtime 须 `cargo build --release --bin zagens-runtime` 后替换 + 重启 app，或像这次**全量打包重装**。
- **PowerShell**：无 heredoc。多行 commit message 用「写文件 + `git commit -F`」（这次用 `.git/ZAGENS_COMMIT_MSG.txt` 临时文件，提交后删）。
- **日志**：sidecar 无 `tracing` subscriber，stderr → `~/.zagens/logs/sidecar.log` 是唯一 sink；`[lht-probe]`/`[stream-probe]`/`[thinking-probe]` 都在这。
- **非确定性**：DeepSeek V4 思考模式静默忽略 `temperature`/`top_p`，无 `seed`——复现只能靠客观 oracle 判终态，不能靠输出比对。

## 7. 相关文档

- `docs/harness/LONG_HORIZON_CODE_TASKS.md`（闸门定义、DEMO 实证、turn 终止出口审计、0.8+ backlog）
- `docs/harness/LHT_TEST_SUITE.md`（测试集、§5.1 非确定性、最小回归集）
- `docs/harness/test-cases/DEMO3-monkey-interpreter.md`（DEMO3 完整复现规格 + §6 本次结论）
- `CHANGELOG.md` `[Unreleased]`（DEMO2–6 + 本次 B/A 记录）
