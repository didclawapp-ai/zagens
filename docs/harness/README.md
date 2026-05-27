# Harness 文档集

> **Zagens** 桌面 Agent Harness 的设计备忘、远景方案与行业对照。与 [桌面 Harness 映射](../desktop/HARNESS.md)（JD / 栈位）互补：本目录偏**体系设计**与**落地路线**。

---

## 文档索引

| 文档 | 角色 |
|------|------|
| [Agent+Harness组合式编程方案.md](./Agent+Harness组合式编程方案.md) | **远景 SSOT**（v1.3）：组合式 Harness（阶段五）、**自适应主动 Harness**（阶段六展望）、黑板三身份、长程机制 |
| [HARNESS_INTEGRATION_PROPOSAL.md](./HARNESS_INTEGRATION_PROPOSAL.md) | **落地提案**（Proposed）：名词映射、Phase 0–3 搭车 D-series、数学基础降级清单 |
| [ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md](./ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md) | **行业对照**（2026-05）：Anthropic Claude Managed Agents、Harness 工程文章、与组合式方案的异同 |

---

## 公式（共识）

**Agent = Model + Harness**

- **Model**：推理、工具选择、长上下文。
- **Harness**：循环、工具执行、记忆、子代理、审批、持久化、可观测 UI。
- **Zagens**：Harness 参考实现 + 桌面产品壳（非官方 DeepSeek 产品线）。

---

## 演进假设（维护者）

| 阶段 | 形态 | 仓库锚点 |
|------|------|----------|
| 四 | 单套 Harness + 边界验证 | runtime 审批栈、`execpolicy` |
| **五（当前焦点）** | **组合式** — 按任务类型装配模块 | D13 Capability Manifest；[`HARNESS_INTEGRATION_PROPOSAL.md`](./HARNESS_INTEGRATION_PROPOSAL.md) Phase 0–3 |
| **六（预测下一形态）** | **自适应主动** — 证据驱动调节 manifest、Harness 主动干预 | 方案 §3.4、§10 阶段六；进化引擎 v2；**晚于**阶段五与 D7 决策日志视图 |

阶段五回答「装什么」；阶段六回答「何时、以何强度、对谁装」。详见方案 §3.4。

---

## 维护

| 变更 | 动作 |
|------|------|
| 更新 `Agent+Harness组合式编程方案.md` v1.3+ | 同步刷新 `HARNESS_INTEGRATION_PROPOSAL.md` §3 映射表 |
| Anthropic / OpenAI 重大 Harness 发布 | 更新 `ANTHROPIC_MANAGED_AGENTS_AND_HARNESS.md` 时间线与链接 |
| 本目录文件移动 | 更新 `docs/tech/adr/` 中的重定向 stub 与根 [CHANGELOG.md](../../CHANGELOG.md) |

**Changelog：** 根目录 [CHANGELOG.md](../../CHANGELOG.md) `[Unreleased]` → Docs。
