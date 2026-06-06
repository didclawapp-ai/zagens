# 工具审批

高风险工具调用会暂停，等待桌面**批准/拒绝**对话框（默认超时约 120 秒）。

## 何时弹出

取决于**审批策略**，常见包括：

- 超出安全前缀的 shell 命令
- 受保护模式下的文件写入
- 网络域名首次访问（prompt 模式）

## 策略级别（runtime）

| 级别 | 行为 |
|------|------|
| **on-request** | 高风险操作前询问 |
| **untrusted** | 写入前询问 |
| **never** | 全部自动通过（慎用） |
| **auto** | 按 runtime 规则自动决策（与 on-request 组合使用） |

在 **设置 → 系统** 中与**执行策略**（`read-only`、`workspace-write` 等）一并配置。

## 会话记忆

可对会话**记住**本次批准，避免重复打断。

## 建议

- 日常开发用 `workspace-write`；`danger-full-access` 仅用于可信维护。
- 回合卡住时检查主窗口后方是否有审批框。

相关：[流式输出](/zh-Hans/docs/chat/streaming) · [终端](/zh-Hans/docs/workspace/terminal)
