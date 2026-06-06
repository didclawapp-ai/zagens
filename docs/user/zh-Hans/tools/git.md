# Git 工具

**代码**模式下，Agent 可在对话中查看仓库状态。

## 可用工具

| 工具 | 典型用途 |
|------|----------|
| `git_status` | 暂存/未暂存摘要 |
| `git_diff` | 查看差异（可限定范围） |
| `git_log` | 最近提交 |
| `git_show` | 某次提交或某版本文件 |
| `git_blame` | 行级归属 |

多为**只读**辅助；若你要求提交/推送，仍可能通过 `exec_shell` 执行 `git commit` 等，并受审批约束。

## 适用场景

- 「昨天以来改了什么？」
- 「本分支最近 5 条提交摘要」
- 打补丁前先对齐当前 `git diff`

## 办公模式

办公任务类型**不注册** Git 工具；仓库相关工作请切**代码**模式。

## 建议

- 工作区根目录指向仓库根（或 monorepo 子目录）。
- 大改前配合[快照](/zh-Hans/docs/workspace/snapshots)。

相关：[文件工具](/zh-Hans/docs/tools/files) · [Shell](/zh-Hans/docs/tools/shell)
