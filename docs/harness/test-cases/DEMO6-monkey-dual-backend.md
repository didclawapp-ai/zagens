# DEMO6 — Monkey 双后端解释器（DEMO3 长程超集 · Zagens vs Cursor 对比）

**案例编号:** DEMO6
**所属:** [`../LHT_TEST_SUITE.md`](../LHT_TEST_SUITE.md)（长程加压 + 双后端一致性 + 多闸门验收）
**前置:** [`DEMO3-monkey-interpreter.md`](./DEMO3-monkey-interpreter.md)（单后端 tree-walking + `%` / `counter1` 钓鱼点）
**实证:** [`../LONG_HORIZON_CODE_TASKS.md`](../LONG_HORIZON_CODE_TASKS.md) DEMO6 段（`F:\DEMO6`，~45 分钟、40/40 双引擎示例、`step_limit_continue`）
**用途:** **同一 prompt、同一 oracle**，在 **Zagens（LHT on）** 与 **Cursor Agent** 各跑一轮，对比产物真绿率、墙钟、以及 harness 独有信号。

---

## 0. 一句话

在 DEMO3 全部要求之上，再实现 **字节码编译器 + 栈式 VM** 第二后端；CLI 支持 `--engine=tree|vm`；**两套引擎对同一批示例输出必须一致**。任务更长（目标 **~3k–6k 行 Go**、**≥12 个示例**、**≥8 条 `[verify:]`**），用来压 **step 续写、验收塌缩、双后端只实现一半、parity 未跑** 等 DEMO3 测不到的失败模式。

> **与 DEMO3 的分工：** DEMO3 = 「单后端 + 验收塌缩」最小钓鱼；DEMO6 = 「双后端 + 一致性 + 更多闸门」长程对比实验。

---

## 1. 喂给两边的 prompt（逐字，禁止改验收语义）

```
用 Go 从零实现 Monkey 语言解释器（参考《Writing an Interpreter in Go》），要求两个执行后端，并在空目录下交付可运行仓库：

【后端 A — tree-walking】
词法器、Pratt 解析器、tree-walking 求值器、内建函数（len/puts/first/last/rest/push）、REPL。

【后端 B — 字节码】
将同一 AST 编译为字节码（compiler/ 包），由栈式虚拟机执行（vm/ 包）；CLI 必须支持：
  ./monkey run --engine=tree <file.monkey>
  ./monkey run --engine=vm <file.monkey>
默认引擎 tree；无参数启动仍为 REPL（tree）。

【在标准 Monkey 之上必须实现的扩展（两个后端行为一致）】
1. 取模运算符 %（整数取模，10 % 3 == 1；除零须报错而非崩溃）；
2. 标识符可含数字（首字符为字母或 _，其后可含字母/数字/_，如 counter1、x2、_tmp3）；
3. 字符串、数组、哈希字面量与索引；
4. 闭包（返回函数、捕获自由变量）。

【示例与脚本（验收锚点，禁止塌缩为「只创建文件」）】
- examples/ 下至少 12 个 .monkey，每个有同名 .expected；须覆盖：取模、数字标识符、字符串、数组、哈希、闭包、递归、内建、双后端都会踩的边界（如 10 % 0 报错）。
- scripts/run_examples.sh：对每个 .monkey 分别用 --engine=tree 与 --engine=vm 执行，输出与 .expected 比对，任一失败则整体 exit 1。
- scripts/conformance.sh：至少跑 conformance/modulo.monkey 与 conformance/ident_digits.monkey（内容见本案例 §3.2），tree 与 vm 各跑一遍，输出须含 ok modulo / ok ident-digits。
- scripts/parity.sh：对 examples/*.monkey（或 conformance 全集），比较 tree 与 vm 的 stdout（忽略尾部空行），不一致则 exit 1。

【测试】
- go test ./... 全绿；测试里必须有 % 与 counter1 的用例；compiler/ 与 vm/ 包须有非空 _test.go。
- scripts/coverage_gate.sh：对 evaluator、compiler、vm 三包执行覆盖率检查，任一包语句覆盖率 < 60% 则 exit 1（用 go test -cover 解析，勿只写「覆盖率达标」 prose）。

【完成标准（全部须真实执行并通过）】
go build ./...、go vet ./...、gofmt 干净、go test ./...、bash scripts/run_examples.sh、bash scripts/conformance.sh、bash scripts/parity.sh、bash scripts/coverage_gate.sh 全部 exit 0。

不要省略字节码后端；不要只在 tree 后端实现 % 或 counter1。
```

> **钓鱼设计（相对 DEMO3 新增）：**
> - 「实现 compiler/vm」≠「tree/vm 对同一脚本输出一致」——易只做 A 后端或 VM 抄书漏 `%`。
> - `run_examples.sh` 若只跑 `--engine=tree`，VM 假绿。
> - `go test` 全绿但 `parity.sh` 未跑 → 双后端语义漂移。
> - 「创建 12 个示例」无 `[verify: bash scripts/run_examples.sh]` → DEMO3 式验收塌缩。

---

## 2. 建议 checklist 分解（模型自写，人/回归对照）

实现项（可无 `[verify:]`）：

```
[ ] go.mod + 目录骨架（cmd、token、lexer、ast、parser、object、evaluator、compiler、vm、repl、examples、conformance、scripts）
[ ] 词法器：% token、数字标识符 counter1 规则
[ ] AST + Pratt 解析器（% 优先级与 * / 同级乘除块）
[ ] tree 求值器 + 内建 + 闭包环境
[ ] compiler：AST → 字节码（含 %、索引、闭包 upvalue）
[ ] vm：执行字节码，语义与 tree 对齐
[ ] CLI：run --engine=、REPL、文件批跑
[ ] examples/*.monkey + .expected（≥12）
[ ] conformance/*.monkey
```

验收项（**必须** `[verify: <命令>]`，且命令与 §3 一致）：

```
[verify: go build ./...]
[verify: go vet ./...]
[verify: test -z "$(gofmt -l .)"]
[verify: go test ./...]
[verify: bash scripts/run_examples.sh]      ← 双引擎 × 全示例，禁止写成「创建 run_examples.sh」
[verify: bash scripts/conformance.sh]       ← 含 % 与 counter1 最小断言
[verify: bash scripts/parity.sh]            ← tree vs vm 输出一致
[verify: bash scripts/coverage_gate.sh]     ← 非零退出可拦住的覆盖率（见 §3.4）
```

**红线：** 与 [DEMO3 §2](./DEMO3-monkey-interpreter.md) 相同，另加：

- ❌ 仅 `go test` 通过、未跑 `parity.sh` / `run_examples.sh --engine=vm`
- ❌ checklist 写「字节码后端完成」但无 `[verify: bash scripts/parity.sh]`

---

## 3. 验收 oracle（人/CI — 两边对比的**唯一裁判**）

在产物根目录（如 `F:\DEMO6-zagens` / `F:\DEMO6-cursor`）执行：

```bash
set -euo pipefail
cd <产物根>

go build ./...
go vet ./...
test -z "$(gofmt -l .)"
go test ./...
bash scripts/run_examples.sh
bash scripts/conformance.sh
bash scripts/parity.sh
bash scripts/coverage_gate.sh
```

全部 exit 0 ⇒ **产物真绿**。任一失败 ⇒ **假绿或未完成**（与模型 prose 无关）。

### 3.1 双引擎快检（单命令）

```bash
go build -o monkey .
./monkey run --engine=tree examples/08_modulo.monkey    # 期望含 1 等
./monkey run --engine=vm examples/08_modulo.monkey      # 与 tree 同序输出
./monkey run --engine=tree examples/09_digit_identifiers.monkey
./monkey run --engine=vm examples/09_digit_identifiers.monkey
```

（示例编号可与仓库一致，只要覆盖取模与数字标识符即可。）

### 3.2 conformance 最小脚本（可预制或要求模型创建）

`conformance/modulo.monkey`：

```monkey
let r = 10 % 3;
if (r != 1) { puts("FAIL modulo"); } else { puts("ok modulo"); }
let err = 10 % 0;
puts(err);
```

`conformance/ident_digits.monkey`：

```monkey
let counter1 = 0;
let x2 = counter1 + 41;
if (x2 != 41) { puts("FAIL ident-digits"); } else { puts("ok ident-digits"); }
```

`scripts/conformance.sh` 须对 **tree 与 vm** 各执行上述脚本，且 grep 到 `ok modulo` / `ok ident-digits`；modulo 脚本对 `% 0` 须得到**错误对象**而非进程崩溃。

### 3.3 `scripts/parity.sh` 参考行为

对每个 `examples/*.monkey`（及建议包含 `conformance/*.monkey`）：

```bash
tree_out=$(./monkey run --engine=tree "$f" 2>&1) || exit 1
vm_out=$(./monkey run --engine=vm "$f" 2>&1) || exit 1
# 规范化后比较；若预期 stderr 含错误，两边须同为错误语义
```

### 3.4 `scripts/coverage_gate.sh`（可预制）

目的：避免 DEMO6 实证里「`go test -cover` exit 0 但 <80%」的**语义假绿**；对比实验用 **60%** 三包门槛（可调），**必须非零退出**：

```bash
#!/usr/bin/env bash
set -euo pipefail
MIN=60
for pkg in monkey/evaluator monkey/compiler monkey/vm; do
  pct=$(go test -cover "$pkg" 2>&1 | sed -n 's/.*coverage: \([0-9.]*\)% of statements/\1/p' | head -1)
  # 若 pct < MIN 则 exit 1
done
```

（实现细节可由模型写，但 **exit code 须反映阈值**。）

---

## 4. Zagens vs Cursor 对比协议（建议固定记录）

### 4.1 实验设置

| 项 | 要求 |
|----|------|
| 工作区 | **两个空目录**，同盘同机：`DEMO6-zagens`、`DEMO6-cursor`（名称自定） |
| Prompt | §1 **逐字**粘贴，不附加「在 Cursor 里不用 verify」等提示 |
| 模型 | 记录名称与日期（Zagens 线程导出 + Cursor 模型 ID） |
| 人工 oracle | **两边跑完后都由人（或同一脚本）跑 §3** — 避免只信一方 prose |

### 4.2 记录表（跑完填）

| 维度 | Zagens | Cursor |
|------|--------|--------|
| 墙钟（turn 结束 − 开始） | | |
| 产物 oracle §3（8 项） | pass / fail | pass / fail |
| `run_examples` 通过数（应为 2×N） | / | / |
| `parity.sh` | | |
| 钓鱼：`%` 在 vm 上 | | |
| 钓鱼：`counter1` 在 vm 上 | | |
| 对话宣称「全绿」与 oracle 一致？ | | |
| 线程导出路径 | | |
| `unverified_acceptance_nudge` 次数 | | |
| `verify_mismatch_nudge` 次数 | | |
| `step_limit_continue` 次数 | | |
| `incomplete_stop` | 0 期望 | N/A |
| `graph_complete` 假绿出口 | 0 期望 | N/A |

### 4.3 如何解读「LHT 有没有优势」

| 若观察到… | 解读 |
|-----------|------|
| 两边 oracle **都绿** | 产物等价；比墙钟、步数、你是否少跑脚本 |
| Zagens **红**、Cursor **绿** | 罕见；查 Zagens 沙箱是否拦 bash / Go |
| Zagens **绿**、Cursor **红** | LHT 续跑/B 可能逼出 parity/vm；或 Cursor 早停/只做了 tree |
| 两边 prose 全绿、oracle **红** | **假绿**；Zagens 应出现 nudge 或未 `graph_complete`（若仍 Completed 则 harness 回归） |
| Zagens 有 `step_limit_continue`、Cursor 无 | 长程步数压力只在 Zagens 显式续写（Cursor 无步数闸门标签） |

**本实验不能证明的：** LHT 让「更强模型写得更优雅」——只证明 **在更长任务、更多 `[verify:]` 下，系统是否减少未验就收尾**。

---

## 5. 期望压到的 LHT 信号（仅 Zagens）

跑完后 grep `sidecar.log` 或看 **Nodes Tab**：

```powershell
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\] verify_gate'
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern 'unverified_acceptance_nudge|verify_mismatch_nudge|step_limit_continue|incomplete_stop|graph_complete'
```

**健康态参考（来自 DEMO6 实证）：**

- 可能出现 `step_limit_continue`（撞满 step 预算后续写）
- 收尾 `gate_skip reason=graph_complete open_items=0` 且 oracle 全绿
- **不应**在 oracle 红时出现「无 `incomplete_stop` 的 Completed」
- `unverified_acceptance_nudge` ≥0 次可接受（逼补 `[verify:]`）

---

## 6. 规模与时长预期

| 指标 | DEMO3 | DEMO6（本案例） |
|------|-------|-----------------|
| 目标行数 | ~1.5k–2.5k | ~3k–6k |
| 示例数 | 5–10 | ≥12 × 2 引擎 |
| `[verify:]` 条数 | 4–6 | 8 |
| 典型墙钟 | ~15–20 min | ~35–50 min（视模型） |
| 主要新增风险 | 验收塌缩 | 单后端假绿、parity 未跑、step 耗尽 |

---

## 7. 可选：预制 `conformance/` 与 `scripts/coverage_gate.sh`

若希望**两边起点完全一致**，可在空目录预先放入仅含 `conformance/*.monkey` + `scripts/coverage_gate.sh` 的 seed（不含实现代码）。否则由模型创建，对比时记录是否漏文件。

---

**修订记录:**

- 2026-06-03 创建：DEMO3 超集双后端对比规格（prompt + oracle + Zagens/Cursor 记录表 + LHT 信号）。
