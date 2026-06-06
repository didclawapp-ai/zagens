# 工作区概览

**工作区**是 Zagens Agent 读取、编辑和执行命令的文件夹。建议每个项目或办公 inbox 使用一个目录。

## 如何选择工作区

1. 首次启动或从侧栏进入 **工作台**。
2. 指向 git 仓库（代码模式）或含 `inbox/`、`data/` 的办公目录（办公模式）。
3. 会话与工作区绑定 — 切换目录会显示该路径下的历史对话。

## 代码 vs 办公布局

| 模式 | 典型结构 |
|------|----------|
| **代码** | 仓库根目录；使用终端、diff、符号索引 |
| **办公** | `inbox/` 放简报，`data/` 放表格；输出在 `deliverables/` |

办公目录说明见[办公工作区](/zh-Hans/docs/office/workspace)。

## 能做什么

- 在**文件树**中浏览 — 见[文件树](/zh-Hans/docs/workspace/file-tree)
- 在右栏**预览**支持的格式
- 代码模式下：使用**嵌入式终端**、查看 **[Diff](/zh-Hans/docs/workspace/diff)**、**[恢复快照](/zh-Hans/docs/workspace/snapshots)**

## 建议

- 使用专用文件夹，不要指向磁盘根目录或用户主目录。
- 办公演示可将 `docs/harness/fixtures/office-demo/` 复制到工作区。
- 大型 monorepo：首条消息说明关注的子目录或 crate。

相关：[文件预览](/zh-Hans/docs/workspace/preview) · [终端](/zh-Hans/docs/workspace/terminal)
