# DEMO5 实证 · LHT 问题交接文档

> **用途**:本次 DEMO5 测试(全新项目代码生成,任务实际全部完成)暴露了一组 LHT 问题。
> 本文把问题、证据、根因、修法、文件指针全部罗列,供**新会话**直接接手落地。
> **接手第一步**:先读本文 → 再读「修法」里点名的源码 → 按「落地顺序」推进。

---

## 0. 背景一句话

DEMO5 是一个全新项目(Go,monkey 解释器风格)的代码生成任务。**任务实际已全部完成**(产物可 build),
但 UI 任务进度条卡在 **61%**、显示 **12 个未完成项**,且 LHT 在收尾时报了 `incomplete_stop`(假阳性放弃信号)。
排查发现根因是「plan 与 checklist 被当成不相交的工作量相加」,以及一条独立的 `verify_gate` 全 mismatch 现象。

**关键现场数据(来自 `F:\DEMO5\deepseek-thread-thr_68a4.json` + `C:\Users\Administrator\.zagens\logs\sidecar.log`)**:
- 模型 `update_plan` 只调用过 **1 次** → 建了 **12 个 plan 项**,此后全程 **pending**,再没更新过。
- 模型 `checklist_update` 调用多次 → **19 个 checklist 项全部 completed**。
- 进度 = 已完成 / 总数 = `19 / (12 + 19) = 19/31 ≈ 61%`。
- `[lht-probe]` 日志:`verify_gate` 对 items 12–19 全部 `verdict=mismatch`;`gate` 节点 `reason=nudge_skip`;`incomplete_stop` 触发,`open_items:12`。

---

## 1. 问题清单(去重后 5 项)

| # | 类型 | 优先级 | 一句话 |
|---|------|:---:|------|
| **#1** | 确认·根因 | **P1** | plan/checklist 双重计数 → 进度条卡死 + incomplete 误判 + gate 空转 + incomplete_stop 假阳性 |
| **#2** | 待调查 | **P2** | `verify_gate` items 12–19 全 `mismatch`(matcher 过严 vs 模型未验先标,需先定性) |
| **#3** | UI 改进 | **P2** | LHT 面板新增「节点」Tab,把 `[lht-probe]` 节点决策流搬进 UI |
| **#4** | 提示词(辅) | **P3** | base.md 补 plan/checklist 纪律(#1 的软性补充) |
| **#5** | 确认·缺口 | **P1/P2** | cycle 阈值判断只在「回合之间」评估,长 turn 内不周期评估 → 干净的 75% 提前换脑对长 turn 失效,只剩硬溢出兜底 |

---

## 2. 问题详情

### #1 plan/checklist 双重计数(根因 · P1)

**现象(4 个症状,同一根因)**:
- 1a 进度条卡 61%
- 1b `incomplete()` 对真完成任务返回 `true`
- 1c **`continue_injected` 全程一次没触发** —— gate 因 `in_progress_id=null` 直接 `Skip`(`reason=nudge_skip`)
- 1d `incomplete_stop` 假阳性 —— 把真完成的任务误报为「放弃」,`open_items:12`

**根因代码**:`crates/runtime-server/src/long_horizon/graph.rs` 的 `CodeTaskGraph::from_snapshots`
(L33–93)把 plan 项数与 checklist 项数**直接相加**当总工作量:
- `total = phases.len() + checklist_items.len()`(L53)
- `completed = 已完成 plan + 已完成 checklist`(L54–61)
- `open_items` 同样两边相加(L68–75)
- `incomplete()`(L108–119)只要**任一**侧有未完成项就为真

当模型用 plan 起草大纲、然后改用 checklist 执行同一批工作并弃用 plan 时,plan 的 12 项变成「僵尸未完成项」,
被当成额外工作量重复计入。`in_progress_id`(L77–82)优先取 checklist 的,checklist 收尾后为 `None`,
plan 又全是 pending(无 InProgress),于是 `in_progress_id=null` → nudge gate 直接 Skip(见
`crates/runtime-server/src/long_horizon/nudge.rs` `prepare_nudge` L198–200:`in_progress_id` 为 `None` 时
`return NudgeDecision::Skip`)。

**修法(Option 1:checklist 为完成权威)**:
当 checklist 非空时,以 checklist 作为完成度/`open_items`/`incomplete()` 的**权威来源**,plan 仅作大纲展示不计入工作量;
仅当 checklist 为空时回退到 plan。改动集中在 `from_snapshots` 与 `incomplete()`,并补单测:
- 新增用例:plan 全 pending + checklist 全 completed → `completion_pct=100`、`open_items=0`、`incomplete()=false`。
- 保留现有 `plan_only_incomplete`(checklist 为空时仍以 plan 计)行为不变。
- 注意 `in_progress_id` 的回退逻辑(L77–82)也要随之复核:checklist 权威时不应再回退到 plan 的 InProgress。

**验收**:用 DEMO5 的快照(12 plan pending + 19 checklist completed)跑出 100% / 0 open / not incomplete。

---

### #2 verify_gate items 12–19 全 mismatch(待调查 · P2)

**现象**:`[lht-probe]` 中 `verify_gate` 对 items 12–19(带 `[verify:]` 标签的项)全部 `verdict=mismatch`
—— 即「标了 completed 的 verify 项,没匹配到对应验证命令的成功 exec 记录」。但项目实际 build 成功了。

**两种假设(需先定性,别急着改)**:
- **(a) matcher 过严**:模型确实跑了验证(可能在不同 cwd、或经 `run_examples.sh` 间接跑、或命令字符串略有出入),
  但 gate 的「命令字符串 ↔ exec 记录」匹配没认出来 → **假 mismatch 噪声**。修法:放宽匹配(归一化命令、匹配子串/退出码)。
- **(b) 模型未验先标**:模型在收尾批量把 verify 项标 completed,没逐条跑验证命令 → **这正是 LHT 要防的假绿**,
  mismatch 是**正确**信号,只是当前是 advisory 没拦住。修法:让 verify mismatch 对 `[verify:]` 项有更强的收口(阻止标完成 / 强制重跑)。

**起点**:grep `sidecar.log` 里 `verify_gate`,对照 thread 导出里 items 12–19 的 `checklist_update` 时间点与
最近的 `exec_shell` 记录,人工判定属于 (a) 还是 (b)。verify_gate 匹配逻辑在 `crates/runtime-server/src/long_horizon/`
(找 `verify` 相关模块/函数);`[verify:]` 标签解析与命令匹配是关键。

---

### #3 UI:LHT 面板新增「节点」Tab(P2)

**目标**:把现在只能离线 grep `sidecar.log` 才看得到的 `[lht-probe]` 节点决策流,搬进 UI 实时可见。
(本次诊断就是靠扒 log 才看出 `continue_injected` 没触发 —— 有这个 Tab 就一眼可见。)

**位置**:`crates/desktop/web-ui/src/components/LongHorizonPanel.tsx`,在现有三个 Tab(task/cycle/context)旁加第四个。
- Tab 类型定义:`LongHorizonPanelTab`(从 `types` 导入,见 L22);现有值 `'task' | 'cycle' | 'context'`,加 `'nodes'`。
- tabs 数组在 L442–445;状态机在 L319(`useState<LongHorizonPanelTab>('task')`)、轮询在 L426/438。
- i18n:加 `longHorizon.tabNodes`(与 `tabTask`/`tabCycle`/`tabContext` 同处)。

**内容**:每条节点决策一行 —— 时间、节点类型
(`continue_injected` / `gate_skip` / `step_limit_continue` / `incomplete_stop` / `verify_gate` /
`loop_guard_continue` / `cycle_advanced` / `blocked` / `context_warning`)、关键字段
(`reason` / `open_items` / `nudge_count` / `verdict`)。颜色:续写类绿、skip/blocked 黄、incomplete_stop/halt 红、verify mismatch 橙。

**数据来源(关键:基本免后端)**:这些节点**已**作为 `long_horizon.*` status 事件在发,且早前「任务图实时可观测」那次
已建了遥测缓存喂面板(参考现有 `fetchGraph`/`fetchCycles`/`fetchContext` 的拉取命令模式,L426–440)。
所以这个 Tab **主要是前端活** + 可能加一个「取节点事件流」的 Tauri/runtime 查询命令(对照 cycle/context 的命令实现)。
先确认事件是否已落到可查询的缓存里;若已落,纯前端即可。

---

### #4 base.md plan/checklist 纪律(辅 · P3)

**目标**:#1 的软性补充(机制为主、提示为辅)。在 `crates/runtime-server/src/prompts/base.md` 里补一句纪律:
同一批工作不要同时用 plan 和 checklist 各列一遍;若用 checklist 执行,plan 仅作高层大纲,或保持二者同步收尾。
避免模型「建完 plan 即弃用、只更新 checklist」造成僵尸 plan 项。

---

### #5 cycle 阈值判断只在「回合之间」评估,长 turn 内不周期评估(确认·缺口 · P1/P2)

**用户提出的问题**:cycle 靠感知上下文触发 —— 上下文是不是实时统计?如果不是,cycle 就有问题。

**核实结论(分两层)**:
- **上下文 token 值是实时的**:`estimated_input_tokens()` 每次调用都用当前消息缓冲重算
  (`estimate_input_tokens_conservative(&self.session.messages, system)`,`crates/core/src/engine/context.rs:327`)。
  不是「会话结束才统计」的缓存值。UI 面板显示的「当前 N%」也是实时刷新。**数值本身没问题。**
- **但 cycle 的触发判断不是连续/实时的,只在「回合之间」评估**:`maybe_advance_cycle` 全仓**唯一调用点**是
  `crates/runtime-server/src/core/engine/message_handlers.rs:300`(在 `handle_deepseek_turn` 返回 `Completed` 之后)。

**缺口**:一个长程 turn 在 turn loop 内连跑上百个 tool step 期间,`maybe_advance_cycle` **一次都不会被调用**。
即使上下文实时涨过 ~77% 换脑阈值,干净的提前换脑也不会在 turn 内发生。turn 内 checklist 完成只把
`pending_cycle_at_checkpoint = true` 置位(`crates/runtime-server/src/core/engine/turn_loop/host_impl/mod.rs:426`),
该 flag **要等回合之间**才被消费 —— 对 100 步不返回的长 turn,这个边界迟迟不来。

**唯一例外**:刚修的 backlog C `maybe_cycle_handoff_on_context_overflow` 会在 turn 内强切一次,但**只在硬溢出**
(应急压缩压不下去、撞模型硬上限)时触发,是最后关头兜底,**不是干净的 75% 提前换脑**。

**后果**:在「干净 75% 阈值」与「硬溢出兜底」之间,长 turn 内没有任何 cycle 检查 → 为长 turn 设计的「预警带提前换脑」
实际对长 turn 失效;真到顶只能靠 backlog C 应急切(非干净断点)。DEMO5 本身只到 34% 未触发,但任何冲过 77% 的长 turn 都会踩到。

**修法方向**:在 turn loop 内的**安全断点**(每个 tool step 完成 / checklist 项 `completed` 后)增加一次
走阈值闸门 + 干净边界守卫(无 in-flight stream/approval)的 `maybe_advance_cycle` 评估;复用 backlog C 已验证可在
turn 内工作的 `perform_cycle_advance` 主体。注意别在 edit/stream 半道切;沿用 `should_advance_cycle(..., in_flight)`
的干净边界判断。与 backlog C 的硬溢出兜底互补:#5 是「提前、干净」,backlog C 是「到顶、应急」。

**关键文件**:`maybe_advance_cycle` / `perform_cycle_advance`(`crates/runtime-server/src/core/engine/cycle_hooks.rs` L25–57 / L76）、
调用点 `message_handlers.rs:300`、turn loop(`crates/core/src/engine/turn_loop/run.rs` 的 tool step 循环)、
`pending_cycle_at_checkpoint` 置位 `host_impl/mod.rs:426`。

---

## 3. 不计入「问题」的两类(避免混淆)

- ✅ **已修/已实测生效**:backlog C(context 溢出 cycle 交接)、G(loop_guard 续写)、
  I(`incomplete_stop` 探针 —— 本次实测它确实触发了,只是因 #1 而误报)、DEMO4 step 续写(`step_limit_continue` 本次实测生效)。
- 🔭 **未来方向(非 bug)**:并行生成方案,见 `docs/harness/PARALLEL_FRESH_GENERATION.md`(状态 ⬜ 待验证,有去风险实验前置)。

---

## 4. 建议落地顺序

1. **先 #2 定性**(纯调查,不改码):判定 verify mismatch 属 (a) 还是 (b)。这会影响 #1 验收时对 verify 项的预期,也可能本身要改码。
2. **#1 主修**(Option 1):一刀消掉 1a–1d 四个症状。改 `graph.rs` + 单测。
3. **#5 cycle 内周期评估**:与 #1 同属 LHT 可靠性核心,且复用 backlog C 已落地的 `perform_cycle_advance`,改动面可控;建议紧随 #1。
4. **#4 提示词**:随 #1 一起提交(软性补充)。
5. **#3 UI**:独立推进,前端为主;先确认节点事件是否已可查询。
6. 收尾:按 DEMO3/DEMO4 先例,把结论记进 `docs/harness/LONG_HORIZON_CODE_TASKS.md`(DEMO5 实证小节)+ `CHANGELOG.md`。

---

## 5. 关键文件指针速查

| 用途 | 路径 |
|------|------|
| 根因(进度/incomplete/open_items) | `crates/runtime-server/src/long_horizon/graph.rs` `from_snapshots` L33–93 / `incomplete()` L108–119 |
| nudge gate(Skip 逻辑) | `crates/runtime-server/src/long_horizon/nudge.rs` `prepare_nudge` L189–206 |
| cycle 触发(#5) | `crates/runtime-server/src/core/engine/cycle_hooks.rs` `maybe_advance_cycle` L25 / `perform_cycle_advance` L76;唯一调用点 `message_handlers.rs:300`;置位 `host_impl/mod.rs:426` |
| token 估算(实时) | `crates/core/src/engine/context.rs` `estimate_input_tokens_conservative` L327 |
| verify_gate / `[verify:]` 匹配 | `crates/runtime-server/src/long_horizon/`(找 verify 模块) |
| LHT UI 面板(加 Tab) | `crates/desktop/web-ui/src/components/LongHorizonPanel.tsx` Tab 定义 L22/L319/L442 |
| 提示词纪律 | `crates/runtime-server/src/prompts/base.md` |
| LHT 总文档(记结论) | `docs/harness/LONG_HORIZON_CODE_TASKS.md` |
| 现场数据 | thread 导出 `F:\DEMO5\deepseek-thread-thr_68a4.json` / 日志 `C:\Users\Administrator\.zagens\logs\sidecar.log`(grep `[lht-probe]`) |
| 上一份交接 | `.claude/turn-termination-audit-handoff.md` |

---

*生成于 DEMO5 排查会话。新会话接手时,先读 §0–§2 建立全貌,再按 §4 顺序推进。*
