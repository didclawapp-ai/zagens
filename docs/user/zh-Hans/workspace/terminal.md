# 嵌入式终端

在**代码**任务类型下，Zagens 提供绑定工作区目录的 **xterm.js** 终端。

## 适用场景

- 查看 Agent 启动的长时命令输出
- 与 Agent 并行手动执行命令
- 实时查看构建/测试日志

**办公**模式不提供终端 — 文档流程使用 `read_office` / `write_office`。

## 安全

Shell 执行受**执行策略**约束（如 workspace-write）。高风险命令可能弹出**审批对话框**。

可在 **设置 → 系统** 中配置策略与审批。

## 建议

- 每个仓库使用独立工作区，保证终端 cwd 正确。
- 若输出停滞，检查侧栏 runtime 连接状态。
- 可重复的操作尽量交给 Agent，便于会话回放。

相关：[代码模式](/zh-Hans/docs/code-mode) · [工作区概览](/zh-Hans/docs/workspace/overview)
