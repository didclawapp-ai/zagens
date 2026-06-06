# 代码模式

**代码**任务类型面向软件工程：阅读仓库、执行命令、查看 diff，并通过 LHT / CRAFT 类 harness 工具迭代。

## 工作区

将 Zagens 指向 git 仓库或项目目录。Agent 可以：

- 搜索并编辑源码（`grep_files`、符号索引）
- 在[沙箱化工作区终端](/zh-Hans/docs/workspace/terminal)中运行命令
- 生成补丁，并在 [diff 预览](/zh-Hans/docs/workspace/preview) 中审阅

文件树与快照见[工作区概览](/zh-Hans/docs/workspace/overview)。

## 界面区域（代码）

| 区域 | 用途 |
|------|------|
| **工作区面板** | 文件树、预览、diff、终端 |
| **清单侧栏** | 长程任务步骤 |
| **审计网格** | 清单、Scratchpad、LHT 图、子代理 |
| **回放** | 查看历史工具调用 |

整体布局见[界面导览](/zh-Hans/docs/ui-tour)。

## 何时用代码 vs 办公

| 任务 | 模式 |
|------|------|
| 重构、测试、CI 修复 | **代码** |
| 竞品简报、报价表、经营日报 | **办公** |

## 使用建议

- 每个仓库使用独立工作区，上下文更清晰。
- 首条消息写明具体目标（如「为 `foo.rs` 补充单元测试」）。
- 大型 monorepo：说明关注的 crate 或子目录。
- 高风险 shell 命令可能弹出**审批对话框** — 可在设置中配置。

深入阅读：[LHT](/zh-Hans/docs/code/lht) · [CRAFT](/zh-Hans/docs/code/craft) · [审计 Scratchpad](/zh-Hans/docs/code/audit-scratchpad)
