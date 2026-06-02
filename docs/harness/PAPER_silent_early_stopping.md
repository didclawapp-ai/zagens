# 长程编码 Agent 的静默早停：一套失败分类法与基于 Oracle 锚定的完成门禁

*Silent Early-Stopping in Long-Horizon Coding Agents: A Failure Taxonomy and an Oracle-Anchored Completion Harness*

> **文档性质:** 论文草稿（路径 A — 经验/系统型论文）。中文撰写以便迭代；定稿投稿前可整体译为英文。
> **状态:** v0.2 草稿（2026-06-02）。新增 §7.6 LabelMakePro 自发宏流程实证与附录 C（gate 时间线 / 成本对照）。评测一节区分「已有定性证据」与「待补充的统计实验」，后者明确标注 *TODO*。
> **素材来源:** [`LONG_HORIZON_CODE_TASKS.md`](./LONG_HORIZON_CODE_TASKS.md)、[`COMPOSABLE_HARNESS.md`](./COMPOSABLE_HARNESS.md)、[`LHT_TEST_SUITE.md`](./LHT_TEST_SUITE.md)、`crates/runtime-server/src/long_horizon/`（23 文件 / ~6K LOC）、DEMO2–6 压测笔记。

---

## 摘要（Abstract）

大语言模型驱动的编码 Agent 在执行**长程任务**（数千至数万行的代码生成、多文件重构、跨模块迁移）时，普遍存在一种被我们称为**静默早停（silent early-stopping）**的失败：Agent 在任务尚未真正完成时结束本轮交互，却**不报告任何错误**，对外表现为「已完成」。我们发现这一现象并非单一 bug，而是**一类**问题——它从多个互相独立的「回合终止出口」泄漏出来。

本文做出三点贡献。**第一**，我们基于一个真实桌面 Agent 产品（Zagens）上的连续压测，归纳出长程编码任务静默早停的**失败分类法**，识别出至少六种机制各异的早停出口（散文早停、step 预算耗尽、loop-guard 停机、plan/checklist 双计数、验收语义塌缩、length 截断），并提出一条工程不变式：**任何「回合结束」的出口都必须经过同一道完成闸门**。**第二**，我们论证：要可靠地判定「是否真完成」，**评估信号（grounding signal）的质量与独立性比生成机制的复杂度更关键**；并据此确立一条设计铁律——**绝不让 LLM 充当放行/否决的「法官」，只允许它做「缺口枚举器」**，最终裁决必须落在退出码与路径命中这类可离线回放的机器 oracle 上。**第三**，我们给出一个落地实现：一套**组合式完成门禁（composable completion harness）**，由「模型自驱 → exit-code 验收 manifest 主动真跑 → 纯机器交付物对账」三层加一道独立的 stub/半成品扫描门构成，配合有界返工与诚实的 `audit_unmet` 终止，在不引入任何 LLM 裁决的前提下，把「完成」从「模型自报清单清零」改写为「对规格化 manifest 的可验证达成」。

我们在一个 2 万行级的 Go 解释器生成任务上观察到：在修复关键的「验收语义塌缩」假绿出口后，端到端通过率从约 62.5% 提升至连续 5 次复跑 5/5 真绿。我们诚实地讨论该证据的局限（单模型、单任务族、样本量小、缺横向基线），并把它作为后续严谨评测的起点。

**关键词:** 编码 Agent、长程任务、Agent harness、完成判定、grounding signal、自我改进、可复现评测

---

## 1. 引言（Introduction）

把「写一个完整的解释器」「把这个后端从 Electron 迁到 Tauri」这类任务交给一个 LLM Agent，今天的系统大多能**开个好头**：它会拆计划、建清单、改一批文件、跑几次测试。但当任务跨度达到数千上万行、需要数十到上百个工具调用、单次会话撑满甚至超过模型上下文窗口时，一个反复出现的现象是：**Agent 在任务还没真正做完时就停下来了，并且它「以为」自己做完了。**

对用户而言，典型表现是：侧边栏的清单还剩若干 pending 项，或计划阶段没全部标 completed，但 Agent 已经输出了一段「已完成 / 总结」式的文字，不再发起任何工具调用，本轮结束。我们把这种**「认知层面提前终止、且不抛出任何失败信号」**的行为称为**静默早停**。

本文的出发点是一个反直觉的观察：**静默早停不是一个 bug，而是一类 bug。** 在我们的系统里，「一次回合结束」可以从很多条代码路径发生——模型主动以文字收尾、撞满工具步数预算、防循环保护停机、上下文溢出、流式截断……每一条出口如果**没有**经过一道统一的「任务是否真完成」的检查，就会变成一个新的静默早停泄漏点。我们把这条教训提炼为一句工程不变式（§3.4）。

与「早停」同样棘手的是它的**对偶问题——假绿（false-green）**：模型把清单全部勾完、构建也通过，但功能其实是缺的或错的。我们在一次 2 万行的解释器压测中遇到过：任务跑完、checklist 全勾、回合干净结束，但事后实跑 4 个示例脚本崩了 2 个。根因不是模型「撒谎」，而是**它在分解任务时，把「示例能跑通」这个可运行的验收，降级成了「创建示例文件」这个只要写盘就算完成的项**。这暴露了一个更深的问题：**当「完成」的定义完全来自模型自产的清单时，完成度的上界就只是「模型自拆清单的完整度」，而不是「对规格目标的达成度」。**

这两个问题——早停与假绿——指向同一个根因：**缺少一个独立于建造者本身的、可靠的「完成」锚定信号**。本文围绕这个根因展开，贡献如下：

1. **失败分类法（§3）：** 长程编码任务静默早停的六类出口，每类附真实压测线程的实证签名，以及「所有出口必须收敛到同一道闸门」的不变式。
2. **设计原则（§4）：** 论证「评估信号的质量 × 独立性 > 生成机制复杂度」，并由此推出「LLM 只做缺口枚举器、不做法官」的铁律。
3. **系统与实现（§5、§6）：** 组合式完成门禁的三层结构 + stub 门 + 有界返工 + 诚实耗尽，全程机器裁决、可离线回放。

我们也在 §7、§8 中明确**没有解决什么**：本机制的完成度上界等于 manifest 的完整度，而非规格散文本身；上下文/文档化记忆能维持注意力但终会填满，知识参数化属于训练侧，harness 够不到。

---

## 2. 背景与问题定义（Background & Problem Statement）

### 2.1 Agent = Model + Harness

我们采用业界逐渐形成共识的拆分：一个编码 Agent 由**模型（Model）**与**载具（Harness）**两部分组成。模型负责推理、工具选择、长上下文处理；harness 负责回合循环、工具执行、记忆与上下文管理、子代理编排、审批、持久化与可观测 UI。本文讨论的所有机制都落在 **harness 层**——我们不微调模型，只改变「模型被运行的方式」。

这一定位很重要：它界定了我们**能**和**不能**解决的问题。harness 能维持注意力、保留情节记忆、施加外部约束；但它无法把经验**参数化**进模型权重——那属于训练侧，是另一条正交的轴（§8.3）。

### 2.2 长程任务与「完成」的定义问题

我们关注的任务具有三个特征：（a）跨度长——足以撑满工具步数预算、上下文预警带、上下文窗口换脑阈值；（b）可分解、可叠加——能拆成带验收边界的子项；（c）自带判定式 oracle——存在编译器、类型系统、测试、可运行示例等客观验收。解释器/编译器、协议实现（如 Redis）、库级生成、真实 issue 修复都属此类。

在多数现有 harness（含我们系统的早期形态）里，「完成」被实现为一个朴素判据：

```
完成 := 模型自产的 plan + checklist 中，不存在 {pending, in_progress} 项
```

这个判据衡量的是**模型对自己计划的忠实度**，而**不是对规格目标的完成度**。它有两个致命弱点：

- **早停弱点：** 模型可以在清单未清空时就以文字收尾、不再调工具——如果 harness 不在这个出口拦一下，回合就静默结束了。
- **假绿弱点：** 模型可以欠拆一个不完整的清单，把它跑到 0，得到一个「合法」的完成——而漏做的交付物**压根没进清单**，任何只遍历清单的检查都抓不到。

### 2.3 一个关键约束：评测的非确定性

我们的目标模型（DeepSeek V4 思考模式）有一个对评测方法论有直接约束力的特性：**它不支持 `temperature` / `top_p` / `seed` 等采样控制**（思考模式下这些参数被静默忽略、设置不报错也不生效）。叠加系统级不确定性（浮点非结合、GPU 规约顺序、服务端 batching、MoE 专家路由受同 batch 影响），以及 agent 级联放大（上游一个 token 的差异沿数百 step 放大成完全不同的执行路径），**同一 prompt 每次的执行轨迹都不同，且这种随机性既不可控、又被长程放大。**

这个约束**否定了「靠输出逐字/逐结构一致来判定」的整条路线**，并成为我们方法论的基石：**判定只能依赖不随机的客观 oracle**（`[verify:]` 命令退出码、官方协议测试、SWE-bench 的 `FAIL_TO_PASS`），看**终态行为**而非中间轨迹。模型每次走的路不同无所谓，终态正确即过。这既是限制，也恰好与本文「机器 oracle 当法官」的核心主张同构。

---

## 3. 贡献一：静默早停失败分类法（A Taxonomy of Silent Early-Stopping）

本节是本文的核心经验贡献。我们通过对一个真实桌面 Agent 产品上连续长程压测（代号 DEMO2–DEMO6，主要载体为 2 万行级 Go Monkey 解释器）的离线复盘，归纳出静默早停的**六类机制各异的出口**。每一类都不是设计阶段预想到的，而是被一个具体压测案例「钓」出来的——这本身说明了该问题空间的隐蔽性。

### 3.1 分类法总览

| # | 早停类型 | 机制 | 实证签名 | 修复方向 |
|---|---------|------|---------|---------|
| T1 | **散文早停（prose stop）** | 模型写完「已完成/总结」文字、不再调工具、回合结束 | 0 工具调用 + 清单未清空 | 在 *no-tool-uses* 出口注入强制续写 nudge |
| T2 | **进展放行误判** | 写了文件但清单仍 0%，却被「有进展」逻辑放行收尾 | `gate_skip: nudge_skip_progress_reset`，`incomplete=true` 却 Completed | 「有进展」只清零放弃计数、不跳过续写 |
| T3 | **验收语义塌缩** | 「示例能跑通」被拆成「创建示例文件」，写盘即完成 → 假绿 | checklist 全勾、turn Completed，实跑示例崩 | `[verify: cmd]` 纪律 + 漏标软门禁不放行 |
| T4 | **step 预算耗尽** | 工具密集型回合撞满 `max_steps`（默认 100），`break` 直接收尾 | `[stream-probe]` 恰 100 条、全 `tool_calls`、卡 40% | step cap 处再发预算窗口 + 注入续写 |
| T5 | **loop-guard 停机** | 同一工具连续失败 N 次被防循环保护中断，绕过续写闸门 | tool_phase `break_outer_loop` 不经 no-tool 路径 | halt 出口清失败计数 + 注入「换方法」nudge |
| T6 | **plan/checklist 双计数** | plan 项与 checklist 项被当成不相交工作量相加，僵尸 pending 卡死进度 | 进度卡 61%、12 个假未完成项、假 `incomplete_stop` | checklist 为完成权威，plan 仅作大纲 |
| （T0） | **length 截断** | 输出 token 上限截断流（背景类，已由 `max_tokens` 提升修复） | `stop_reason=length` | 提高 `max_tokens`、流探针监控 |

> 这些案例的载体选择不是偶然：Monkey 解释器（词法器→Pratt parser→求值器→内建函数→REPL）天然跨度到 2 万行级，逼到 step/context/cycle 三道阈值之上；且 `go build`/`go vet`/`go test`/`bash run_examples.sh` 全是 exit-code oracle，能直接写成 `[verify:]` 前缀；特性多（取模、标识符词法规则、闭包）单测易漏覆盖，正好暴露「测试绿 ≠ 行为对」。

### 3.2 典型案例细读

**T3 — 验收语义塌缩（最隐蔽的假绿）。** 一次 2 万行 Go 解释器压测里，任务完整跑完、checklist 全勾、回合 Completed，`max_tokens` 全程在位、零截断；但事后实跑示例脚本 4 个崩 2 个（`%` 取模未实现、带数字标识符 `counter1` 词法器不认）。根因不是模型谎报：**它在分解时把「REPL 能跑通全部示例」这个可运行验收，拆成了 checklist 第 13 项「创建示例脚本」——创建文件即算完成**；唯一带 `[verify:]` gate 的只有 `go build/vet/test`，而单测没覆盖那两个特性，于是 `go test` 真绿 → 全勾 → 收尾。这是一个**纯验证闭环漏洞**，与截断、早停都无关。

**T4 — step 预算耗尽（第三种静默出口）。** 一次同款压测跑到约 29 分钟卡在 40%、回合空转。流探针证明不是流/length 截断：`[stream-probe]` **恰好 100 条**、全 `stop_reason=tool_calls`、`stream_errors=0`——即撞满了默认 `max_steps: 100` 工具步预算，运行时用一句 `break`（`Reached maximum steps`）就终止了。关键在于：**续写 nudge 此前只挂在 *no-tool-uses* 路径，而工具密集型回合打满步数预算时完全绕过了 harness**，任务直接停摆（全程无续写探针、无清单完成项）。

**T6 — plan/checklist 双计数。** 一次全新 Go 项目（双后端解释器）生成任务**实际全部完成、产物可 build**，但 UI 进度条卡 **61%**、显示 12 个未完成项，收尾还报了假 `incomplete_stop`。根因：进度计算把 plan 项数与 checklist 项数当成不相交工作量直接相加（`total = phases + checklist`）；模型只建了 12 个 plan 项随即弃用、全程推 checklist（19 项全完成），于是 `19/(12+19) ≈ 61%`，12 个 pending plan 变成「僵尸未完成项」，把真完成误判为放弃。

### 3.3 一次系统性的「回合终止出口审计」

T4、T5 的发现促使我们做了一件方法论上有意义的事：**对回合循环的全部终止出口做一次走查**。结构性结论是——外层循环里所有 `break` 最终都汇到同一个 `Completed` 落点（除非显式置 `turn_error` 才走 `Failed`）。于是判定标准变得极简：

> **任何「绕过完成闸门就 break、且没置 `turn_error`」的出口 = 把「任务未完成」标成「Completed」的假绿。**

逐出口核对后，我们发现 loop-guard 停机（T5）正是这样一个漏点，并补上了它。这次审计把「再钓一个 case 补一个洞」的被动模式，升级成了「枚举所有出口、证明每个都收敛到闸门」的主动模式。

### 3.4 不变式（本节的可迁移结论）

> **不变式（Completion-Gate Invariant）:** 在长程任务激活时，回合循环的**每一个**终止出口（散文收尾、step 耗尽、loop-guard 停机、上下文溢出、流截断……）都必须先经过同一道「任务是否真完成」的闸门；任何绕过它的 `break` 都是一个潜在的静默早停/假绿泄漏点。

这条不变式独立于我们的具体实现，可迁移到任何「回合循环 + 工具调用」结构的 Agent harness。它也解释了为什么静默早停是「一类」而非「一个」问题：出口越多，泄漏面越大。

---

## 4. 贡献二：grounding signal 的质量 × 独立性（Design Principle）

### 4.1 核心论点

我们把分类法背后的设计原则提炼为一句话：

> **一个自我改进的循环能否不退化，不取决于生成机制有多复杂，而取决于其评估/锚定信号（grounding signal）的质量，以及该信号相对于模型自身的独立性。缺少可靠且独立的锚定信号，循环最终必然退化为「自我确认（self-confirmation）」——模型不断强化它已相信的模式，而不一定更接近真实目标。**

这一判断与近期关于持续学习/自我改进的研究判断同构。把它落到本文语境：「**模型自报全部完成、build 也绿**」正是自我确认的工程实例。`completion := 模型自产清单清零`（§2.2）之所以失效，是因为**评估信号（清单状态）与被评估者（模型）不独立**——清单本身就是模型写的。

### 4.2 独立性的两个层次

我们区分两类独立性，二者叠加才构成可靠锚定：

| 层次 | 含义 | 本文实例 |
|------|------|---------|
| **规则独立（rule-independent）** | 由机器规则扫描裁决，不经任何模型推理 | stub/半成品门（正则扫 `todo!()`/`unimplemented!()`/"not implemented" 抛出，§5.4）；交付物路径/glob 对账 |
| **主体独立（agent-independent）** | 由建造者**之外**的主体产出 | 对抗式只读审核员（独立子代理，仅枚举缺口，§5.5；待实现） |

### 4.3 铁律：LLM 只做缺口枚举器，不做法官

一个直觉方案是「完成后让一个 LLM 子代理审核，不过就返工」。方向对（要一道独立的门），但**让 LLM 拥有放行/否决权是错的**：一个靠「读一读、给 LGTM」的 LLM 审核员，本身是**软的、非确定、可被忽悠、不可离线回放**的 oracle——这恰好把「建造者自我确认」平移成「审核者—建造者合谋确认」，独立性形同虚设。

因此我们写死一条边界：

| | 法官型审核（**禁止**） | 缺口枚举器型审核（**允许**） |
|---|---|---|
| 权力 | 直接判 pass/fail、放行或拦截 | 无放行/否决权；只产出「疑似缺口」候选 |
| 输出 | 「LGTM / 不通过」散文 | 机器可检验断言：`{file:line, 缺什么, 建议 [verify: cmd]}` |
| 最终裁决者 | 它自己（软、可被忽悠、不可回放） | **仍是机器 oracle**——候选回灌后由退出码/路径对账裁决 |
| 最坏失败 | 审核者—建造者合谋盖章 | 只是「多报几个假缺口」，被机器门一跑即证伪，不会放水 |

> **设计原则（写死）:** 独立主体（无论规则还是 LLM）的产物**绝不直接进入完成判定的放行/拦截**；它只能**拓宽**机器门的检查面（把没进清单、正则也没覆盖的缺口，转译成新的 `[verify:]` / deliverable / stub 模式），最终绿不绿仍由退出码与路径命中说了算。**独立性用来拓宽锚定信号，而非替代它。**

这条原则使本文与「多 Agent 互评」「LLM-as-a-judge」一类方法划清界限：我们认为在缺乏外部验证器的设定下，再多的 LLM 互评也只是把自我确认换了个主语。

---

## 5. 贡献三：组合式完成门禁（System Design）

基于 §3 的不变式与 §4 的铁律，我们设计并实现了一套**组合式完成门禁（composable completion harness）**。「组合式」指三层可独立开关，算子按任务类型装配。

### 5.1 三支柱的运行底座

完成门禁运行在三个已有的长程能力之上，它们解决「同一回合内不早停」之外的「跨小时任务如何续命」：

- **Cycle（检查点-重启）：** 在同一会话线程内，当估算的输入 token 越过阈值（默认 ~768K，约 1M 窗的 75%）时，归档当前 transcript、清空消息缓冲、用 seed 消息启动新 cycle——用户仍在同一条聊天里「换脑不换聊天」。这是长程任务管理上下文的**主路径**（优于有损压缩，避免「半原文半摘要」的 Frankenstein 上下文）。
- **交接（handoff）：** 跨 cycle 的两层保留——确定性层自动快照 plan/checklist/working-set/子代理状态（`StructuredState`），模型层写 `<carry_forward>`（决策+原因、约束、已失败方案）。
- **可视化：** 任务图、cycle 时间线、完成门禁节点流实时呈现，让用户能基于事实 steer，而非盯着模型的 prose 焦虑。

为支持 §3.4 的不变式，我们还把 cycle 闸门从「仅回合之间评估」扩展到「长回合内的安全断点也评估」（`maybe_advance_cycle_at_checkpoint`），并在上下文溢出硬失败前强制一次 cycle 交接——这两处都是把「续命」机制接到此前绕过它的出口上。

### 5.2 层 1：模型自驱 + 强制续写

保留现有自驱（自拆 plan/checklist + 工具推进）。在 *no-tool-uses* 收尾出口插入强制续写：当（无工具调用 ∧ 长程激活 ∧ 任务图未完成 ∧ 本回合未注入 ∧ 未豁免）全部成立时，注入一条带**目标 + 进度条 + 仍待完成项**的 nudge，让回合继续而不增加 step 预算。配套一个 `NudgeTracker`：同一进行项连续若干次无「qualified progress」则标记放弃（`blocked`），换项重置，并设每项 nudge 硬上限防空转。

**「有进展」如何判定（实事求是）。** 早期用命令正则白名单（`cargo test`/`go test`…）判断「这一轮有没有进展」，但这对 `make`/自定义脚本/非主流语言项目会失真。我们升级为**客观、语言无关的信号**：`git status --porcelain` 的签名相对上次 nudge 是否变化——只要工作树真的变了（且不只是 gitignore 的产物），无论用什么命令产生都算进展。正则只作为「测试跑绿但无文件变更」这类进展的补充，不再是唯一裁判。

我们还修正了一个微妙的语义错误（T2）：**「有进展」只清零放弃计数、不跳过续写**。因为闸门恰恰在「模型写了点东西、然后中途撒手」时触发，这一轮几乎必然「有进展」；若以此跳过 nudge，模型就会写了文件、清单停在低位就收尾。

### 5.3 层 2：exit-code 验收 manifest（harness 主动真跑）

层 2 是把「完成」从「模型说跑过」改成「harness 现在真跑、看退出码」的关键。在「任务图判完成」放行之前，harness **主动 exec** 一组必须 exit 0 的验收命令，任一非 0 即强制返工。它**不信任**模型侧「曾经跑过且 exit 0」的历史记录。

层 2 的命令有三个**任务无关、零 per-task 配置**的来源，用一个全局开关覆盖所有任务：

| 来源 | 命令从哪来 | 授信 |
|------|-----------|------|
| **算子 manifest** | 受信配置/夹具里手写的 `verify` 列表（per-task） | 受信全局配置/内置夹具 |
| **模型 `[verify:]` 复跑** | 收尾时扫已完成 checklist 项里模型自己写的 `[verify: cmd]`，主动复跑 | 无新增授信面——命令本就在模型 exec 权限内 |
| **工具链探测门** | 探测 `go.mod`/`Cargo.toml`/`package.json`/… → 跑该工具链 canonical build/test | 内置固定命令 |

三来源合并去重后一次跑完，**每条按自己来源的 mode（observe/enforce）裁决**，可单轮混合。这一设计回应了「改进应面向所有任务而非逐个手写配置」——层 2 的价值内核不是那张命令表，而是「harness 主动跑、退出码当法官」这个**动作**，而这个动作可以零配置地覆盖所有代码任务。

**执行契约（编码前必须钉死，§安全）：** 目标平台含 Windows，manifest 不得假设 `bash` 存在（每条命令 `{cmd, shell?}` 或 `{argv, shell="none"}`）；cwd 为任务 workspace 根并 canonicalize 防 `..` 逃逸；每条独立超时（超时视为该门未绿）；区分 `assertion`（正常 test red）与 `infra`（命令找不到/段错误/超时）退出类别，`infra` 连续失败倾向诚实 `audit_unmet` 而非逼模型修环境问题；**enforce 可执行命令只接受用户全局配置/内置夹具/明确受信算子配置**，workspace 项目配置、issue/PR 文本、模型生成文件默认只能 observe（由 `sanitized_for_source(trusted=false)→observe` 强制）。

### 5.4 层 2.5：stub/半成品门（规则独立的锚定）

这是 §4.2「规则独立」的直接实例，堵一种最常见的假完成：**项目能编译、`cargo build --release` exit 0、二进制也产出，但功能其实还是 stub。** 绿色构建掩盖了缺失实现，层 2 的 build/test 门**证明不了**这一点（stub 恰恰能编过）。

stub 门在「判完成」候选时、**先于**层 2/3 跑（因为是纯文件扫描、零命令执行——既然有 stub 就没必要再花分钟级跑 build 去「证明」一个掩盖了缺失功能的绿）。判定分两档严格区分以防误伤：

- **阻断级**（高信号「故意未完成」）：`todo!()`/`unimplemented!()`（Rust 宏，编过但运行即 panic）、`NotImplementedError`、携带 "not implemented" 的 `throw`/`panic!`/`raise`（语言无关）→ 命中即顶回。
- **仅记录**：裸 `TODO`/`FIXME` 注释（真实代码太常见，enforce 会误伤）→ 只进遥测计数，永不阻断。

### 5.5 层 3：纯机器交付物对账

层 3 解决「**压根没进 checklist 的交付物**」（即 §2.2 的假绿弱点、T3 的更一般形态）。关键决策：**层 3 不是 LLM headless runner，而是与层 2 同性质的纯 Rust 对账模块**——在 oracle 铁律下，交付物清单必须由算子离线翻译成一张显式 manifest，运行时只做存在性/glob 命中对账，LLM 没有任何「非自由裁量」的活可干。

层 3 在层 2 全绿后同轮执行：输入 = 交付物 manifest + 层 2 本轮 oracle 缓存；动作 = workspace 工作树路径/glob 存在性对账（仅 `tracked=true` 条目额外查 `git ls-files`）+ 可选 per-item verify；输出确定性 JSON `{pass, missing_deliverables[]}`。**它必须举出具体缺失项**（如「交付物 24：无 `router/trie.go`、git log 无 refactor commit」），而非「看起来不完整」。

### 5.6 有界返工与诚实耗尽

任一层未达标 → 把缺口作为合成 user 消息回灌 → 模型续做 → 再审。两套独立计数器（层 2 的 `manifest_gate_rounds`、层 3 的 `audit_rounds`）各自封顶；耗尽 → 记**诚实的 `audit_unmet`**（列出未达成门），**不假绿、不死循环**。返工优先级**高于** `NudgeTracker` 的 `blocked`（后者是「自驱续跑」的泄气阀，前者是「按规格必须达标」的硬约束）。

### 5.7 架构图

```
                 ┌─────────────────────────────┐
                 │ 层1: 模型自驱                 │
                 │ plan + checklist 自拆 + nudge │
                 └──────────────┬──────────────┘
                                │ graph.incomplete?
                  是 ◄──────────┤
              (续写/续命)        │ 否（判完成候选）
                                ▼
                 ┌─────────────────────────────┐
                 │ 层2.5: stub 门（纯扫描，先跑）  │ ──命中──► 返工
                 └──────────────┬──────────────┘
                                ▼
                 ┌─────────────────────────────┐
                 │ 层2: exit-code manifest       │ ──任一非0──► 返工
                 │ harness 主动真跑（operator    │
                 │ + 模型[verify:] + 工具链）     │
                 └──────────────┬──────────────┘
                                ▼ 全绿（同轮 trust cache）
                 ┌─────────────────────────────┐
                 │ 层3: 纯机器交付物对账           │ ──缺口非空──► 返工
                 │ 路径/glob 存在性，无 LLM       │
                 └──────────────┬──────────────┘
                                ▼ 通过
                          【真完成 done】
   任一层撞轮次上限 ──► 诚实 audit_unmet（列未达成门，不假绿不死循环）
```

> **组合性：** 只跑层 1 = 现状；层 1+2 = 纯 exit-code 门禁（无 LLM，最确定）；层 1+2+3 = 完整。算子按任务选组合。无 manifest 时行为与原系统**逐字节一致**（`is_active()=false`），不污染既有任务。

---

## 6. 实现（Implementation）

我们在 Zagens（一个基于 Rust 的桌面 Agent harness，内嵌运行时 sidecar）中实现了上述设计。完成门禁与长程机制集中在 `crates/runtime-server/src/long_horizon/` 模块，共 **23 个源文件、约 6000 行 Rust**，关键文件包括：

| 文件 | 职责 | 行数（约） |
|------|------|-----------|
| `nudge.rs` | NudgeTracker、续写消息模板、session 状态 | 843 |
| `manifest_gate.rs` | 层 2 主动执行验收 manifest | 566 |
| `mod.rs` | 续写入口 `maybe_continue_incomplete_code_task` | 495 |
| `generic_gate.rs` | 任务无关层 2 来源（`[verify:]` 提取/工具链探测/去重） | 474 |
| `stub_gate.rs` | 层 2.5 stub/半成品扫描 | 304 |
| `completion_audit.rs` | 层 3 纯机器交付物对账 | 284 |
| `deliverable_manifest.rs` | 交付物 manifest 解析与发现 | 274 |
| `graph.rs` | 任务图派生与完成判定（checklist 为权威，修复 T6） | 260 |
| `integration_gate.rs` / `generic_gate.rs` / `verify.rs` / `plan_drift.rs` … | 跨层集成门、verify 判定、plan 一致性等 | — |

**关键工程约束（与论文主张直接相关）：**

- **不给 Engine 加字段（架构冻结合规）：** 状态挂在 `EngineRuntimeExt.long_horizon_state`（`LongHorizonSessionState`），不破坏既有 Engine 结构。
- **续写不增加 step 预算：** 强制续写走 harness nudge 路径，不消耗模型的正常工具步预算（避免与 T4 的 step cap 相互踩踏）。
- **可离线回放：** 所有门禁决策（`continue_injected`/`gate_skip`/`verify_gate`/`manifest_gate_*`/`stub_gate`/`audit_unmet`…）既进 UI 面板的节点流，也以 `[lht-probe]` 探针写入 `sidecar.log`，可用一条 grep 按时序重放整条 harness 决策环——这是「机器 oracle 可离线回放」主张的落地保证。

模块自带单元测试（任务图的空/plan-only/checklist-only/全完成、NudgeTracker 的换项重置与封顶、`derive_objective` 的 fallback 链、`[verify:]` 提取/去重、工具链探测、stub 扫描、来源降级等），`long_horizon` 模块在最近一次记录中为 45 个测试全过。

---

## 7. 评估（Evaluation）

> **诚实声明:** 本节区分**已有的定性/案例证据**与**尚未完成的统计实验**。前者足以支撑「机制有效、修复正确」的定性结论；后者（多模型、多任务、足够样本、横向基线）是本工作公开承认的局限，列为后续工作（§8、§*TODO*）。

### 7.1 评估方法论：只靠 oracle 判终态

受 §2.3 非确定性约束，我们的判定准则**不看模型 prose、不做输出比对**，只看客观信号。一次测试**通过**定义为：

1. 所有 `[verify:]` gate 为 `verified`（无 `mismatch`/漏标）；
2. 进度图诚实 100%（`open_items=0`、`incomplete=false`）；
3. 抽查 `[verify:]` 之外的真实产物行为正确（如示例真能跑、取模真实现）；
4. 节点流里**不出现任何孤立的静默早停出口**（`incomplete_stop`/T1–T6 签名）。

### 7.2 案例证据：T3 假绿修复的因果链坐实

我们以 DEMO3（2 万行 Go Monkey 解释器）为锚点验证「验收语义塌缩」修复（一道软门禁：漏标 `[verify:]` 的可运行验收项**不放行收尾**，注入续写）。客观核验（只看产物 + `sidecar.log`）：

| 指标 | 修复前（基线） | 修复后 |
|------|--------------|--------|
| 端到端通过率 | **~62.5%**（8 次挂 3） | **5/5**（连跑 5 次真绿） |
| 失败签名 | 100% 同一根因：`unverified_acceptance` → `graph_complete` 假绿收尾 | 旧假绿出口 `graph_complete`/`gate_skip` **归零** |
| 续写因果链 | 无 | 每 run 各 1 次 `unverified_acceptance_nudge`（漏标→nudge→补 `[verify:]` 真跑→`verified`） |
| 钓鱼特性 | 4 示例崩 2（`%` 取模、`counter1` 词法器） | **5/5 全过** |

这条证据的价值在于**因果链完整**：不仅通过率提升，而且每一次提升都能在日志里看到「漏标被拦→注入续写→模型补验证→真跑通过」的确定性轨迹，而非「碰巧这几次跑对了」。

### 7.3 分类法的可观测性验证

DEMO4/DEMO6 的节点流进一步验证了多类出口的修复：DEMO6 复跑（同款 2 万行任务，45 分钟通过）观察到——`step_limit_continue open_items=10`（T4：撞满 100 步预算时正确续写而非停摆）、收尾 `gate_skip reason=graph_complete open_items=0`（T6 修复后 checklist 权威、0 未完成项正确放行）、**全程无 `incomplete_stop`**（T6 那个卡 61% 的假阳性不再出现）。

### 7.4 已知的评测边界（重要）

- **覆盖率类语义阈值，门禁拦不住。** DEMO6 中 `go test -cover` 命令 exit 0、但「每包 ≥80%」的人读阈值未满足——这是 exit-code oracle 的**固有边界**：它能确认「命令跑过且 exit 0」，不能判「≥80%」。治法是把阈值写进会非零退出的脚本/内置子命令（`coverage-gate`），让 exit code 真正反映阈值。
- **完成度上界 = manifest 完整度，非规格散文本身。** 本机制保证「manifest 内的交付物/门不被漏做」，但不解决「规格里有、manifest 没列」——那等于把「模型欠拆清单」平移为「算子欠写 manifest」。净收益在于 manifest 是规格的**离线、可评审、可回归、可复用**的人工子集，比模型每次临场自拆稳定得多。

### 7.5 *TODO* — 尚缺的统计实验（公开承认的局限）

为达到严谨实证论文标准，以下实验**尚未完成**，是本工作的明确后续：

1. **足够样本的通过率分布：** 当前最强证据 n=5、单任务族。需每配置 N≥20 次、报均值 ± 置信区间（受非确定性所迫，单点不可靠）。
2. **横向基线对比：** 与「裸 Agent 无 LHT」及主流开源 harness（SWE-agent / OpenHands / Aider 等）在同任务上的对比。
3. **消融实验：** 三层门 + stub 门各自的边际贡献（关掉某层通过率/假绿率如何变）。
4. **跨任务族泛化：** 已规划但未系统跑——CodeCrafters Redis（协议即契约）、SWE-bench Verified 子集（修复路径）、MicroStack（微服务框架、接口稳定性）。
5. **跨模型验证：** 当前仅 DeepSeek V4。需在不同模型上验证分类法与门禁的普适性。
6. **评测基建：** 目前缺 headless 批量跑 + 自动判 pass/fail；2 万行任务单次约 45 分钟，10 次约 7.5 小时，需更快的 proxy 任务把单次压到分钟级。

### 7.6 案例证据：LHT·strict + CRAFT + Audit 自发宏流程（LabelMakePro v2.67.1）

本节记录一次**非刻意编排**、却在真实桌面产品上完整跑通的端到端样本：用户措辞仅为「先仔细看下项目」，Composer 全局 **LHT·strict** 已开，工作区为第三方 Electron 单体仓库（`F:\label_rust`，LabelMakePro 单机版 ~2.67.1）。模型未收到「全库 audit」的显式指令，却在多机制叠加下**自发**跑完 inventory → 并行 Explore → 报告 → manifest 门禁 → 仓库修复，是 §5 组合门禁与 §3.4 不变式在**非 Go 解释器任务族**上的第一条完整轨迹。

#### 7.6.1 叠加条件（为何「看项目」变成审计）

| 层 | 条件 | 效应 |
|----|------|------|
| 用户措辞 | 「仔细看下项目」 | `base.md` 全库评审模式可被读成 repo-wide review |
| LHT·strict | Composer 全局开关 → `settings.toml` | `plan_gate`：≥3 次 tool 且无 plan → 强制 `checklist_write` / `update_plan` |
| Audit prompt | `base.md` + audit-repo skill | 强制 `scratchpad_init`、16 area inventory、`verified` finding 契约 |
| CRAFT | `agent_spawn(type=explore)` ×8 | Electron 四层 + 核心引擎 + 业务层并行读码 |
| 项目体量 | Electron 主进程 + Vue 前端 + 90+ IPC | 自然拆 16 inventory area，非 trivial graph |

缺任意一环，行为都会轻很多（见 §8.5 产品含义）。这不是 demo script，而是 **V4 + Zagens harness 在真实仓库上的 emergent macro workflow**。

#### 7.6.2 可观测产出（磁盘 + UI 对齐）

| 维度 | 结果 | 路径 / 签名 |
|------|------|-------------|
| Inventory | **16/16 done** | `.zagens/scratchpad/2026-06-02-full-audit/inventory.json` |
| Scratchpad notes | 35 行；**5** 条 `kind=finding` + `status=verified` | `.../notes.jsonl` |
| 并行子代理 | **15** 完成（Explore） | `.zagens/blackboards/2026-06-02-full-audit.json`；UI 子代理面板 ~1091s/路 |
| 审计报告 | 18 HIGH + ~50 MEDIUM + ~35 LOW | `doc/CODE_AUDIT_REPORT-v2.67.1.md`（2026-06-02） |
| Harness 驱动的仓库改动 | 根目录补 `test` 脚本 | `package.json`: `"test": "cd frontend && npm test"` |
| LHT 面板 | Task 100%；Nodes 见附录 C | `thread=thr_3658ee8d` `turn=turn_f4337847` |

报告中的 HIGH（路径遍历 ×3、SSRF、preload 混淆、授权绕过等）与 scratchpad 中 5 条 verified finding 一致；其余 HIGH 在报告正文展开，报告「验证声明」诚实标注部分 MEDIUM/LOW 仅来自子代理结论、未逐条独立验证——这与 §4.3「子代理只枚举、主代理须 read 验证」的纪律部分对齐、部分未完全达标，可作为后续 auditor 子代理强制化的动机。

#### 7.6.3 manifest_gate 两轮自愈（硬门禁的直接价值）

收尾时 **层 2 工具链门** `toolchain_npm_test`（`npm test --silent`）在 strict enforce 下拦截了假完成：

1. **Round 1 · failed** — 根 `package.json` 无 `test` 脚本 → exit 1 → `manifest_gate_result passed=false failing=toolchain_npm_test`。
2. **模型返工** — 添加 `"test": "cd frontend && npm test"` 委托 frontend vitest。
3. **Round 2 · passed** — 30 tests exit 0 → `gate_skip reason=graph_complete open_items=0` → turn **干净 Completed**。

若仅有模型 prose「审计完成」而无 manifest oracle，回合会在**根目录无法跑测**的假绿状态下结束。附录 C 给出 `[lht-probe]` 原文摘录。

#### 7.6.4 成本与模型选型（为何 DeepSeek V4 是 Zagens 的合适默认）

同一 harness 深度若换 **Claude Sonnet 4.6**（公开价 $3/M input · $15/M output），在 agent 典型 **80% input / 20% output** 混合下，**22 亿 token** 量级约 **$11,000–$15,000**（约 8–11 万人民币）；即使用全 input 下限亦 ~$6,600。实测 DeepSeek V4 压测段总花费约 **800 元**，隐含混合价 ~**0.36 元/百万 token**，约为 Sonnet 同工作量的 **1/80–1/130**。

| 指标 | 本 run 量级（压测段合计） | Sonnet 4.6 粗算（同 token） |
|------|---------------------------|-----------------------------|
| Token | ~22 亿 | 同左 |
| API 请求 | ~1.8 万（均 ~12 万 token/次） | 同左 |
| 费用 | ~800 元 | ~$11k–15k |

**结论（产品层，非论文主 claim）：** LHT strict + CRAFT + 全库 audit 是 **token 密集型**宏流程；在 Sonnet 档几乎只有团队/实验室用得起，在 V4 档个人开发者仍可把 harness **开满**做压测与日常长程任务——Zagens 专门适配 V4，不仅是 API 兼容，更是 **经济可行性**与 **全球调用量/生态**的联合选择。harness 提升的是「可控性与可观测性」，成本上界仍由模型单价决定。

#### 7.6.5 对本论文主张的印证与张力

**印证：**

- §3.4 不变式：manifest 门禁在 graph_complete 候选处**真拦**了未绿收尾（非 prose 自报）。
- §4.3：完成报告以 verified scratchpad + 主代理 `read_file` 为主；子代理 blackboard 仅辅助。
- §5.2–5.3：strict 下 `plan_gate` + toolchain 门 + 续写网**同时存活**，无需人工编排 Phase 4 宏观循环即可见到「LHT 段 + CRAFT Explore 段」组合。

**张力（诚实登记）：**

- **触发过宽：** 「看项目」→ 全库 audit，对用户意图是 feature 还是 bug 取决于交付价值；本例交付了报告 + test 修复，但 token 仍高。
- **verified 比例：** 18 HIGH 中仅 5 条进 scratchpad verified——报告体量 > 机器可回放证据量。
- **单样本：** n=1、单仓库、单模型；不能替代 §7.5 的统计实验。

---

## 8. 讨论与局限（Discussion & Limitations）

### 8.1 我们解决了什么

把「完成」从「模型自报清单清零」改写为「对规格化 manifest 的可验证达成」，并通过 §3.4 的不变式堵住多类静默早停出口。核心是**用独立的机器 oracle 替换自我确认的清单**——这在不微调模型、不引入 LLM 裁决的前提下，显著降低了长程任务的早停与假绿。

### 8.2 我们没解决什么

- **manifest 完整度依赖**（§7.4）：完成度上界由人工 manifest 决定，非规格散文本身。
- **语义阈值类验收**（§7.4）：exit-code 二值裁决表达不了「覆盖率 ≥80%」这类连续阈值，需算子把阈值写进脚本。
- **真正的「理解性」缺口**：正则覆盖不了、又没进清单的占位实现（如函数体只 `return Ok(())` 却无任何标记），需主体独立的对抗式审核员（§5.5，**待实现**）来枚举——但即便实现，它也只枚举、不裁决。

### 8.3 一条诚实的边界：harness 只管「情节记忆 + 外部锚定」

上下文管理 + 文档化记忆（cycle / handoff / memory）能维持注意力、保留经验，但**注意力窗口终会填满**，届时需要把知识参数化——那属于训练侧，harness 层够不到。本文方法只负责「情节记忆 + 外部锚定」这一半；参数化是另一条正交的轴。明确这条边界，是为了不把 harness 的作用夸大成「解决了长程问题」。

### 8.4 对评测方法论的一点主张

非确定性（§2.3）通常被视为麻烦，但我们认为它**强化**了本文主张：既然采样随机不可控、又被长程级联放大，那么任何依赖「输出一致」的评测都站不住，**唯一可靠的判定就是不随机的机器 oracle 判终态行为**。这与「让 LLM 当法官」在哲学上是对立的——后者把一个本就非确定的系统，用另一个非确定的系统去评判。

### 8.5 LHT·strict 作为「默认硬门禁网」的产品含义

Composer 全局 **LHT·strict**（`settings.toml` → 下轮 engine 生效）把长程纪律从「模型自觉规划后才有 LHT」升级为：**凡代码面 Agent 任务，空 plan 不得在实质 tool 活动后继续 freestyle；已开启的子 completion 门在 strict 下一律 `enforce`**（`strict_completion_gate()`）。再叠加 `base.md` 全库评审模式与 audit scratchpad，会在**未明确要求 audit** 的场景自发进入 macro workflow（§7.6）。

对 Zagens 产品：这是 **V4 单价 + harness 深度** 能同时成立的前提——用户用得起「开 strict 做长 refactor / 压测 / 偶发全库 audit」；换 Sonnet 档同深度需四个数量级预算（§7.6.4）。对论文：LabelMakePro run 证明组合门禁在 **Electron 第三方仓库**上可离线 grep 重放，补足了 DEMO3–6 以 Go 生成任务为主的证据偏倚。

---

## 9. 相关工作（Related Work，待补全引用）

- **编码 Agent 与 harness：** SWE-agent、OpenHands、Aider、Claude Code 等提供回合循环与工具执行，但「完成」多依赖模型自判或测试通过；本文聚焦于**完成判定的独立锚定**与**早停出口的系统化堵漏**。*[补具体引用与对比]*
- **自我改进 / 反思：** Reflexion 及后续工作让 Agent 基于反馈自我修正；本文论点是——若反馈信号本身不独立于模型，自我改进会退化为自我确认（§4.1）。*[补引用]*
- **LLM-as-a-judge 及其批判：** 大量工作用 LLM 评判 LLM；本文（§4.3）给出一个具体设定下的反对理由，并提出「缺口枚举器 vs 法官」的折中。*[补引用]*
- **长上下文与记忆管理：** 压缩、检索增强、检查点等；本文采用 cycle（检查点-重启）优于有损压缩的工程论证（§5.1）。*[补引用]*
- **持续学习 / grounding signal：** §4.1 的核心判断与该方向同构。*[补引用]*

> *注：本节引用待补。作为经验/系统论文，相关工作以「定位差异」为主，不求穷尽。*

---

## 10. 结论（Conclusion）

我们把长程编码 Agent 的**静默早停**刻画为一类（而非一个）问题，给出六类出口的失败分类法与「所有出口必须收敛到同一道完成闸门」的不变式；论证了**评估信号的质量与独立性比生成机制复杂度更关键**，并据此确立「LLM 只做缺口枚举器、不做法官」的铁律；最后给出一套**组合式完成门禁**的落地实现——三层 + stub 门 + 有界返工 + 诚实耗尽，全程机器裁决、可离线回放。在一个 2 万行级解释器任务上，修复关键假绿出口后端到端通过率从约 62.5% 提升到连续 5 次 5/5；在 LabelMakePro v2.67.1 第三方 Electron 仓库上，LHT·strict + CRAFT Explore + audit scratchpad 在一次「看项目」措辞下自发跑通 16/16 区域审计，manifest 工具链门两轮自愈后干净收尾（§7.6、附录 C）。

我们诚实地承认评测的局限（单模型、单任务族、样本小、缺基线），并把严谨的统计评测列为明确的后续工作。我们相信，本文的**问题分类法**与**设计原则**即便脱离具体实现也具有可迁移价值，可作为后续长程 Agent harness 设计与评测的参考。

---

## 附录 A — 复现与观测探针

所有门禁决策可经离线 grep 重放（PowerShell 示例）：

```powershell
$log = "$env:USERPROFILE\.zagens\logs\sidecar.log"
# 续写/放行/验证判定全流程
Select-String -Path $log -Pattern '\[lht-probe\].*long_horizon\.'
# 组合式门禁
Select-String -Path $log -Pattern 'manifest_gate_start|manifest_gate_result|completion_audit|audit_unmet'
# 假绿回归：不应在缺口未补前 graph_complete
Select-String -Path $log -Pattern 'gate_skip.*graph_complete'
```

## 附录 B — `[verify:]` 验收编写规范（防 T3 假绿）

```
案例名：<语言> 实现 <项目>
分解（模型自拆 checklist）：
  - [ ] 词法器 / 解析器 / 求值器
  - [verify: go build ./...] 编译通过
  - [verify: go test ./...] 单测通过
  - [verify: bash scripts/run_examples.sh] 全部示例脚本跑通  ← 关键：不是「创建示例脚本」
钓鱼点：<想压的阈值/漏洞，如「取模未实现假绿」「step 耗尽」>
```

三条铁律：①「创建文件」≠「验证通过」；② 单测必须覆盖钓鱼特性（否则绿得没意义）；③ plan 与 checklist 是一体工作量，不是两份。

## 附录 C — LabelMakePro 实证：gate 时间线与成本对照（2026-06-02）

**工作区：** `F:\label_rust`（LabelMakePro 单机版 v2.67.1）  
**日志：** `%USERPROFILE%\.zagens\logs\sidecar.log`  
**线程 / 回合：** `thread=thr_3658ee8d` · `turn=turn_f4337847`  
**Scratchpad run_id：** `2026-06-02-full-audit`

### C.1 关键 `[lht-probe]` 事件（节选，按时间序）

| 阶段 | 事件 | 载荷要点 |
|------|------|----------|
| 计划 bootstrap | `long_horizon.plan_gate` | `nudged:true round:1` — strict 下空 graph + 实质 tool 活动 |
| 审计脚手架 | （工具面）`scratchpad_init` | inventory 16 area；与 `checklist_write` 对齐 |
| 并行读码 | （子代理）Explore ×8 | blackboard `2026-06-02-full-audit.json` |
| 收尾 · 门 1 失败 | `manifest_gate_start` → `manifest_gate_result` | `toolchain_npm_test` exit 1（根目录无 test 脚本） |
| 返工 | （仓库）`package.json` | `"test": "cd frontend && npm test"` |
| 收尾 · 门 2 通过 | `manifest_gate_result` | `passed:true` · 30 vitest passed |
| 干净结束 | `gate_skip` | `reason=graph_complete` · `open_items=0` |

Round 1 失败行（原文）：

```text
[lht-probe] long_horizon.manifest_gate_result: {"passed":false,"failing_count":1,"manifest_round":1,
  "detail":{"failing_ids":["toolchain_npm_test"],"results":[{"id":"toolchain_npm_test",
  "command_display":"npm test --silent","exit_code":1,"exit_class":"assertion"}]}}
  thread=thr_3658ee8d turn=turn_f4337847
```

Round 2 通过 + 放行（原文）：

```text
[lht-probe] long_horizon.manifest_gate_result: {"passed":true,"failing_count":0,"manifest_round":2,...}
[lht-probe] long_horizon.gate_skip: {"reason":"graph_complete","open_items":0,...}
  thread=thr_3658ee8d turn=turn_f4337847
```

离线重放命令（PowerShell）：

```powershell
$log = "$env:USERPROFILE\.zagens\logs\sidecar.log"
Select-String -Path $log -Pattern 'thr_3658ee8d.*long_horizon\.(plan_gate|manifest_gate|gate_skip)'
```

### C.2 成本对照表（压测段合计 · 2026-05—06）

| 项目 | DeepSeek V4（实测） | Claude Sonnet 4.6（同 token 粗算） |
|------|---------------------|-------------------------------------|
| 总 token | ~2.2×10⁹ | 同左 |
| API 请求 | ~1.8×10⁴ | 同左 |
| 总费用 | ~800 CNY | ~$11k–15k USD（~8–11 万 CNY） |
| 混合单价 | ~0.36 CNY / M token | ~$5.4 / M token（80/20 in/out） |
| 倍数 | 1× | **~80–130×** |

Sonnet 单价来源：Anthropic 公开 API 价（2026-06），$3/M input · $15/M output；混合按 agent 典型 80% input / 20% output。Opus 4.8（$5/$25）同 token 约 **~$19.8k**。

### C.3 交付物清单（可复现检查）

```text
doc/CODE_AUDIT_REPORT-v2.67.1.md          # 主报告
.zagens/scratchpad/2026-06-02-full-audit/inventory.json
.zagens/scratchpad/2026-06-02-full-audit/notes.jsonl
.zagens/blackboards/2026-06-02-full-audit.json
package.json                               # + "test" script（manifest 门驱动）
```

---

*（草稿结束。下一步可做：补 §9 引用、补 §7.5 的统计实验、把 §7.6 扩为独立 case study 图、或把任一节译为英文。）*
