# DEMO7 — Monkey 平台化（DEMO6 超集 · 目标 ≥10k 行 Go）

**案例编号:** DEMO7
**所属:** [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md)（超长程 · 代码量闸门 · 多子系统）
**前置:** [`DEMO6-monkey-dual-backend.md`](./DEMO6-monkey-dual-backend.md)（双后端 + parity + DEMO3 钓鱼特性）
**实证基线:** DEMO6 对比跑约 **3.6k–4.3k** 行 Go（`F:\DEMO6-1` / `F:\DEMO6-2`），未达 1 万行；本案例用**可执行 LOC 闸门 + 语言/工具链扩容**把体量推到 **10k+**。
**用途:** 压 **step 耗尽续写、cycle、manifest_gate、验收塌缩**；与 Cursor 对比时更能看出「长程未跑完却 Completed」与「只实现 tree 未实现 vm/类/工具」的假绿。

---

## 0. 一句话

在 **DEMO6 全部要求不变**的前提下，把 Monkey 做成「带 **类与实例**、**while/break/continue**、**20+ 内建**、**formatter/linter/disasm 子命令**、**≥50 个 testdata 程序**」的平台级仓库，并用 **`scripts/loc_gate.sh`（Go 源码合计 ≥10000 行，不含空行注释可配置）** 作为硬 oracle——避免「功能宣称完成但代码量只有 DEMO6 体量」的缩水交付。

> **体量设计：** 不是空洞堆文件，而是 **tree + vm 各实现一遍新语义** + **独立 fmt/lint 包** + **大体量 Go 测试**（`testdata/` 驱动），自然落到 10k–15k 行。

---

## 1. 喂给 runtime 的 prompt（逐字）

```
在空目录用 Go 实现「Monkey 平台」：在 DEMO6 规格之上交付（不要删减 DEMO6 已有能力）。

=== DEMO6 基线（必须全部保留）===
- 双后端：tree-walking（evaluator/）+ 字节码（compiler/ + vm/ + code/）。
- CLI：./monkey run --engine=tree|vm <file>；无参数 REPL（tree）。
- 扩展：% 取模（除零报错）、数字标识符 counter1/x2/_tmp3、字符串/数组/哈希/索引、闭包。
- scripts：run_examples.sh（每个 examples/*.monkey 对 tree 与 vm 各跑并比对 .expected）、conformance.sh、parity.sh、coverage_gate.sh（evaluator/compiler/vm 三包覆盖率各 ≥60%）。
- 全部 DEMO6 验收命令保持 exit 0。

=== DEMO7 新增语言特性（tree 与 vm 语义必须一致，parity 仍适用）===
1. 循环：while (<cond>) { ... }；break; continue;（在 while 或 fn 内）。
2. 类与实例（参考《Crafting Interpreters》风格，不必实现继承）：
   - class Name { fn method(...) { ... } }  或等价语法；
   - 实例化：let o = Name{} 或 Name()；
   - 方法调用：o.method(args)；实例字段 o.field / o.field = expr；
   - 方法内 this 指向当前实例。
3. 错误处理：除零、未定义变量/字段、调用非函数，均返回可打印 ERROR 行（进程不 panic）。

=== DEMO7 新增内建（至少再实现 12 个，两后端一致）===
在 len/puts/first/last/rest/push 之外至少包含：
type、str、int、string (类型转换)、join、split、push（已有可复用）、pop、set（改数组/哈希元素）、keys、values、contains、range（对数组/哈希产生可迭代，若语言层不好做可简化为内置高阶：map/filter 二选一实现即可）。

=== DEMO7 工具链子命令（独立包，须有 _test.go）===
- ./monkey fmt <file.monkey>   — 实现于 fmtmonkey/ 或 cmd/fmt，格式化后输出到 stdout；非法语法 exit 1。
- ./monkey lint <file.monkey>  — 实现于 lintmonkey/ 或 cmd/lint，至少检查：未定义标识符、class 外非法 this；发现问题 exit 1 并打印诊断。
- ./monkey disasm <file.monkey> — 编译为字节码并人类可读反汇编（cmd/disasm 或 debug/ 包）；exit 0 输出非空。

=== DEMO7 测试与示例体量===
- examples/：在 DEMO6 的 12 个基础上再至少新增 8 个 .monkey（合计 ≥20），覆盖 while、class、新内建；每个有 .expected。
- testdata/programs/：至少 50 个小型 .monkey（可无 .expected），由 Go 测试用 golden 或内嵌期望驱动；go test 必须全部跑过。
- testdata/invalid/：至少 10 个应解析/编译失败的 .monkey，lint 或 run 必须报错。
- Go 测试：除 DEMO6 外，新增测试文件合计至少 2500 行（物理行，可含 _test.go）；必须覆盖 %、counter1、while、class、至少 6 个新内建。

=== DEMO7 脚本（禁止塌缩为「创建脚本」）===
在 DEMO6 四套脚本之外新增：
- scripts/run_testdata.sh — 对 testdata/programs/*.monkey 用 tree 与 vm 执行，失败即 exit 1。
- scripts/loc_gate.sh — 统计仓库内 *.go 行数（不含 vendor），若合计 < 10000 行则 exit 1 并打印实际行数。
- scripts/toolchain.sh — 依次：monkey fmt、monkey lint、monkey disasm 对至少 3 个示例文件执行且 exit 0。

=== 完成标准（全部真实执行，exit 0）===
go build ./...、go vet ./...、gofmt 干净、go test ./...、
bash scripts/run_examples.sh、bash scripts/conformance.sh、bash scripts/parity.sh、bash scripts/coverage_gate.sh、
bash scripts/run_testdata.sh、bash scripts/loc_gate.sh、bash scripts/toolchain.sh

不要只在 tree 实现 class/while；不要跳过 parity；不要用注释块或生成无用文件凑行数（loc_gate 会人工抽查结构）。
```

---

## 2. 期望代码量分解（规划用，非模型 prose 验收）

| 子系统 | 目标行数（Go，含测试） | 说明 |
|--------|------------------------|------|
| DEMO6 核心（lexer…vm） | ~4,000 | 与 DEMO6 实证同量级 |
| class / while / break（×2 后端） | ~2,200 | /parser + evaluator + compiler + vm 同步 |
| 扩展内建 + object 类型 | ~700 | 新运行时对象、哈希/数组辅助 |
| fmtmonkey + lintmonkey + disasm | ~1,400 | 三工具 + 测试 |
| testdata 驱动测试 | ~2,500+ | `*_test.go` 中表驱动 ≥50 程序 |
| cmd/、repl 增强、文档常量 | ~400 | |
| **合计** | **≥11,000** | `loc_gate` 硬门槛 **≥10,000** |

---

## 3. 建议 checklist（约 30–40 项，便于压 step 预算）

**阶段 A — DEMO6 基线（1–16）**  
与 [DEMO6 §2](./DEMO6-monkey-dual-backend.md) 相同，含 8 条 `[verify:]`（build/vet/gofmt/test/run_examples/conformance/parity/coverage_gate）。

**阶段 B — 语言扩容（17–24）**  
while/break/continue；class/this/方法/字段；新内建 ≥12；examples +8；`[verify: bash scripts/parity.sh]` 再跑（类与循环加入后）。

**阶段 C — 工具链（25–28）**  
fmt / lint / disasm 三包；`[verify: bash scripts/toolchain.sh]`。

**阶段 D — 体量与 testdata（29–32）**  
testdata/programs ≥50、invalid ≥10；Go 测试 +2500 行；`[verify: bash scripts/run_testdata.sh]`；`[verify: bash scripts/loc_gate.sh]`。

**收尾**  
`[verify: go test ./...]`（全量回归）。

> **plan 提示（防 DEMO5 双计数）：** 固定 4 个高层 plan 阶段对应 A/B/C/D，checklist 细项往四阶段下挂，不要 12 个 plan 项全程 pending。

---

## 4. 验收 oracle（唯一裁判）

```bash
set -euo pipefail
cd <产物根>

# DEMO6 基线
go build ./...
go vet ./...
test -z "$(gofmt -l .)"
go test ./...
bash scripts/run_examples.sh
bash scripts/conformance.sh
bash scripts/parity.sh
bash scripts/coverage_gate.sh

# DEMO7 增量
bash scripts/run_testdata.sh
bash scripts/loc_gate.sh
bash scripts/toolchain.sh
```

**全部 exit 0** ⇒ 真绿。`loc_gate` 失败 ⇒ 代码量未达标（即使 `go test` 绿）。

### 4.1 `scripts/loc_gate.sh`（可预制进空目录 seed）

```bash
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MIN_LINES=10000
count="$(find "$ROOT" -name '*.go' ! -path '*/vendor/*' -print0 | xargs -0 wc -l | tail -1 | awk '{print $1}')"
echo "Go line count: $count (minimum $MIN_LINES)"
if [ "$count" -lt "$MIN_LINES" ]; then
  exit 1
fi
```

### 4.2 钓鱼快检（DEMO7 特有）

```bash
# 类 + vm 必须同时过
./monkey run --engine=vm testdata/programs/class_method.monkey
./monkey run --engine=tree testdata/programs/class_method.monkey
bash scripts/parity.sh

# while + break
./monkey run --engine=vm examples/while_break.monkey

# 工具链非空
./monkey disasm examples/modulo.monkey | head -5
./monkey lint testdata/invalid/undefined_var.monkey ; test $? -ne 0
```

### 4.3 体量与 DEMO6 对比记录表

| 指标 | DEMO6 实证 | DEMO7 目标 |
|------|------------|------------|
| Go 总行 | ~3.6k–4.3k | **≥10k** |
| examples | 14 | **≥20** |
| testdata/programs | 0 | **≥50** |
| 子命令 | run/repl | **+fmt +lint +disasm** |
| 典型墙钟 | ~20–40 min | **~60–120 min**（易触 step_limit_continue） |

### 4.4 Zagens 实证记录（`F:\DEMO6-3`，2026-06-03 人工 oracle）

工作区为 DEMO6 对比目录续跑 DEMO7 规格（线程曾中断重连；右栏 LHT/Checklist 曾显示 100%）。**裁判仍为 §4 脚本，非 UI 面板。**

#### Oracle §4（12 项）

| 命令 | exit | 备注 |
|------|------|------|
| `go build ./...` | 0 | |
| `go vet ./...` | 0 | |
| `gofmt -l .` | 0 | 干净 |
| `go test ./...` | 0 | 含 `code_test.go` 集成测 |
| `bash scripts/run_examples.sh` | 0 | **40/40**（20×tree+vm） |
| `bash scripts/conformance.sh` | 0 | |
| `bash scripts/parity.sh` | 0 | **20/20** |
| `bash scripts/coverage_gate.sh` | **1** | evaluator **78.8%**、compiler **74.1%**、**vm 7.9%**（&lt;60%） |
| `bash scripts/run_testdata.sh` | 0 | **55/55**（本地逐文件 vm 亦 0 失败） |
| `bash scripts/loc_gate.sh` | 0 | **10014** 行（阈值 10000，余量 **+14**） |
| `bash scripts/toolchain.sh` | 0 | fmt/lint/disasm 三例 |

**§4 合计：11/12 真绿；未达标项 = `coverage_gate`（vm 包覆盖率）。**

#### 体量与特性快检

| 项 | 实测 |
|----|------|
| `examples/*.monkey` | 20（均有 `.expected`） |
| `testdata/programs` | 55 |
| `testdata/invalid` | 13（其中 **7/13** 仅 `lint` 仍 exit 0；规格要求 lint/run 须报错 — **invalid 门禁偏弱**） |
| 子命令 | `run` / `fmt` / `lint` / `disasm`（`main.go`） |
| 新内建 | ≥12（含 `map`；无 `range`，符合 map/filter 二选一） |
| class + vm | `examples/class_methods.monkey` tree/vm → `42` |
| `*_test.go` 物理行 | ~5409 |

#### 质量结论（相对 DEMO7 目标）

| 维度 | 判定 |
|------|------|
| DEMO6 基线 + 双后端一致性 | **达标**（parity / examples / testdata 执行面） |
| DEMO7 平台体量 `loc_gate` | **压线达标**（主要靠测试与 `code_test.go` 等撑行数） |
| DEMO7 `coverage_gate` | **未达标**（vm 单元测试过薄；集成路径绿但包覆盖率红） |
| 右栏 100% vs oracle | **可能假绿**：面板 HTTP 轮询与 checklist 可绿，`coverage_gate` 若未 `[verify:]` 不会挡 Completed |
| `run_testdata.sh` | 脚本在 vm 失败时 **SKIP 仍计 PASS**（本次 vm 全过未触发，属潜在假绿） |

**补刀建议（真绿）：** 增厚 `vm/*_test.go`；收紧 `run_testdata.sh`（vm 失败 → FAIL）；修正 invalid 样本；checklist 增加 `[verify: bash scripts/coverage_gate.sh]`。

### 4.5 OpenCode 实证记录（`F:\DEMO6-5`，2026-06-03 人工 oracle）

同一 §1 prompt（IDE 工作流 + 子代理并行铺 examples/testdata/scripts）。**裁判仍为 §4 脚本。**

#### Oracle §4（12 项）

| 命令 | exit | 备注 |
|------|------|------|
| `go build ./...` | 0 | |
| `go vet ./...` | 0 | |
| `gofmt -l .` | 0 | |
| `go test ./...` | 0 | 分包 `*_test.go`（含 `vm` **83.6%** 覆盖） |
| `bash scripts/run_examples.sh` | 0 | **46/46**（23×tree+vm） |
| `bash scripts/conformance.sh` | 0 | |
| `bash scripts/parity.sh` | 0 | **23/23** |
| `bash scripts/coverage_gate.sh` | 0 | evaluator **76.8%**、compiler **75.8%**、vm **83.6%** |
| `bash scripts/run_testdata.sh` | 0 | 55 programs tree+vm |
| `bash scripts/loc_gate.sh` | 0 | **10742** 行（余量 **+742**） |
| `bash scripts/toolchain.sh` | 0 | |

**§4 合计：12/12 真绿（DEMO7 唯一裁判全过）。**

#### 运行与对话一致性

| 项 | 记录 |
|----|------|
| **墙钟** | **64 分钟**（用户计时，turn 结束） |
| 对话收尾表 | 与 §4 oracle **一致**（build/vet/gofmt/test、23 examples、parity、55 testdata、coverage、toolchain、10742 行） |
| 对话宣称 vs oracle | **一致真绿**（非仅 prose；与人工 bash 复验同） |

模型收尾自述要点（存档）：DEMO6 基线保留；DEMO7 增 while/break/continue、class/this/字段、17 内建（含 **range**）、fmt/lint/disasm；**~5900 行** Go 测试 / 10 个 `*_test.go`。  
**注意：** 自述写「VM 对 class **partial support**」，但 `parity.sh` **23/23** 与 `run_examples` **46/46** 全过 — 以脚本为准；若 class 仅在 tree 完整，应补仅-vm 的 class 用例或收紧 parity 集合。

#### 体量与特性快检

| 项 | 实测 |
|----|------|
| `examples/*.monkey` | **23** |
| `testdata/programs` | 55 |
| `testdata/invalid` | 11 |
| `scripts/*.sh` | 7 |
| CLI | `cmd/monkey`（`go build` 后脚本调用） |
| `*_test.go`（自述） | ~5900 行 / 10 文件 |
| 上下文（产品 UI） | 约 **113k Token、~11% 使用率**（同任务 Zagens 约 **20–30%**） |

#### 与 Zagens §4.4 对照（同 prompt、同盘 oracle）

| 维度 | Zagens `DEMO6-3` | OpenCode `DEMO6-5` |
|------|------------------|---------------------|
| 墙钟 | （待补） | **64 min** |
| §4 oracle | 11/12 | **12/12** |
| `loc_gate` | 10014（+14） | **10742**（+742） |
| `coverage_gate` / vm | **7.9% fail** | **83.6% pass** |
| `run_examples` | 40 | **46** |
| `parity` | 20 | **23** |
| 对话 vs oracle | 面板可 100%、oracle 11/12 | **一致 12/12** |
| Harness 可观测 | LHT / verify / sidecar | 清单 14 项 + 子代理铺量 |
| 上下文占用 | 较高 | **较低**（见 §8） |

---

## 5. Zagens vs Cursor / OpenCode 对比（沿用 DEMO6 §4）

### 5.1 实验设置

| 项 | 要求 |
|----|------|
| 工作区 | Zagens `F:\DEMO6-3`、OpenCode `F:\DEMO6-5`、Cursor `DEMO7-cursor`（须在记录表注明） |
| Prompt | 本文 §1 **逐字** |
| Oracle | §4 **全部 12 项**（含 `loc_gate` / `coverage_gate`） |
| 人工裁判 | 跑完后 **同一套 bash 脚本** — 不信 prose / 右栏百分比 alone |

### 5.2 记录表（跑完填）

| 维度 | Zagens（`F:\DEMO6-3`） | OpenCode（`F:\DEMO6-5`） | Cursor |
|------|------------------------|-------------------------|--------|
| 墙钟 | （待补） | **64 min** | |
| §4 oracle（12 项） | **11/12** | **12/12** | |
| 对话收尾 vs oracle | 可能不一致（coverage） | **一致** | |
| `loc_gate` | **10014** | **10742** | |
| `run_examples` | 40/40 | **46/46** | |
| `parity.sh` | 20/20 | **23/23** | |
| `run_testdata.sh` | 55/55 | 55/55 | |
| `coverage_gate` | fail（vm **7.9%**） | **pass**（vm **83.6%**） | |
| 上下文占用（UI） | ~20–30% | ~**11%** / ~113k tok | |
| 右栏/清单 vs oracle | 面板可 100%，oracle 缺 coverage | 清单完成 ≠ 需人工 §4（本次 **一致全绿**） | |
| `step_limit_continue` | （待补） | N/A | N/A |

### 5.3 预期差异（假设）

- **Cursor：** 可能交付 DEMO6 体量 + 部分 DEMO7 特性，`loc_gate` 红而 prose 宣称完成 → **对话假绿**（无人跑 `loc_gate` 时）。
- **Zagens：** 更易出现 `step_limit_continue` 续写；`manifest_gate` 在收尾重跑脚本；若 `loc_gate` / `coverage_gate` 未进 checklist `[verify:]`，可能出现 **面板绿 + oracle 红**（见 §4.4）。

---

## 6. 若 10k 仍不够：备选超大案例（换赛道）

| 案例 | 体量 | 文档 |
|------|------|------|
| **MicroStack** Go 微服务框架 | 1.5万–4万行 | [`microstack-framework.md`](./microstack-framework.md) |
| CodeCrafters Redis | 协议驱动、阶段多 | [`codecrafters-redis.md`](./codecrafters-redis.md) |

DEMO7 与 DEMO6 **同赛道**（Monkey），便于你连续对比；MicroStack 测的是接口冻结/重构，不是解释器。

---

## 7. 空目录 seed（可选）

若希望两边起点一致，可预先只放入：

- `conformance/modulo.monkey`、`conformance/ident_digits.monkey`（同 DEMO3）
- `scripts/loc_gate.sh`、`scripts/coverage_gate.sh`（阈值脚本）
- `README.md` 一行：「实现 DEMO7 spec，勿删 scripts」

**不要**预置 lexer/parser 实现，避免削弱「从零长程」压力。

---

## 8. 后续迭代方向（Zagens harness）

OpenCode 对照（`F:\DEMO6-5`）在 **build 通过后** 用子代理并行铺 `examples` / `testdata` / `scripts`，墙钟上明显优于单线程逐个 `write_file`。Zagens runtime **已具备** `agent_spawn` / `delegate_to_agent` 与 `max_subagents`（系统设置），DEMO7 Zagens 实证（`F:\DEMO6-3`）未强制走该路径，主 turn 直写仍可达相近 oracle。

**建议作为下一版 LHT / prompt 迭代（缩短长程墙钟，不降低 oracle）：**

| 阶段 | 主 agent | 子代理（并行） |
|------|----------|----------------|
| A 内核 | 串行：lexer…vm、双后端打通 | — |
| B 扩容 | 协调、`agent_wait` 收束 | 分片：`examples`+`.expected`、`testdata/programs`、`testdata/invalid` |
| C 测试/脚本 | 跑 `[verify:]`、`manifest_gate` | 分片：`*_test.go` 表驱动、7×`scripts/*.sh` 草稿 |
| 收尾 | **必须**主线程执行 §4 全部 12 项（含 `loc_gate` / `coverage_gate`） | 子代理不得单独宣称「完成」 |

**落地触点（实现时择一或组合）：**

1. **任务 prompt 增补**（§1 可选附录）：铺体量文件时优先 `agent_spawn` 分目录并行，并限制每子代理 scope（≤10–20 文件/批）。
2. **LHT checklist 模板**：阶段 D 拆为可并行子项 + 主线程 `[verify: bash scripts/run_testdata.sh]` 等硬门。
3. **产品默认**：长程 code 任务提高 `max_subagents` 提示或 LHT strict 下默认开启「扩容阶段 spawn 策略」（仍由模型调用工具，非硬编码编排）。
4. **对比指标**：同 prompt 记录墙钟、spawn 次数、§4 pass 数；目标 **墙钟↓、oracle 不缩水**。

**非目标：** 用子代理替代 `verify_gate` / §4 oracle；避免子代理铺盘与主线程 `[verify:]` 脱节导致「Agents 绿、loc/coverage 红」。

---

**修订记录:**

- 2026-06-03 创建：DEMO6 超集、≥10k 行 `loc_gate`、class/while/工具链/testdata 扩容规格。
- 2026-06-03 §4.4：填入 Zagens `F:\DEMO6-3` 人工 oracle（11/12、`loc_gate` 10014、vm coverage 7.9%）；§5 记录表 Zagens 列预填。
- 2026-06-03 §8：子代理并行铺量缩短墙钟 — 后续 harness / prompt 迭代方向（对照 OpenCode `F:\DEMO6-5`）。
- 2026-06-03 §4.5：OpenCode `F:\DEMO6-5` 人工 oracle **12/12**；§5.2 增加 OpenCode 列。
- 2026-06-03 §4.5：OpenCode 墙钟 **64 min**、对话收尾表与 oracle 一致；备注 class/vm 自述 vs parity 全绿。
