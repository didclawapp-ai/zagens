# 执行策略

在 **设置 → 系统 → 安全** 中控制 Agent **能在本机做什么**。

## 沙箱模式

| 模式 | 效果 |
|------|------|
| **Workspace write** | 仅工作区根内读写（默认） — `workspace-write` |
| **Read only** | 禁止写文件 — `read-only` |
| **Full access** | 更广写范围 — **`danger-full-access`**（慎用） |

对应 `config.toml` 的 `sandbox_mode`。桌面 **设置 → 系统** 下拉可能显示为 “Full access”（内部值 `full-access`）；保存后应以 config 中的 **`danger-full-access`** 为准。

## 功能开关

| 设置 | 控制 |
|------|------|
| **Shell tool** | `exec_shell` 家族 |
| **Web search** | `web_search` / `fetch_url` / `web.run` |
| **Exec policy** | 运行时强制执行沙箱与工具策略 |
| **Sub-agents** | `agent_spawn` 等 |

## 审批策略

独立下拉：**on-request**、**untrusted**、**never**、**auto**。对话框行为见[工具审批](/zh-Hans/docs/settings/approval)。

## 外部沙箱

可选 OpenSandbox 经 HTTP 执行 shell — 在 `config.toml` 配置 `sandbox_backend = "opensandbox"`。

## 建议

- 日常开发：`workspace-write` + `on-request` 审批。
- 隔离环境：关闭联网与 shell。
- 办公会话无论开关均不提供 shell。

相关：[网络策略](/zh-Hans/docs/settings/network) · [Shell 工具](/zh-Hans/docs/tools/shell)
