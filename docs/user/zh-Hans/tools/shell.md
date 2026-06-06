# Shell 工具

**代码**模式可在工作区内执行命令。**办公**模式**不提供** Shell。

## 主要工具

| 工具 | 行为 |
|------|------|
| `exec_shell` | 执行命令（前台或后台） |
| `exec_shell_wait` / `exec_wait` | 等待后台任务 |
| `exec_shell_interact` / `exec_interact` | 向运行中进程发送 stdin |
| `exec_shell_cancel` | 取消后台 shell |
| `task_shell_start` / `task_shell_wait` | 长任务辅助 |

输出进入工具结果，并常同步到[内嵌终端](/zh-Hans/docs/workspace/terminal)。

## 审批

多数非常规命令会弹出[审批对话框](/zh-Hans/docs/desktop/approval-dialog)。**安全前缀字典**可在策略允许时自动放行常见开发命令（`cargo test`、`npm run` 等）。

在[工具审批](/zh-Hans/docs/settings/approval)与系统执行策略中配置。

## 安全

- 以当前 Windows 用户、工作区 cwd 执行。
- 破坏性命令（删系统目录等）应拒绝。
- 配置启用时可能走外部沙箱。

## 建议

- 优先用 `run_tests`、`cargo check` 等专用工具。
- 起服务（`npm run dev`）用后台 `exec_shell` + `exec_shell_wait`。

相关：[文件工具](/zh-Hans/docs/tools/files) · [CRAFT](/zh-Hans/docs/code/craft)
