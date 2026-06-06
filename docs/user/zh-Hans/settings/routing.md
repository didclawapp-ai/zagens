# 模型路由

**Routing** 将**意图（intent）**映射到**模型**，让不同提示自动选用不同模型。

## 入口

**设置 → Routing**（需 runtime 已连接）。

## 规则

每条规则为 `intent → model`，例如：

| 意图 | 模型（示例） |
|------|-------------|
| `code` | `deepseek-v4-pro` |
| `chat` | `deepseek-v4-flash` |
| `research` | `deepseek-v4-pro` |

在面板增删改；重复 intent 会被拒绝。

## 在对话中使用

在编写器选择 **route intent**（若界面暴露），或通过 API 传入。回合开始时运行时匹配规则并选定模型。

## 建议

- 意图名短且稳定（`code`、`office`、`fast`）。
- 摘要用 Flash，多文件改动用 Pro。
- 路由不替代 [API Key](/zh-Hans/docs/settings/api-key) — 模型须在所选提供商下可用。

相关：[API Key](/zh-Hans/docs/settings/api-key) · [用量](/zh-Hans/docs/settings/usage)
