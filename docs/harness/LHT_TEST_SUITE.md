# 长程代码任务测试集（LHT Test Suite）

**状态:** **活文档（回归素材已实跑，编纂中）** — DEMO2–DEMO5 为真实压测产物，外部经典案例为待补充 backlog
**日期:** 2026-05-30（创建）
**范围:** 给 [LHT harness](./LONG_HORIZON_CODE_TASKS.md) 与 [并行生成](./PARALLEL_FRESH_GENERATION.md) 提供**可复现、可客观验收**的长程任务测试案例
**上游:** [`LONG_HORIZON_CODE_TASKS.md`](./LONG_HORIZON_CODE_TASKS.md)（闸门定义、DEMO 实证修正）、[`PARALLEL_FRESH_GENERATION.md`](./PARALLEL_FRESH_GENERATION.md)（fan-out 去风险实验）
**相关:** [`../../CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]`（DEMO2–5 修复记录）、`crates/runtime-server/src/prompts/base.md`（`[verify:]` 纪律）

---

## 0. TL;DR

| 问题 | 立场 |
|------|------|
| 长程任务测试该选什么案例？ | **优先选自带客观验收（编译/测试/跑示例）的项目** —— 契合 LHT「事实源 > 模型声明」第一原则 |
| 最有价值的素材在哪？ | **仓库自己跑过的 DEMO2–DEMO5**（2W 行级 Go 解释器）。每个都钓出过一个**静默早停 / 假绿**漏洞，是天然的回归锚点 |
| 怎么算「测过了」？ | 不看模型「已完成」prose，看 **`[verify:]` gate 全绿 + 实跑产物**。DEMO3/DEMO5 证明「checklist 全勾」可以是假绿 |
| 经典外部案例选哪些？ | 解释器/编译器（多阶段、验收清晰）、CodeCrafters「Build your own X」（协议即契约）、SWE-bench Verified（修复，不可并行）|

**核心教训（来自 DEMO 系列）：** 一个长程任务测试的价值 = 它能否**逼出一种新的「turn 结束但任务没完成」的出口**。length 截断 / prose 早停 / step 耗尽 / loop-guard 停机 / plan-checklist 双计数 / 验收塌缩成创建项 —— 这些都是被具体案例钓出来的，不是设计阶段想到的。

---

## 1. 选型原则：什么样的任务适合做长程测试

LHT 的第一原则是**事实源 > 模型声明**（[`LONG_HORIZON_CODE_TASKS.md` §2](./LONG_HORIZON_CODE_TASKS.md)）。测试案例的选型必须服务这一点：

1. **自带判定式 oracle（最重要）** — 项目必须有**编译器 / 类型系统 / 测试 / 可运行示例**作为客观验收，能写成 checklist 的 `[verify: <command>]` 前缀。没有 oracle 的任务（如「写一篇设计文档」）无法判定假绿，不适合做 harness 回归。
2. **可分解、可叠加** — 多个相对独立的阶段/特性，能拆成 checklist 项；每加一个特性验收边界清晰（解释器、光线追踪、协议实现天然如此）。
3. **跨度足够长** — 要能压到 **step 预算（默认 100）**、**context 预警带（~75%）**、**cycle 换脑阈值（768K）** 之上，才能测到续写/换脑/交接。2W 行级是已验证的"够长"基准。
4. **验收语义不能塌缩** — 警惕 DEMO3 式陷阱：「REPL 跑通全部示例」被拆成「创建示例脚本」就算完成。验收项必须保留**可运行**语义（见 §4 编写规范）。

> **反例（不该当长程测试用）：** trivial 单步任务（改一行、加注释）——会被 LHT gate 的 `graph_trivial` 守卫正确跳过，测不到续写逻辑，只能当作"**不该误续写**"的负向案例。

---

## 2. 黄金回归案例：DEMO2–DEMO5（已实跑）

这四个是仓库**真实压测**跑出来的案例，每个都附带了一个已修复的漏洞编号，可直接当**回归基线**。详细实证见 [`LONG_HORIZON_CODE_TASKS.md`](./LONG_HORIZON_CODE_TASKS.md) 各 DEMO 段与 [`CHANGELOG.md`](../../CHANGELOG.md) `[Unreleased]`。

| 案例 | 任务 | 钓出的漏洞 | 验收信号（回归时盯） |
|------|------|-----------|---------------------|
| **DEMO2** | Go 解释器（写文件后 prose 收尾） | **progress-pass 放行早停**：写了文件却 0% checklist 收尾 | gate 不再 `SkipProgressReset`；`thr_0eda7dcc` 重放 `incomplete=true` 时必须续写 |
| **DEMO3** | 2W 行 Go 解释器（Monkey） | **验收塌缩成创建项**：checklist 全勾、turn `Completed`，但实跑 4 示例崩 2（`%` 取模、`counter1` 词法器不认） | 凡「运行/构建/跑示例」类验收**必须** `[verify: cmd]`；漏标走**软门禁**（B：`unverified_acceptance` 不放行收尾，注入续写，发 `long_horizon.unverified_acceptance_nudge`）。**B 验证：连跑 5 次 5/5 真绿、nudge 全员触发、旧 `graph_complete` 假绿出口归零**（§7） ｜ **完整复现规格：** [`test-cases/DEMO3-monkey-interpreter.md`](./test-cases/DEMO3-monkey-interpreter.md) |
| **DEMO4** | 2W 行 Go 解释器 | **step 耗尽型早停**：~29 分钟卡 40%、turn 空转，撞满默认 `max_steps:100` | `maybe_continue_at_step_limit` 再发预算窗口；`[stream-probe]` 恰 100 条是签名 |
| **DEMO5** | 全新 Go 项目（Monkey 双后端解释器） | **plan/checklist 双计数**：实际全完成、可 build，但进度卡 61%、12 假未完成项、假 `incomplete_stop`；外加 **verify_gate 全 `mismatch` 假绿噪声** + **长 turn 内 cycle 不评估** | checklist 为完成权威 → 100%/0 open；verify matcher 不再误判；`maybe_advance_cycle_at_checkpoint` 长 turn 内换脑 |

### 2.1 为什么 Monkey 解释器是好的压测载体

DEMO3–5 都选了 [*Writing an Interpreter in Go*](https://interpreterbook.com/) 的 Monkey 语言，不是偶然：

- **跨度天然到 2W 行级** — 词法器 → 语法树 → Pratt parser → 求值器 → 内建函数 → REPL，逼到 step/context/cycle 三道阈值之上。
- **验收能判定式** — `go build` / `go vet` / `gofmt` / `go test` / `bash scripts/run_examples.sh` 全是 exit-code oracle，能直接做 `[verify:]` 前缀。
- **假绿陷阱密集** — 特性多（取模、标识符词法规则、闭包），单测易漏覆盖，正好暴露「测试绿 ≠ 行为对」（DEMO3 的根因）。
- **双后端变体（DEMO5）** — tree-walking + 编译到字节码两个后端，制造 plan（高层阶段）与 checklist（细粒度）并存的结构，钓出双计数 bug。

> **复现建议：** 固定 prompt「用 Go 实现 Monkey 语言完整解释器，含 REPL，并写示例脚本覆盖算术（含取模 `%`）、字符串、数组、哈希、闭包、带数字的标识符，最后跑通全部示例」。这条 prompt 同时踩中 DEMO3（取模/`counter1`）与长程阈值，是性价比最高的单条回归。
>
> **Zagens vs Cursor 对比实验：** 用 [**DEMO6 双后端超集**](./test-cases/DEMO6-monkey-dual-backend.md)（同一 prompt + §3 oracle 记录表）——比 DEMO3 更长、多 `parity.sh` / `coverage_gate.sh`，用于观察 LHT 在「无人帮你跑 oracle」时是否仍占优。
>
> **代码量 ≥10k 加压：** [**DEMO7 Monkey 平台化**](./test-cases/DEMO7-monkey-platform-10k.md)（DEMO6 + class/while/工具链/testdata≥50 + `scripts/loc_gate.sh`）——DEMO6 实证仅 ~4k 行，本案例用硬 LOC 闸门拉高长程与 step 压力。
>
> **盲测对比（仅目标 · 无过程）：** [**DEMO8**](./test-cases/DEMO8-monkey-blind-goal-only.md) — 同最终 oracle（DEMO7 §4），prompt **不写**步骤/脚本名/行数阈值；用于 Zagens / OpenCode / Cursor 公平对比「自主拆解 vs 显式 harness」。

### 2.2 存量审计实证：CMS-AUDIT（CMS02，2026-06）

[`cms-full-code-audit.md`](./test-cases/cms-full-code-audit.md) 记录 **F:\CMS框架** **CMS02 全链路跑通（✅）**：**221 源文件、19 区域、12 Explore、5 项 `[verify:]` 多轮真绿、HIGH×13 报告、19 处类型清理、P0×5+P1×3 修复、checklist 9/9·100%、终验 `tsc` exit 0**。价值在于证明 LHT 与 CRAFT **无意组合**即可撑起「审→验→修→再验」——单 thread 内无需用户切换模式。诚实边界：P2 设计器残留与部分 HIGH 未收口（见案例 §9.5）。后续 **CMS-L** / **CMS-XL** 沿用同一 oracle 骨架。

---

## 3. 推荐扩展案例（外部经典，按 harness 能力分组）

下表把社区公认的长程编程任务，映射到 LHT / 并行生成的具体闸门。优先补**带自带测试**的项目。

| 能力 / 闸门 | 推荐案例 | 为何对口 | 客观验收 |
|------------|----------|----------|----------|
| **强制续写（§4）+ 验收纪律** | 光线追踪器（*Ray Tracing in One Weekend*）、Crafting Interpreters（Lox） | 渐进式叠特性，中途留 pending 项可测「不早停」 | 渲染输出像素比对 / 解释器测试套件 |
| **Cycle + 交接** | SQLite-clone（cstack 教程）、库级生成（Long Code Arena） | 状态强依赖、超单窗，逼出换脑 + carry_forward | 自带集成测试 / 库 API 编译 |
| **接口稳定性 + 重构抗性 + cycle 交接** | **MicroStack（Go 微服务框架，1.5–4 万行）** — 见 [`test-cases/microstack-framework.md`](./test-cases/microstack-framework.md) | 15+ 互相依赖包 + 跨模块接口契约 → 测长期架构稳定性 + 非破坏性修改（解释器/Redis 测不到）；cycle 走「验证跑 A」低阈值单长 turn | `go build/vet/gofmt/test -cover` + **`git diff contracts/` 零改动** + Router 换 trie 后测试全绿 + Todo e2e |
| **存量全库审计 + LHT↔CRAFT 审修闭环** | **CMS 单体（TypeScript/Vue，~221 文件）** — 见 [`test-cases/cms-full-code-audit.md`](./test-cases/cms-full-code-audit.md)（首跑 **CMS02**） | 19 区域并行 Explore + scratchpad + 可交付 DOCX 报告 + **审→验→修→再验**；与 MicroStack（从零搭建）互补；对标 OpenCode 存量审计族 | `tsc`/`vue-tsc` exit 0 + 报告存在 + inventory 全 done + scratchpad 笔记数；夹具 [`fixtures/cms-audit-completion-gate.toml`](./fixtures/cms-audit-completion-gate.toml) |
| **并行 fan-out/join + 契约闸门** | CodeCrafters「Build your own Redis / Git / HTTP server」、全栈 CRUD（TodoMVC） | 协议/接口即天然契约，模块边界清晰 → 验 P0.5/P1.5 | 协议官方测试用例 / 端到端 e2e |
| **修复 / 重构（不可并行，验 §1.2）** | SWE-bench Verified 小样本（500 题取子集） | 真实 issue + 仓库自带测试判定，已存在代码耦合 | 仓库 `pytest` / 复现脚本 |
| **算法生成（防污染）** | LiveCodeBench、Exercism（Aider polyglot 同源） | 持续更新题库，测纯生成 + 多语言编辑纪律 | 题目自带测试 |

> **并行案例的前置：** 引入 CodeCrafters / 全栈 CRUD 做并行测试前，先跑 [`PARALLEL_FRESH_GENERATION.md` §7.1](./PARALLEL_FRESH_GENERATION.md) 的去风险实验（量痛点 + 手动模拟一次 fan-out）。这些案例正是那一步的理想载体（协议即契约，契约能否一次冻结一目了然）。

---

## 4. 测试案例编写规范（避免 DEMO3 式假绿）

每个进回归集的案例都按下面模板写，核心是**让验收带 `[verify:]`、保留可运行语义**：

```
案例名：<语言> 实现 <项目>
prompt：<一句话目标，显式点名易漏特性>
分解（参考，模型自拆 checklist）：
  - [ ] 词法器
  - [ ] 解析器
  - [ ] 求值器
  - [verify: go build ./...] 编译通过
  - [verify: go test ./...] 单测通过
  - [verify: bash scripts/run_examples.sh] 全部示例脚本跑通   ← 关键：不是「创建示例脚本」
验收 oracle：上述 [verify:] 命令全 exit 0 + 人工抽查 1 个示例输出
钓鱼点：<这条想压的阈值/漏洞，如「取模未实现假绿」「step 耗尽」>
```

**三条铁律（DEMO3/DEMO5 血泪）：**

1. **「创建文件」≠「验证通过」** — 凡「运行 / 构建 / 测试通过 / 跑示例 / lint 干净」类验收,必须写成 `[verify: cmd] <label>`,绝不能拆成「创建 xxx 文件」。
2. **单测要覆盖钓鱼特性** — DEMO3 的 `go test` 真绿但漏了 `%` 和 `counter1`。回归案例的 `[verify:]` 命令必须真能验到目标特性,否则是"绿得没意义"。
3. **plan 与 checklist 是一体** — 别让模型起个 plan 又弃用、只动 checklist（DEMO5 双计数根因）。prompt 可提示「checklist 执行工作时,plan 保持几个稳定高层阶段」。

---

## 5. 验收与观测：怎么判一次测试通过

不靠模型 prose,靠 harness 自己的探针 + 实跑:

| 信号源 | 看什么 | 工具 |
|--------|--------|------|
| **`[lht-probe]` 节点流** | `continue_injected` 是否触发、`gate_skip` 的 `reason`、`verify_gate` 的 `verdict`(`verified`/`mismatch`/`unverified_acceptance`/`untagged_ok`)、漏标验收是否触发 `unverified_acceptance_nudge`（B 软门禁，理想：漏标→nudge→补 `[verify:]` 真跑→`verified`，而非 `graph_complete` 直接收尾） | `Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\]'` |
| **LHT 面板 Nodes Tab** | 决策流实时颜色编码(续写绿 / skip 黄 / `incomplete_stop` 红 / verify `mismatch` 橙)；**组合式门禁(P2)** 顶栏 `completion_gate` 摘要 + `manifest_gate_*`/`completion_audit`/`audit_unmet` 节点 | Zagens 左下长程面板；grep 模板见 [`COMPOSABLE_HARNESS.md` §11](./COMPOSABLE_HARNESS.md) |
| **`[stream-probe]` 摘要** | 区分截断类型:`stop_reason`、`stream_errors`、`chunk_timeout`、流条数(恰 100 = step 耗尽签名) | 同上 grep `[stream-probe]` |
| **进度图客观性** | checklist 全勾时 `completion_pct=100`、`open_items=0`、`incomplete=false`(DEMO5 #1 回归) | `harness/task-graph` 负载 |
| **产物实跑** | 抽查 `[verify:]` 之外的真实行为(DEMO3:示例真能跑、取模真实现) | 人工 / 脚本 |

**判定准则:** 一次测试**通过** = `[verify:]` gate 全 `verified`(无 `mismatch`/`untagged_ok`)+ 进度图诚实 100% + 抽查产物行为正确 + **无任何静默早停出口**(节点流里不出现孤立的 `incomplete_stop`)。

### 5.1 为什么判定只能靠 oracle，不能靠输出比对（非确定性）

同一 prompt 每次输出都不同，是 LLM 的**固有属性**，分三层、且大部分压不掉：

1. **采样随机**(`temperature`/`top_p`)—— 唯一原本可调的旋钮;
2. **系统级不确定**(浮点非结合 + GPU 规约顺序 + 服务端 batching + MoE 专家路由受同 batch 影响)—— 即便贪心解码也压不掉,且与 prompt 无关;
3. **agent 级联放大** —— 长程任务里上游一个 token 的差异沿数百 step 放大成完全不同的执行路径。

> **⚠️ DeepSeek V4 quirk（[官方文档](https://api-docs.deepseek.com/zh-cn/guides/thinking_mode)）:** 思考模式**不支持 `temperature`/`top_p`/`presence_penalty`/`frequency_penalty`**——为兼容已有软件,**设置不报错、但也不生效**(静默忽略,排查时易被骗)。思考强度改由 `reasoning_effort`(high/max;Agent 类请求默认 max)控制,与采样随机性无关。所以连第 1 层旋钮都没有——**"调低 temperature 稳复现"这条路对 V4 思考模式封死**。本仓库 runtime 也未下发 `seed`(DeepSeek 链路无此参数)。

**推论(本测试集的立身之本):** 既然随机性既不可控、又会被长程放大,**测试判定就绝不能依赖"输出逐字/逐结构一致"**,只能靠不会随机的客观 oracle(`[verify:]` 跑测试、`conformance.sh` 验特性、SWE-bench `FAIL_TO_PASS`)判**终态行为**。模型每次走的路不同无所谓,终态正确即过。这正是"事实源 > 模型声明"在测试层的硬约束。

### 5.2 自动化：headless L2 测试基建

§5 的观测信号应能**机器采集**,而非只靠桌面 grep。端到端驱动、任务 TOML、harness profile、JSONL 落盘与 PR/nightly gate 的规格见 **[`LHT_EVAL_INFRASTRUCTURE.md`](./LHT_EVAL_INFRASTRUCTURE.md)**（v0.2 定位：**Harness 正规测试方法**,非论文统计）。

| 人工（今天） | 自动化（目标） |
|--------------|----------------|
| 粘贴 prompt、盯面板 | `lht-harness-run.ps1` + task spec |
| grep `sidecar.log` | run 结束解析 `[lht-probe]` → `probe_summary` |
| 手跑 `scripts/test_redis.sh` | oracle 子进程 + exit code |
| 「感觉 DEMO3 又假绿了」 | `outcome_class: false_green` + harness 不变量断言 |

---

## 6. 最小回归集建议（先跑这三个）

不必一上来铺全。建议先固化下面三条覆盖主要风险面:

1. **Monkey 解释器(DEMO3 复现 prompt)** —— 一条覆盖验收塌缩 + step/context 阈值,性价比最高(§2.1)。完整可执行规格见 [`test-cases/DEMO3-monkey-interpreter.md`](./test-cases/DEMO3-monkey-interpreter.md)(prompt / `[verify:]` 清单 / conformance 脚本 / 离线回放 / 判定矩阵)。
2. **CodeCrafters Redis(单模型串行)** —— 协议官方测试做 oracle,验「长程 + 客观验收」且为后续并行实验铺路。可复制 prompt + redis-cli oracle 见 [`test-cases/codecrafters-redis.md`](./test-cases/codecrafters-redis.md)。
3. **SWE-bench Verified 小样本(10–20 题)** —— 验修复路径与「重构/修复不可并行」结论,与生成路径互补。可复制 prompt + 官方 harness oracle 见 [`test-cases/swe-bench-verified-sample.md`](./test-cases/swe-bench-verified-sample.md)。

> 三条都满足「自带客观 oracle」,可纳入 CI 形态的离线回放(grep `[lht-probe]` 重放决策环),不依赖人工判 prose。

---

**文档修订记录:**
- 2026-05-30 创建:编纂 DEMO2–DEMO5 真实压测为黄金回归案例 + 外部经典案例映射 + `[verify:]` 编写规范 + 最小回归集。
- 2026-05-30 更新 DEMO3 行 + §5 信号表:记录 B（`unverified_acceptance` 软门禁）落地后连跑 5 次 5/5 真绿验证、`unverified_acceptance_nudge` 纳入观测信号、旧 `graph_complete` 假绿出口归零。
- 2026-06-05 新增 §2.2 + §3 行：**CMS-AUDIT（CMS02）** 存量全库审计案例（`test-cases/cms-full-code-audit.md` + `fixtures/cms-audit-completion-gate.toml`）；预留 CMS-L / CMS-XL 规模梯度。
- 2026-06-05 §2.2 终态：CMS02 全链路签收（审计+verify+类型清理+P0/P1+终验）。
