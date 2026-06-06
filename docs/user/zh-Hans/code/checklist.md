# 清单侧栏

当模型调用 `checklist_write` / `checklist_update` 时，**Checklist** 侧栏跟踪代码长任务的宏观步骤。

## 出现位置

- 代码模式左侧 **Checklist** 标签
- **审计网格** → 清单象限

条目状态：待办、进行中、完成、阻塞。

## 如何生成

由 Agent 创建清单 — 无需手改 JSON。典型流程：

1. LHT 或 CRAFT 宏观周期开始
2. 模型通过清单工具写入计划项
3. 工具成功后可能勾选完成

## 价值

- 十步以上重构时一眼看进度
- 在问「还剩什么」前发现阻塞项

办公模式不展示工程清单。

## 建议

- 若模型未建清单，可明确要求：「为本重构维护 checklist」。
- 阻塞常因测试失败或审批 — 查 [Diff](/zh-Hans/docs/workspace/diff) 与[审批框](/zh-Hans/docs/desktop/approval-dialog)。

相关：[LHT](/zh-Hans/docs/code/lht) · [子代理](/zh-Hans/docs/code/subagents)
