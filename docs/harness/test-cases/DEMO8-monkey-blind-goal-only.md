# DEMO8 — Monkey 平台「盲测」对比（仅目标 · 无过程）

**案例编号:** DEMO8  
**所属:** [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md)  
**对照:** [`DEMO7-monkey-platform-10k.md`](./DEMO7-monkey-platform-10k.md)（**过程显式** · 同一客观 oracle）  
**用途:** 在 **同一最终验收（DEMO7 §4 十二项 bash）** 下，比较各产品**自主拆解路径**、墙钟、上下文费用、对话假绿率——而不是比较「谁更会照抄步骤清单」。

---

## 0. 一句话

给三边（Zagens / OpenCode / Cursor）**同一段「只讲要什么、不讲怎么做」的 prompt**；**禁止**在 prompt 中出现阶段列表、`[verify:]`、脚本文件名、`loc_gate` 行数、testdata 个数等过程约束。跑完后由**同一人/同一脚本**执行 [DEMO7 §4](./DEMO7-monkey-platform-10k.md#4-验收-oracle唯一裁判) 作为**隐藏 oracle**。

> **盲测 ≠ 无验收。** 盲的是**模型可见的过程说明**；裁判仍是客观的。

---

## 1. 与 DEMO7 的分工

| 维度 | DEMO7（显式） | DEMO8（盲测） |
|------|---------------|---------------|
| Prompt | §1 逐字：阶段、脚本名、阈值、禁止项 | §2 逐字：**仅目标与完成语义** |
| 模型是否知道 `parity.sh` / `loc_gate` | **知道** | **不知道** |
| Oracle | §4 十二项 | **同一套 §4**（评测者保管，不发给模型） |
| 主要观测 | 能否按 harness 真绿 | **自主发现的验收结构**、缩水/假绿、墙钟、token |
| Zagens LHT | 通常 **Strict**（与 DEMO7 一致） | 见 §4.2（建议记录两档） |

**已有效实证（过程显式）：**

| 产品 | 目录 | 墙钟 | §4 oracle |
|------|------|------|-----------|
| Zagens | `F:\DEMO6-3` | 待补 | 11/12 |
| OpenCode | `F:\DEMO6-5` | **64 min** | **12/12** |

DEMO8 回答的是：**去掉「作业步骤」之后，差距是拉大还是缩小？**

---

## 2. 喂给三方的 prompt（逐字 · 禁止附加过程提示）

**实验员纪律：** 只粘贴本段。不得追加「请写 scripts/parity.sh」「请 go test」「请用子代理」等——除非三方产品**固定内置**且对所有参与者一致（须在记录表声明）。

```
在空目录用 Go 从零实现一门可运行的 Monkey 语言「平台」，语法可合理自定，但需自洽。

【必须达成的能力 — 只描述结果，不规定文件布局】

1. 执行方式
   - 能运行 .monkey 源文件；支持 REPL。
   - 同一程序必须能用两种实现执行：树遍历求值，以及「编译为字节码 + 虚拟机」；对同一输入，两种方式输出一致

2. 语言特性
   - 算术（含取模，除零为可报告错误）、字符串、数组、哈希、索引、闭包。
   - 标识符支持常见数字形式（如 counter1、x2）。
   - 循环：while，以及 break / continue。
   - 类与实例：可定义类、创建实例、方法调用、实例字段读写；方法内能指代当前实例（如 this）；不必实现继承。
   - 运行时错误（未定义名、类型错误、除零等）应打印可读错误信息，进程不得 panic。

3. 工具
   - 提供独立的格式化、静态检查（至少能发现部分语义/未定义问题）、以及字节码反汇编/调试视图三类能力，均以子命令或等价 CLI 暴露。
```

**相对 DEMO8 初稿，你已删掉（更「盲」）：** 参考书目、`go build`/`go test`/示例测试体量、README 自声明验收、末尾「不得省略双后端」— 隐藏 oracle 仍用 [DEMO7 §4](./DEMO7-monkey-platform-10k.md#4-验收-oracle唯一裁判)，更易观察谁自发补测试/parity/loc。

**刻意未写入（相对 DEMO7 §1 显式任务）：** `scripts/loc_gate.sh`、`≥10000` 行、`testdata` 数量、`coverage_gate` 60%、`parity.sh` 文件名、`[verify:]`、plan 阶段数、包名布局等。

**粘贴说明：** 首句原稿误写 `自洽）。`，存档版改为 `自洽。`；三方须用**同一段**原文。

---

## 3. 实验设置

### 3.1 目录与模型

| 项 | 要求 |
|----|------|
| 工作区 | **DEMO8 盲测实盘（2026-06-03）：** Cursor `F:\DEMO6-6` · Zagens `F:\DEMO6-7` · OpenCode `F:\DEMO6-8` |
| Prompt | §2 **逐字**，不附加 |
| 模型 | 记录名称与日期；三边尽量同模型族（如均 DeepSeek V4 Pro） |
| 种子 | **均不预置** DEMO7 的 `loc_gate.sh` / `coverage_gate.sh`（避免泄露过程） |

### 3.2 隐藏 oracle（仅评测者）

跑完后在产物根执行 [DEMO7 §4](./DEMO7-monkey-platform-10k.md#4-验收-oracle唯一裁判) 全部命令。

若产物**未生成**某脚本（如没有 `scripts/parity.sh`），对应项记 **N/A 或 fail**——这正是盲测要量的「自主验收成熟度」。

可选：将 DEMO7 的 `scripts/*.sh` 复制到产物根再跑（**探测模式 B**）——区分「实现够但缺脚本包装」vs「实现不够」；须在记录表注明是否启用 B。

### 3.3 Zagens 两档（建议都跑）

| 档位 | 设置 | 测什么 |
|------|------|--------|
| **A — 公平盲测** | Composer **LHT Off**（或 Auto，但不给 DEMO7 式 checklist 模板） | 与 OpenCode/Cursor 同条件「只有目标」 |
| **B — 产品默认** | LHT **Strict** + 默认 harness | 「用户只给目标时，Zagens 是否**自动**长出 verify/plan」——过程来自 harness 而非 prompt |

对比 DEMO7 时：**B 档** 可能仍注入 plan/checklist（来自 runtime，不是用户 prompt）——须在 sidecar 记录 `verify_gate` / `step_limit_continue`。

### 3.4 进行中快照

**开工（2026-06-03）**

| 目录 | 产品 | 约 `.go` 文件 / 行 | `.monkey` | 备注 |
|------|------|-------------------|-----------|------|
| `F:\DEMO6-6` | Cursor | 10 / ~2937 | 0 | 已有 ast…compiler 骨架 |
| `F:\DEMO6-7` | Zagens | 1 / ~93 | 0 | 刚起步 |
| `F:\DEMO6-8` | OpenCode | 0 / 0 | 0 | 盘空 |

**进行中更新（同日，三方均未收尾）**

| 目录 | `.go` / 行 | `.monkey` | `scripts/` | `examples/` | `testdata/` | 备注 |
|------|------------|-----------|------------|-------------|-------------|------|
| `F:\DEMO6-6` Cursor | 18 / **4308** | 4 | 无 | **有** | 无 | 有 `cmd`/`runner`/`disasm`；**C: 盘满后中断，会话「接着开始」续跑**（见下） |
| `F:\DEMO6-7` Zagens | 14 / **4642** | 2 | 无 | 无 | 无 | 核心包齐，尚未铺 examples 批量 |
| `F:\DEMO6-8` OpenCode | 14 / **4191** | 6 | 无 | 无 | 无 | 已追上；有 `repl` |

**共同阶段：** 均在堆 **Go 内核 + 工具包**；**尚无** `scripts/`、成体系 `testdata/`（DEMO7 盲测 oracle 的敏感项）。`loc_gate`（≥10k）目前 **Cursor/OpenCode 未达**，Zagens 约 **46%** 阈值。

**实验员：** 三方均只贴 §2 修订 prompt；跑完后对各自根目录执行 DEMO7 §4，勿中途泄露 oracle 脚本名。

**中断记录（公平性）：** Cursor（`F:\DEMO6-6`）因 **C: 磁盘耗尽** 停过一轮，后在**同目录续跑**（非空盘重开）。记录表须区分：

- **日历墙钟**：含等待清盘/重启 IDE 的时间（不宜单独用来比 Cursor vs 另两家）  
- **有效 Agent 墙钟**：仅计「会话内在跑」的分钟数（建议用户估填）  
- **中断次数**：Cursor **≥1**（C: full）；Zagens **UI/壳受影响**（§3.5）；OpenCode 壳未崩 — 详见 §3.8  

工作区已在 **F:**，续跑应以 `F:\DEMO6-6` 现有文件为准；注意 Cursor 缓存/临时目录默认可能在 **C:**（`%TEMP%`、`%USERPROFILE%\.cursor`），清空间或改临时目录，避免再次写满。

### 3.5 极端环境：磁盘满时的壳/UI 韧性（非 oracle · 实验员观察）

**场景：** 系统盘或 `%USERPROFILE%` 所在盘（`~/.zagens` / `~/.cursor` / 临时目录）耗尽。盲测长程中**极少**触发，但 DEMO8 已遇到一次（Cursor C: 满）。

**原则：** 界面**不应崩溃**；若 WebView 无法加载资源，应给出可读说明而非空白「页面不存在」；刷新/重连**不得**在 UI 已失联时让后台 Agent **静默继续扣费**。

| 产品 | UI/壳 | 恢复方式 | 后台回合（UI 失联后） | 实验员备注 |
|------|-------|----------|------------------------|------------|
| **OpenCode** | **未崩溃**（本轮最佳） | — | — | 极端态韧性最好 |
| **Cursor** | **崩溃** | **完全重启 IDE** 后正常 | 未细测刷新路径 | C: 满 → 中断后续跑（§3.4） |
| **Zagens** | **崩溃/整页不可用**（WebView 资源加载失败时常显示「页面不存在」） | **刷新**后界面可再进 | **刷新后 LHT/计划进度仍在跑**（UI 与 sidecar 脱钩，存在继续计费风险） | 与「流式假停、后台真跑」同类；需磁盘压力暂停 + 脱钩恢复（见 CHANGELOG `[Unreleased]` 磁盘压力项） |

**Zagens 产品目标（非 DEMO8 oracle）：**

1. **临界磁盘**（`~/.zagens` 或工作区盘 &lt;100MB）：自动 Stop 回合、禁止新发、顶栏告警。  
2. **脚本加载失败**：`index.html` 静态说明，避免裸 404。  
3. **刷新/重连**：与 runtime 脱钩时持久提示 + Stop；长时间离线自动 interrupt（已有流式恢复逻辑）。

本表**不计入** §4 十二项 pass/fail；写入 §4.1「中断/重连次数」与实验员备注即可。

### 3.8 C: 耗尽与盲测产物/验收的连带关系（实验员归纳）

本轮 **并非只有 Cursor「工作区在 F:」就免疫 C: 问题** — 三边工作区均在 `F:\DEMO6-*`，但 **IDE/壳、用户配置、临时目录、WebView 缓存** 默认仍写 **C:**。C: 满时会出现 **「F: 上代码还在长，C: 上壳/会话/脚本执行先死」** 的分裂态；下文用于解释 §3.6 扫盘差异与 §4 oracle 评测纪律。

| 路径类型 | 典型位置 | C: 满时可能影响 |
|----------|----------|-----------------|
| 工作区（源码） | `F:\DEMO6-6\|7\|8` | 一般仍可写（本轮 Go 内核多落在此） |
| Cursor 状态/缓存 | `%USERPROFILE%\.cursor`、`%TEMP%` | **Cursor 崩、中断、需重启**（§3.4） |
| Zagens 用户数据 | `%USERPROFILE%\.zagens`（sessions、logs、WebView） | **整页「页面不存在」**、刷新后 **sidecar 仍跑**、会话持久化失败风险 |
| OpenCode（本轮观察） | （产品自有缓存，多在 C:） | **壳未崩**；工作区收工正常（§4.5） |
| §4 oracle 脚本 | DEMO7 为 **`bash scripts/*.sh`** | 在 C: 紧张时 **Git Bash/WSL 临时文件** 可能失败；与盘上仅有 **`scripts/*.bat`**（Zagens）需区分 |

**按产品 — 与 C: 的关联度：**

| 产品 | 与 C: 耗尽关系 | 对「收工形态」的合理解释 |
|------|----------------|---------------------------|
| **Cursor** | **直接** — 已记录中断 | `F:\DEMO6-6` 有 `examples/` 但 **无收工**；日历墙钟不可与另两家比 |
| **Zagens** | **间接但显著** — UI/壳绑 C:；**后台 turn 可能仍在 F: 写盘** | 收工截图时盘上 **无** `scripts/`；终态扫盘出现 **`scripts/*.bat`×7** + 少量 `testdata/` → 疑为 **C: 恢复后或刷新续跑补写**，且用 **Windows bat** 规避 bash/临时目录写 C: 失败；**不是** prompt 里要求的 DEMO7 `.sh` 形态。§4.4「面板全绿」与终态文件 **时间线可能不一致** |
| **OpenCode** | **相对弱** — 未观察到壳崩 | 收工摘要完整、**无** `scripts/`、扫盘 **0** `.monkey`：更像 **未自发铺验收包装**，而非 C: 写不进 F:；Checklist 不实时（§3.7）**不宜归因于 C:** |

**评测纪律（避免误判能力）：**

1. **先清 C: 余量**（建议系统盘 &gt;5GB 空闲、`%TEMP%` 可写）再跑 DEMO7 §4 on `F:\DEMO6-7` / `F:\DEMO6-8`。  
2. §4 结果表备注列写：**「盲测期间曾 C: 满」** — 是/否 / 仅 Cursor / Zagens 也受影响。  
3. Zagens 仅 `.bat` 时：复制 DEMO7 `scripts/*.sh` 到产物根（**探测模式 B**）或在本机 bash 下重跑，**勿**把「只有 bat」当成盲测自发验收。  
4. 墙钟、token、oracle **x/12** 三方对比时，排除 Cursor 中断态；Zagens/OpenCode 若有效墙钟含 **等盘/刷新** 须单列。

> 用户归纳：**当前 Zagens/OpenCode 扫盘差异（bat vs 无 scripts、testdata 体量、§4.4 与终态不一致）与早前 C: 耗尽「有点关系」** — 记入本节；**不替代** §4 客观裁判。

### 3.6 收工状态（2026-06-03 · 实验员）

| 产品 | 目录 | 会话状态 | 产物扫盘（收工/中断时） |
|------|------|----------|-------------------------|
| **Zagens** | `F:\DEMO6-7` | **已完整结束** | 15 `.go` / **4284** 行 · 7 `.monkey` · **`scripts/*.bat`×7** · `testdata/`×4 · 无 `examples/` · `monkey.exe` |
| **Cursor** | `F:\DEMO6-6` | **中断**（C: 盘满；未宣告收工） | 18 `.go` / **3946** 行 · 4 `.monkey` · 有 `examples/` · 无 `scripts/` / `testdata/` |
| **OpenCode** | `F:\DEMO6-8` | **已完整结束**（15 sub-tasks · 会话收尾摘要） | 14 `.go` / **3927** 行 · **0** `.monkey`（扫盘） · 无 `scripts/` / `examples/` / `testdata/` · **`repl/`** · `monkey.exe` |

**下一步（评测员）：**

1. 对 **`F:\DEMO6-7`**、**`F:\DEMO6-8`** 各执行 [DEMO7 §4](./DEMO7-monkey-platform-10k.md#4-验收-oracle唯一裁判)（隐藏 oracle），填 §4.1。Zagens 若仅有 `scripts/*.bat`，须注明是否复制 DEMO7 `scripts/*.sh` 或探测模式 B。  
2. Cursor 若续跑或冻结为「中断交付」，在 §4.1 注明 **未完成** 与有效墙钟。

> 扫盘行数为 `*.go` 物理行（PowerShell `Get-Content | Measure-Object -Line`），**不等于** `loc_gate`（≥10k）；oracle 仍以 §4 脚本为准。

### 3.7 右栏进度可观测性（非 oracle · 实验员）

长程盲测时，用户能否**边跑边看**任务拆解进度，影响「假绿」判断与中断决策（与 §4 oracle 独立）。

| 产品 | Checklist / 任务清单 | Plan / 长程图 | 实验员备注（DEMO8） |
|------|----------------------|---------------|---------------------|
| **Zagens** | **实时更新**（SSE/轮询；收工截图 checklist 11/11 与对话同步） | LHT Task/Plan/Nodes **随回合推进**（§4.4 manifest 时间戳连续） | 右栏可作为进行中「真进度」参考；聊天流曾出现 desync，与 checklist 不同轨 |
| **OpenCode** | **非实时** — Checklist **不随工具步即时刷新** | （待收工补记） | 进行中观察：**后台仍在写盘，但 Checklist 滞后/静止**，易误判「卡住」或错过中间里程碑 |
| **Cursor** | （待补） | （待补） | 本轮重点为 C: 中断；未系统记录 checklist 刷新策略 |

**记录表用途：** §4.1「对话宣称完成 vs oracle」与 **面板是否实时** 应分开写 — OpenCode 可能 **oracle 强 + 面板钝**，Zagens 可能 **面板绿 + oracle 体量项红**（§4.4）。

---

## 4. 记录表（跑完填）

### 4.1 主表

| 维度 | Zagens (`F:\DEMO6-7`) | OpenCode (`F:\DEMO6-8`) | Cursor (`F:\DEMO6-6`) |
|------|------------------------|-------------------------|------------------------|
| 目录 | `F:\DEMO6-7` | `F:\DEMO6-8` | `F:\DEMO6-6` |
| LHT 档位 | （跑时记录 Off/Strict/Auto） | N/A | N/A |
| 状态 | **已结束** | **已结束**（§4.5） | **中断**（C: 满；未收工） |
| 墙钟 | （待填） | （待填） | （待填；区分日历 vs 有效） |
| §4 oracle（12 项 pass 数） | **4/12 严格**（§4.6；探测 B） | **待跑 §4** | 待续跑/冻结后跑 |
| 对话宣称完成 vs oracle | **宣称完成**；§4 **4/12**（§4.6） | **宣称完成**（15/15 sub-tasks）；oracle **待跑** | |
| 上下文占用（UI % / tok） | | | |
| 是否自建 parity/等价物 | | | |
| 是否达 loc≥10k（oracle） | **预判 N**（扫盘 ~4284） | **预判 N**（扫盘 3927） | |
| vm coverage≥60%（oracle） | | | |
| 子代理/并行铺量 | | | |
| 中断/重连次数 | **UI/壳**（C: 关联 §3.8） | 壳未崩 | **≥1（C: 满 · 中断）** |
| 盲测期间 C: 耗尽影响 | **Y**（壳/续跑/bat） | **弱/无**（壳稳） | **Y**（直接中断） |
| 磁盘满时 UI 崩溃（Y/N） | Y（刷新可进；后台仍跑） | **N** | **Y**（需重启） |
| 有效墙钟（不含等盘） | | | |
| Checklist/进度面板实时更新 | **Y**（§3.7） | **N**（§3.7） | （待补） |

### 4.4 Zagens 收工 UI 证据（`F:\DEMO6-7` · 盲测 · 2026-06-03）

**会话收尾（截图）：** 用户问「结束了吗」；助手 **宣称已完成**（checklist **100%**、plan **11/11**、`go build` / `go test` 通过、双引擎一致、fmt/lint/disasm 子命令可用）。工具链显示 **64+** 次工具调用（含 `list_dir`、`update_plan` 等）。

**右栏 LHT（截图）：**

| 面板 | 状态 |
|------|------|
| **Checklist** | 11 项全勾（含 evaluator/compiler/vm、formatter/checker/disassembler、main CLI、`[verify: go build ./...]`、`[verify: go test ./...]`、双引擎 e2e） |
| **Long-horizon · Task** | **100%** · 0 open · 1 nudge |
| **Plan** | 11 步全绿（scaffold → lexer → AST → parser → object → tree eval → bytecode → compiler → VM → CLI tools → main） |
| **Nodes** | `verify_gate` → `manifest_gate` **round 1/2** 均 `passed=true` · `failing_count=0` · `gate_skip`（`graph_complete` · `open_items=0`） |

**与产物扫盘（§3.6）对齐 — 盲测「假绿」预警：**

| 维度 | UI / harness | 磁盘 `F:\DEMO6-7` |
|------|----------------|-------------------|
| 完成宣称 | **是** | 15 `.go` · **4282** 行（≪ loc_gate 10k） |
| `[verify:]` | checklist 内 **自发** `go build` / `go test` | 须 §4 复跑确认 exit 0 |
| `scripts/` · parity · loc · coverage | 未在 UI 暴露 DEMO7 脚本名 | 终态有 **`scripts/*.bat`** · `testdata/`×4（§4.4 截图时点可能更早） |
| 对话 vs oracle（预判） | 面板全绿 | **待 §4**；参照 DEMO7 显式 Zagens **11/12**，盲测无体量提示时 **loc/coverage/parity 项大概率红** |

**实验员待填：** LHT 档位（Off/Strict）、墙钟、上下文 %、`sidecar.log` 路径；在 `F:\DEMO6-7` 跑完 DEMO7 §4 后把 §4.1 的 oracle 列从「待跑」改为 **x/12** 并更新上表最后一行。

> **扫盘更新：** 收工后 Zagens 盘上出现 `scripts/*.bat`（7 个）与 `testdata/`（4 个 `.monkey`），与 §4.4 截图时点可能不一致 — 见 **§3.8（C: 满 → 续跑/补写 bat）**；oracle 以**终态目录**为准，§4 前应先清 C:。

### 4.5 OpenCode 收工摘要（`F:\DEMO6-8` · 盲测 · 2026-06-03）

**会话收尾（用户粘贴）：** **All 15 sub-tasks completed.** 宣称 Monkey 平台在 `F:\DEMO6-8` 已交付。

**架构（14 包 + `monkey.exe`）：** `token` · `lexer` · `ast` · `parser`（Pratt）· `object` · `evaluator` · `code` · `compiler`（+ symbol table）· `vm` · **`repl/`** · `formatter` · `checker` · `disassembler` · `main.go`。

**CLI（对话摘要）：**

| 子命令 | 说明 |
|--------|------|
| `monkey run [-vm] <file>` | 默认 tree-walk；`-vm` 走 compiler+VM |
| `monkey repl [-vm]` | REPL |
| `monkey fmt` | 格式化 |
| `monkey check` | 静态检查（未定义、shadowing、类型） |
| `monkey disasm` | 反汇编 |
| `monkey debug` | tokens / AST / bytecode 调试视图（**超出 §2 最小工具集**） |

**语言（宣称）：** 算术/字符串/数组/哈希/闭包 · while+break/continue · class+`this`（无继承）· 可读运行时错误 · **tree 与 VM 同输入同输出**。

**与 Zagens 盲测对照（扫盘 · 非 oracle）：**

| 维度 | OpenCode `DEMO6-8` | Zagens `DEMO6-7` |
|------|-------------------|------------------|
| Go 行数（扫盘） | **3927** | **4284** |
| `repl` 独立包 | **有** | 无（REPL 在 main） |
| 静态检查 CLI 名 | `check` | `lint`（Zagens 对话） |
| `scripts/` | **无** | **有**（`.bat`×7，非 DEMO7 `.sh`） |
| `.monkey` 样例（扫盘） | **0** | 7 |
| Checklist 实时性 | **N**（§3.7） | **Y** |
| UI 壳（磁盘满） | **未崩** | 崩/刷新后台仍跑 |

**oracle 预判：** 与 Zagens 类似，**能力型**项（build、双引擎、fmt/check/disasm）可能绿；**DEMO7 §4 体量项**（`loc_gate`≥10k、`coverage_gate`、bash `scripts/*.sh`、`testdata/programs`≥50）在盲测无提示下 **大概率红**，除非收工后大量补测试（DEMO7 显式 OpenCode 曾 **12/12** 靠后期铺量）。

**实验员待填：** 墙钟、上下文 %；`F:\DEMO6-8` 跑 §4 后填 **x/12** 与「对话 15/15 vs oracle」是否一致。

### 4.6 Zagens 第二轮补验 vs DEMO7 §4 真 oracle（`F:\DEMO6-7` · 2026-06-03）

**背景：** C: 满后 Agent 在 Windows 上补了 `scripts/*.bat` 并自跑「Pipeline」；用户粘贴的 **11 项全 PASS** 来自 **弱化门禁 + 非 DEMO7 路径**，不能当作盲测 oracle 成绩。

#### Agent 自报（对话）摘要

| # | Agent 声称 | 备注 |
|---|------------|------|
| 1–4 | build / vet / gofmt / go test | 与官方前几项一致 |
| 5–8 | run_examples / conformance / parity / run_testdata | 用 **bat** 或手跑；**非** `bash scripts/*.sh` |
| 9 | loc_gate **PASS 4284**（threshold **500**） | `loc_gate.bat` 门槛为 **500 行**，非 DEMO7 **10000** |
| 10 | toolchain PASS | 未要求 `examples/` 三文件 |
| 11 | coverage_gate PASS **lexer 93.6%** | `coverage_gate.bat` 仅 `go test -cover` 出报告，**未**查 evaluator/compiler/vm **≥60%** |
| 修复 | `<=` 编译反了导致 factorial VM=1 | 合理 bugfix；须在 **官方 parity** 上复验 |

#### 评测员复跑：DEMO7 §4（探测模式 B）

从 `F:\DEMO6-3` 复制官方 `scripts/*.sh` 到 `F:\DEMO6-7/scripts-sh/`，在 Git Bash 下对 **终态产物根** 执行（与 §4 原文一致；**不**使用 `.bat`）。

| # | DEMO7 §4 命令 | exit | 说明 |
|---|---------------|------|------|
| 1 | `go build ./...` | **0** | |
| 2 | `go vet ./...` | **0** | |
| 3 | `gofmt -l .` 空 | **0** | |
| 4 | `go test ./...` | **0** | 仅 **lexer** 有测试；evaluator/compiler/vm **无** `_test.go` |
| 5 | `bash scripts/run_examples.sh` | **1** | **无 `examples/`** 目录（无 `.expected`） |
| 6 | `bash scripts/conformance.sh` | **0** | 实质为 build+vet+gofmt 再验 |
| 7 | `bash scripts/parity.sh` | **0*** | *无 `examples/*.monkey` 时 glob 空转，**未**覆盖 testdata；不可视为 20/20 parity |
| 8 | `bash scripts/coverage_gate.sh` | **1** | evaluator **0%** · compiler **0%** · vm **0%**（均 &lt;60%） |
| 9 | `bash scripts/run_testdata.sh` | **0*** | *脚本对缺失 `testdata/programs/` **SKIP exit 0** — 盲测应记 **未达标**，非真绿 |
| 10 | `bash scripts/loc_gate.sh` | **1** | **4856** 行（阈值 **10000**） |
| 11 | `bash scripts/toolchain.sh` | **1** | 依赖 `examples/arithmetic.monkey` 等，**目录不存在** |

**§4 严格计分：4/12 明确通过**（1–4）；若把 6 算作重复则 **5/12**；7/9 的「0」为脚本空洞通过，**不计入真绿**。与 DEMO7 显式 Zagens **11/12**（`F:\DEMO6-3`）相比，盲测终态 **体量与测试包装显著缩水**。

**手验（补充）：** `testdata/example1` 等 4 文件在根下 `testdata/`（非 `testdata/programs/`）；Agent bat 用 `findstr` 子串匹配，**不等于** DEMO7 `run_testdata` 语义。

#### 与「对话宣称完成」对照

| 来源 | 结论 |
|------|------|
| LHT checklist 100% · manifest 绿（§4.4） | harness **过程** 真绿 |
| 第二轮 Pipeline 表 11/11 | **假绿**（弱化 bat 门槛） |
| DEMO7 §4 官方脚本（本节） | **4–5/12** 真绿；**loc / coverage / examples / testdata 未达标** |

**记入 §4.1：** Zagens 盲测 oracle **4/12（严格）** 或 **5/12（含 conformance）**；待用户确认墙钟/LHT 档位后定稿。

### 4.2 过程发现（盲测独有）

| 观测 | Zagens | OpenCode | Cursor |
|------|--------|----------|--------|
| README 自声明验收命令列表 | （待查） | （待查） | |
| 与 DEMO7 §4 重合的命令数 | **4/12**（§4.6） | **待 §4**（无 scripts） | /12 |
| 「创建脚本」但未可执行（塌缩） | `.bat` 待 §4 验 | 无 scripts | |
| 仅 tree、无 vm（钓鱼） | 宣称双引擎；§4 待验 | 宣称双引擎；§4 待验 | |
| 示例/测试体量自发接近 DEMO7 阈值 | **否**（扫盘 &lt;5k 行） | **否**（3927 行 · 无 testdata） | |
| 进行中 Checklist 是否实时刷新 | **Y** | **N**（不随步更新） | |
| 子任务/计划粒度（对话） | 11 步 plan + checklist | **15 sub-tasks** | |

### 4.3 假设（待 DEMO8 验证）

- **OpenCode：** 仍可能 **12/12 或接近**（成熟 Agent 会自发 parity + 厚测试），墙钟/上下文优势保持。  
- **Cursor：** 更易 **loc 或 coverage 红** 但 prose 完成（对话假绿）。  
- **Zagens A：** oracle 可能 **≤ DEMO7 的 11/12**（无过程提示时漏 coverage/testdata）。  
- **Zagens B：** harness 可能 **补上 verify**，oracle **接近 DEMO7**，但上下文与墙钟上升。

---

## 5. 评测员 checklist（实验前后）

- [ ] 三方 prompt 均为 §2 原文（截图或导出存档）  
- [ ] 未在对话中补「记得写 loc_gate」类提示  
- [ ] 产物完成后**再**跑 DEMO7 §4（或注明探测模式 B）  
- [ ] 记录对话收尾表与 oracle 是否一致  
- [ ] Zagens 注明 A/B 档位与 `sidecar.log` 路径  

---

## 6. 与产品叙事

| 叙事 | 支持数据 |
|------|----------|
| 「成熟 Agent IDE 效率高」 | DEMO7 显式 + DEMO8 盲测墙钟/token（OpenCode 64min / 11%） |
| 「Zagens 降低假绿」 | DEMO8 对话 vs oracle；B 档 `verify_gate` 日志 |
| 「给目标即可，不必写作业步骤」 | DEMO8 若 B 档 oracle ≥ A 档且接近 OpenCode → harness 价值 |

---

**修订记录:**

- 2026-06-03 创建：DEMO8 盲测规格（goal-only prompt + 隐藏 DEMO7 §4 oracle + Zagens A/B 档 + 记录表）。
- 2026-06-03 §2：采用用户修订 prompt（仅 §1–3 能力；去掉规模/README/双后端警示句）。
- 2026-06-03 §3.4：DEMO8 实盘目录 `F:\DEMO6-6`（Cursor）/ `F:\DEMO6-7`（Zagens）/ `F:\DEMO6-8`（OpenCode）开工快照。
- 2026-06-03：Cursor `F:\DEMO6-6` 因 C: 盘满中断后同目录续跑 — 记录表区分日历墙钟 vs 有效墙钟。
- 2026-06-03 §3.5：磁盘满时 UI 韧性对比（OpenCode 未崩 / Cursor 崩需重启 / Zagens 崩可刷新但后台仍跑）。
- 2026-06-03 §3.6：收工快照 — Zagens **已结束**（`F:\DEMO6-7`）；Cursor **中断**；OpenCode **进行中**。
- 2026-06-03 §4.4：Zagens 收工 UI 截图归档（checklist 11/11 · LHT 100% · manifest_gate 绿 · 对话宣称完成 vs 扫盘 4282 行）。
- 2026-06-03 §3.7：OpenCode Checklist **非实时**更新；Zagens 右栏实时（对照 §4.4）。
- 2026-06-03 §4.5：OpenCode `F:\DEMO6-8` 收工（15 sub-tasks · 14 包 · `monkey.exe`）；§3.6 终态扫盘更新；Zagens 终态含 `scripts/*.bat`。
- 2026-06-03 §3.8：C: 耗尽与工作区 F: 分裂态 — 关联 Cursor 中断、Zagens 壳崩/续跑/bat 补写、§4 评测前先清 C:。
- 2026-06-03 §4.6：Zagens 第二轮 bat 自验 11/11 vs DEMO7 §4 官方复跑 **4/12**（`F:\DEMO6-7`）。
