# 网络策略

可限制 **`fetch_url`**、**`web_search`** 与 MCP HTTP 的出站域名。

## 界面开关

**设置 → 系统 → Web search** 全局启用/禁用 Web 工具族。

## 配置文件

细粒度规则在 `~/.zagens/config.toml`（或 `~/.deepseek/config.toml`）：

```toml
[network]
default = "prompt"   # allow | deny | prompt
allow = ["api.deepseek.com", "github.com", ".githubusercontent.com"]
deny = []
audit = true
```

| `default` | 未知主机行为 |
|-----------|-------------|
| **prompt** | 首次访问可能需[审批](/zh-Hans/docs/desktop/approval-dialog) |
| **allow** | 放行 |
| **deny** | 拒绝 |

**deny 优先于 allow**。子域通配：`.example.com` 匹配 `api.example.com`。

## 不受限的流量

- 发往所配置提供商的 LLM API
- Stdio MCP（本地进程）

## 技能安装

`/skill install` 需访问 GitHub — 将 `github.com`、`raw.githubusercontent.com` 加入 `allow` 或接受提示。

相关：[网络工具](/zh-Hans/docs/tools/web) · [MCP](/zh-Hans/docs/settings/mcp)
