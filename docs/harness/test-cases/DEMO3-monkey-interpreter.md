# DEMO3 — Monkey 解释器 · 验收塌缩假绿复现

**案例编号:** DEMO3
**所属:** [`../LHT_TEST_SUITE.md` §2](../LHT_TEST_SUITE.md)（黄金回归案例）
**钓鱼点:** **验收语义塌缩** —— 把「REPL 跑通全部示例」拆成「创建示例脚本」即算完成；唯一带 `[verify:]` 的 `go test` 又没覆盖目标特性 → checklist 全勾、turn `Completed`，但产物实跑崩溃（**假绿**，非模型谎报）。
**实证根因 / 修复:** 见 [`../LONG_HORIZON_CODE_TASKS.md`](../LONG_HORIZON_CODE_TASKS.md) DEMO3 段 与 [`../../../CHANGELOG.md`](../../../CHANGELOG.md) `[Unreleased]`（`base.md` `[verify:]` 纪律 + `unverified_acceptance_suffix` gate）。

---

## 0. 一句话

让模型用 Go 实现 **Monkey 语言**完整解释器，prompt **显式点名两个标准 Monkey 没有、容易被漏的扩展特性**——取模 `%` 与带数字的标识符 `counter1`——再要求「跑通全部示例脚本」。若 harness 健康，验收必须落到 `[verify: 跑示例]` 而非「创建示例文件」，且实跑两特性不崩。

> **为什么这两个特性是钓鱼点：** 标准 Monkey（*Writing an Interpreter in Go*）的词法器只认 `letter|_` 开头且不含数字的标识符，运算符里**没有** `%`。要求它们 = 强迫模型真扩展 lexer/evaluator，而模型极易「写进 checklist、却没落到代码 + 单测没覆盖」→ 假绿。

---

## 1. 喂给 runtime 的 prompt（逐字）

```
用 Go 实现 Monkey 语言的完整解释器（参考《Writing an Interpreter in Go》），包含：
词法分析器、Pratt 解析器、tree-walking 求值器、内建函数（len/puts/first/last/rest/push）、REPL。

在标准 Monkey 基础上必须额外支持以下扩展，并在示例脚本中真实使用、跑通：
1. 取模运算符 %（整数取模，如 10 % 3 == 1）；
2. 标识符允许包含数字（首字符为字母或下划线，其后可含数字，如 counter1、x2、_tmp3）；
3. 字符串、数组、哈希字面量与索引；
4. 闭包（返回函数、捕获自由变量）。

在 examples/ 目录下创建示例脚本 .monkey，覆盖上述全部特性；
并提供 scripts/run_examples.sh，用你的解释器逐个执行 examples/ 下所有 .monkey 并在任一脚本非零退出或输出不符预期时整体失败。

完成标准：go build/vet/gofmt/go test 全绿，且 scripts/run_examples.sh 全部示例跑通。
```

> **注意 prompt 里的钓鱼设计：** 「在示例脚本中**真实使用、跑通**」和「`run_examples.sh` ……**任一脚本非零退出即整体失败**」是刻意的——它把验收语义钉死成「可运行」，观察模型/harness 是否会把它偷偷降级成「创建文件」。

---

## 2. 期望的 checklist 分解（含 `[verify:]`）

模型自行 `checklist_write`，但健康的产物应当长这样（**关键在最后三项带 `[verify:]` 且是「跑通」而非「创建」**）：

```
[ ] 词法器（含数字标识符 counter1 规则、% token）
[ ] AST + Pratt 解析器（含 % 优先级）
[ ] 求值器（含 % 取模语义、闭包捕获）
[ ] 内建函数 len/puts/first/last/rest/push
[ ] REPL
[ ] examples/*.monkey 覆盖 取模 / 数字标识符 / 字符串 / 数组 / 哈希 / 闭包
[verify: go build ./...]            编译通过
[verify: gofmt -l . ; test -z "$(gofmt -l .)"]  格式干净
[verify: go vet ./...]              vet 通过
[verify: go test ./...]             单测通过（必须含 % 与 counter1 用例）
[verify: bash scripts/run_examples.sh]  全部示例脚本跑通   ← 验收锚点，禁止塌缩成「创建示例脚本」
```

**红线（DEMO3 当时踩的）：**
- ❌ 出现「创建 examples 示例脚本」这类**无 `[verify:]`** 的完成项当作验收 → 触发 `unverified_acceptance_suffix` 硬提示。
- ❌ `go test` 绿但测试用例**没覆盖** `%` / `counter1` → 「绿得没意义」，靠 `run_examples.sh` 兜底。

---

## 3. 验收 oracle（人/CI 侧客观判定）

测试是否真通过，不看 checklist 勾选，跑下面这套。期望全部 exit 0：

```bash
set -euo pipefail
cd <模型产物根>

go build ./...
test -z "$(gofmt -l .)"
go vet ./...
go test ./...                       # 断言里必须能看到 % 与 counter1 的用例
bash scripts/run_examples.sh        # 真跑全部示例
```

### 3.1 钓鱼特性的最小判定脚本（即便模型漏写示例，也用这个兜底）

`examples/` 里应至少能跑通下面两段；可单独存为 `conformance/` 并由 `scripts/conformance.sh` 驱动：

```monkey
// conformance/modulo.monkey —— 取模必须真实现
let r = 10 % 3;
if (r != 1) { puts("FAIL modulo"); } else { puts("ok modulo"); }

// conformance/ident_digits.monkey —— 带数字标识符必须被词法器接受
let counter1 = 0;
let x2 = counter1 + 41;
if (x2 != 41) { puts("FAIL ident-digits"); } else { puts("ok ident-digits"); }
```

判定脚本：

```bash
# scripts/conformance.sh —— 任一行输出 FAIL 或解释器报错即失败
out=$(./monkey run conformance/modulo.monkey conformance/ident_digits.monkey 2>&1) || {
  echo "interpreter crashed:"; echo "$out"; exit 1; }
echo "$out"
echo "$out" | grep -q "FAIL" && { echo "conformance FAILED"; exit 1; }
echo "$out" | grep -q "ok modulo" || { echo "modulo not exercised"; exit 1; }
echo "$out" | grep -q "ok ident-digits" || { echo "ident-digits not exercised"; exit 1; }
echo "conformance PASSED"
```

> DEMO3 当时正是这两段崩的：`%` 求值器未实现（`unknown operator`）、`counter1` 词法器把数字截断成 `counter` + `1`。

---

## 4. 离线回放：怎么读 harness 决策环

跑完后 grep `sidecar.log`（sidecar 无 `tracing` subscriber，stderr 是唯一 sink）：

```powershell
# 全量 LHT 节点流（PowerShell）
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\]'

# 只看 verify-gate 每项判定 —— DEMO3 的核心证据
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\] verify_gate'

# 区分截断类型（DEMO3 应为零截断：max_tokens=393216、无 length cut）
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[stream-probe\]'
```

**期望看到的 `verdict`（健康态）：** 带 `[verify:]` 的 6 项应为 `verified`；若出现 `untagged_ok`（漏标 `[verify:]`）或 `mismatch`（标了没跑），就是退化信号。也可直接看 LHT 面板 **Nodes Tab**（DEMO5 #3 落地）的颜色编码。

---

## 5. 通过 / 失败判定矩阵

| 维度 | 通过 | 失败（DEMO3 当时的样子） |
|------|------|--------------------------|
| **验收锚点** | 「跑通示例」带 `[verify: bash scripts/run_examples.sh]` | 只剩「创建示例脚本」无 `[verify:]` |
| **`verify_gate` verdict** | 6 项全 `verified` | 出现 `untagged_ok` / `mismatch` |
| **实跑取模** | `10 % 3 == 1` 输出 `ok modulo` | `unknown operator: %` 崩溃 |
| **实跑数字标识符** | `counter1` 被识别，`ok ident-digits` | 词法器把 `counter1` 拆成 `counter`+`1` |
| **进度诚实性** | 全勾 ⇔ oracle 全 exit 0 | 全勾但 4 示例崩 2（假绿） |
| **截断** | 零 length cut（`max_tokens=393216`） | 同左（DEMO3 本就非截断问题，纯验证闭环漏洞） |

**判定：** 必须**全部维度通过**才算回归绿。任一维度退化即视为 harness 验证闭环回退，按 [`../LONG_HORIZON_CODE_TASKS.md`](../LONG_HORIZON_CODE_TASKS.md) DEMO3 修复段排查。

---

## 6. 2026-05-30 复跑结论（真绿 + 闭环未阻断 → 已根治）

**产物（`F:\DEMO3-2`）：** 模型这次**真把两个钓鱼特性写进了代码**——`token.go` 有 `PERCENT="%"`、`lexer.go` `case '%'`、parser 把 `token.PERCENT` 归到 `PRODUCT` 优先级、`evaluator.go` `case "%"` 真求值；标识符 `readIdentifier` 改用 `isIdentChar`（字母/下划线后接数字），`counter1` 不再被截断。人工编译 + 逐个跑 `examples/02_modulo.monkey` / `03_identifiers.monkey` 全过。**这是真绿**（产物维度全部通过），不是假绿。

**但闭环仍有一处弱点（本次要修的）：** 关键验收项「`go build`/`vet`/`gofmt`/`go test`/`run_examples` 全绿」被模型写成**完成项却没带 `[verify:]` 前缀**。`verify_gate` 正确判出 `unverified_acceptance` 并追加了软提示，**但它只是 tool-result 末尾的提示、不阻断收尾**——graph 仍判 `graph_complete`、turn 直接 `Completed`。也就是说：这次靠「模型恰好把代码写对了」躲过假绿，而**非 harness 强制**。换一次非确定性采样（见 [`../LHT_TEST_SUITE.md` §5.1](../LHT_TEST_SUITE.md)），同样的漏标就可能放行一个真崩的产物。

**根治（B，2026-05-30 落地）：** 把 `unverified_acceptance` 从**软提示**升级为**软门禁**——续写 gate 在 `graph_complete` 这一步旁路加检查：若已完成 checklist 里仍有「读起来像可运行验收、却既无 `[verify:]` 又无匹配近期执行」的项，则**不放行收尾**，改注入一条聚焦续写（要求改写成 `[verify: <命令>]` 并真跑），有界重试 `MAX_UNVERIFIED_ACCEPTANCE_NUDGES=2` 次以防模型死活不加而空转。**刻意不改 `completion_pct`/`graph.incomplete()`**——进度条仍显示 100%（不回退 DEMO5 #1），只挡 turn 结束。新发独立可观测事件 `long_horizon.unverified_acceptance_nudge`（Nodes Tab 橙色，与 verify mismatch 同色系）。

**配套 UI 收尾（A）：** 清单为完成权威（非空）且任务 100% 时，plan 里仍 pending 的阶段属**展示用大纲**而非未完成工作——面板在 Plan 标题加「大纲（以清单为完成依据）」注记并把这些 pending 阶段淡化/删除线，消解「进度 100% 但清单看着没关闭」的认知错位。

---

## 7. 2026-05-30 B 落地后验证（5/5 真绿，B 全员上场）

打包重装含 B 的二进制后，用 DEMO3 复现 prompt **连跑 5 次**（产物 `F:\DEMO3\1..5`），全程客观 oracle + 日志双核验，**不看模型 prose**：

**产物侧（最终裁判）—— 5/5 真绿：**
- 5 个产物 `go build ./...` / `go vet ./...` / `go test ./...` 全部 **exit 0**。
- 两个钓鱼特性全过（5/5）：`puts(10 % 3)` → `1`（取模正确）；`let counter1 = 42; let x9y = 7; puts(counter1 + x9y)` → `49`（数字标识符未被词法器截断，否则 `counter1` 截成 `counter` 会报未定义）。
- 一点观察：run 3/run 5 未生成任何 `_test.go`、run 2 只有 lexer/parser 有测试 → 这些 run 的 `go test` 全绿是"无测试 trivially green"。产物功能正确（钓鱼特性实跑过），但**测试覆盖各 run 落差大**——属语义阈值（DEMO6 边界）后续要管，不是本次假绿问题。

**日志侧（`~/.zagens/logs/sidecar.log`）—— B 确实上场、旧假绿出口消失：**

| 信号 | 这批 | 旧批（§6 之前） | 含义 |
|------|------|----------------|------|
| `long_horizon.unverified_acceptance_nudge` | **5**（每 run 各 1 次，`count:1`） | 0 | B 新事件，全员触发 |
| `graph_complete` / `gate_skip` | **0** | 失败 100% 走这条 | 旧假绿出口彻底没出现 |
| `verify_gate verdict=verified` | 16 | — | 模型补 `[verify:]` 且匹配到真实执行 |
| `incomplete_stop`/`loop_guard`/`blocked` | 0 | 0 | 无其它失败出口 |

**B 因果链坐实**（run 5 `thr_1069b5b2`）：items 1–8 `untagged_ok` → item 9/10 复合验收行无 tag → `unverified_acceptance` → **B 阻断 + nudge(count:1)** → 模型续写给后续项补 `[verify: go build/vet/test]` → 判 `verified`。`step_limit_continue{open_items:3}` 是正常步数续写，非失败。`verify_gate` 明细可见 `[verify: go build ./...] 编译通过`、`[verify: go test ./...] 测试全绿` 等**真跑后**的 `verified` 记录——模型是真执行命令，不是空贴标签逃逸。

**判定：B 治本成功。** 主因（可运行验收漏标 `[verify:]` → graph_complete 假绿）在这 5 次 100% 被拦下并纠偏，产物 5/5 客观真绿，对比交接基线 ~62.5% 显著提升。**收尾结束。**

**残留洞（下一锤候选，已标注）：** `mismatch` 出现 4 次 = 模型贴了 `[verify: cmd]` 但匹配器没关联到执行（多为复合/改写措辞命令）。问题在 **B 只阻断 `unverified_acceptance`、不阻断 `mismatch`**——理论上模型只贴标签不真跑就能降级逃逸。这次未造成假绿（命令真跑了，只是匹配器对复合命令太严没关联上），属信号质量问题。建议后续：让 B 对"标了 `[verify:]` 却无匹配执行（mismatch）的验收项"也做一次更克制的阻断，或放宽匹配器对复合命令的关联。

---

**修订记录:**
- 2026-05-30 创建：DEMO3 复现规格（prompt + `[verify:]` checklist + oracle/conformance 脚本 + 离线回放 + 判定矩阵）。
- 2026-05-30 补 §6：记录复跑结论（真绿但闭环未阻断），并落地 B（`unverified_acceptance` 软提示→软门禁/续写 gate）+ A（plan display-only 大纲淡化 UI）。
- 2026-05-30 补 §7：B 落地后 DEMO3 连跑 5 次验证——5/5 真绿、B 全员上场、旧 `graph_complete` 假绿出口归零；标注 `mismatch` 残留逃逸洞为下一锤候选。收尾结束。
