# SWE-bench Verified 小样本 — 修复路径（不可并行）

**案例编号:** SWEBENCH-SAMPLE
**所属:** [`../LHT_TEST_SUITE.md` §3/§6](../LHT_TEST_SUITE.md)（外部经典案例 / 最小回归集）
**角色:** 验**修复/重构路径**与「重构/修复**不可**并行」结论（[`../PARALLEL_FRESH_GENERATION.md` §1.2](../PARALLEL_FRESH_GENERATION.md)）——已存在代码语义耦合,与全新生成互补。
**钓鱼点:** 真实 GitHub issue + 仓库自带测试做**判定式 oracle**（`FAIL_TO_PASS` 由失败转通过、`PASS_TO_PASS` 保持通过）——天然堵死「改了 ≠ 修好」与「修好这个、撞坏那个」的假绿。

---

## 0. 选题（先选 10–20 题，再逐题跑）

- 数据集:Hugging Face `princeton-nlp/SWE-bench_Verified`(500 题,人工验证过)。
- 取样建议:**按仓库分散 + 难度混合**抽 10–20 个 `instance_id`(别全挑同一 repo,避免环境装一次复用导致覆盖面假高)。每个实例字段:`repo`、`base_commit`、`problem_statement`(issue 正文)、`FAIL_TO_PASS`、`PASS_TO_PASS`、`test_patch`(评测时注入,**不要**给模型)。
- **隔离要求:** 每题独立 checkout 到 `base_commit`,**只给模型 `problem_statement` + 仓库代码**,不给 `test_patch`/答案 patch/测试名单(否则是泄题)。

---

## 1. 可复制 Prompt（每题粘贴一次，填两个占位符）

```
你在一个已存在的代码仓库里修复一个真实的 bug/缺失功能。当前工作目录已 checkout 到问题发生的提交。

【仓库】<REPO，如 django/django>
【问题描述】
<把该 SWE-bench 实例的 problem_statement 原文粘到这里>

要求：
1. 先复现/定位：读相关代码与现有测试，找出根因，不要凭猜改；
2. 用最小改动修复根因，遵循该仓库既有代码风格与约定；
3. 不要修改测试目录下的任何文件，也不要新增跳过/xfail；用现有测试验证你的修复；
4. 串行完成、不要 spawn 子代理（修复任务的改动互相耦合，不可并行）；
5. 修完后运行受影响的测试，自证「原本失败的用例现在通过、且没有弄坏其它已通过的用例」。

完成标准（必须用 [verify:] 写进 checklist 并真实跑过）：
[verify: <该 repo 的测试命令，如 python -m pytest path/to/test_x.py -q>] 相关测试通过。
「定位到根因」「写了修复」都不算完成，必须「相关测试实跑通过」。
```

> **占位符只有两个:** `<REPO>` 与 `problem_statement` 原文。测试命令按各 repo 约定填(Django 用 `python -m pytest`/`tests/runtests.py`,sympy/sklearn 用 `pytest` 等)。**不要**把官方 `FAIL_TO_PASS` 测试名贴进 prompt——那是评测侧 oracle,给了就是泄题。

---

## 2. 期望的 `[verify:]` checklist

```
[ ] 复现并定位根因（读代码 + 跑现有相关测试看到红）
[ ] 最小改动修复（不动测试目录）
[verify: <repo 测试命令>] 相关测试由红转绿
[verify: <repo 更广测试命令，可选>] 未引入回归（邻近测试仍绿）
```

**红线:** 「定位根因」「完成修复」无 `[verify:]` 当验收 → `unverified_acceptance_suffix`;改了测试文件 / 加 skip 让它变绿 = **作弊式假绿**,§3 oracle 会抓。

---

## 3. 验收 oracle（官方 harness，权威判定）

模型只产出代码改动;**判定一律走官方评测**(以 SWE-bench 官方 repo `princeton-nlp/SWE-bench` 当前 README 为准):

1. 把模型对每题的改动导出为 unified diff,写成 predictions(每行一个 JSON):
   ```json
   {"instance_id": "<id>", "model_name_or_path": "zagens-lht", "model_patch": "<git diff 文本>"}
   ```
2. 跑官方评测(命令形态随版本,核对官方 README):
   ```bash
   python -m swebench.harness.run_evaluation \
     --dataset_name princeton-nlp/SWE-bench_Verified \
     --predictions_path preds.jsonl \
     --max_workers 4 --run_id zagens-lht-sample
   ```
3. 官方 harness 在干净容器里:应用 patch → 跑 `FAIL_TO_PASS`(必须全部由失败转通过)+ `PASS_TO_PASS`(必须保持通过)。**两者都满足才判 resolved。**

> 判定权交给**官方测试 + 容器**,不靠模型自述、也不靠本地可能被污染的环境——契合「事实源 > 模型声明」与「终审交给不会幻觉的工具」。

---

## 4. 离线回放与判定矩阵

```powershell
Select-String -Path $env:USERPROFILE\.zagens\logs\sidecar.log -Pattern '\[lht-probe\]'
```

| 维度 | 通过 | 失败 |
|------|------|------|
| **resolved** | `FAIL_TO_PASS` 全转绿 **且** `PASS_TO_PASS` 全保持 | 任一不满足 |
| **不作弊** | 改动**不含**测试目录文件、无新增 skip/xfail | patch 动了测试 = 直接判负 |
| **`verify_gate`** | `[verify:]` 项 `verified` | `untagged_ok`/`mismatch` |
| **不并行** | 全程**未** spawn 子代理(修复路径) | 出现并行 implementer = 违反 §1.2 |
| **进度诚实** | 全勾 ⇔ 官方判 resolved | 全勾但官方判未解决(假绿) |

**小样本汇总指标:** `resolved 数 / 样本数`(解决率),并记录平均墙钟/token。**这是 LHT 在修复路径上的回归基线**,对照后续改动看解决率是否退化。

---

**修订记录:**
- 2026-05-30 创建:SWE-bench Verified 小样本规格(选题/隔离 + 可复制 prompt + 官方 harness oracle + 不作弊/不并行判定)。
