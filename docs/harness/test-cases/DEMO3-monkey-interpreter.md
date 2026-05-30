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

**修订记录:**
- 2026-05-30 创建：DEMO3 复现规格（prompt + `[verify:]` checklist + oracle/conformance 脚本 + 离线回放 + 判定矩阵）。
