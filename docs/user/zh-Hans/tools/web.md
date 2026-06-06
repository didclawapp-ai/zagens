# 网络工具

启用**联网**后，Agent 可搜索公网并抓取页面。

## 工具

| 工具 | 用途 |
|------|------|
| `web_search` | 搜索 API，返回摘要与链接 |
| `fetch_url` | 下载 URL 并提取可读正文 |
| `web.run` | 类浏览器抓取，提取更丰富 |
| `finance` | 行情/金融辅助（视配置） |

办公与代码模式在开启联网时注册相同 Web 工具族。

## 网络策略

新域名首次访问在 **prompt** 模式下可能需审批。允许/拒绝列表见 [网络策略](/zh-Hans/docs/settings/network)（亦可直接改 `config.toml`）。

## 办公场景

常见：`web_search` 搜行业动态 → `fetch_url` 读文章 → `write_office` 写报告。见 [P0 竞品](/zh-Hans/docs/office/p0-competitive)。

## 建议

- 已知来源可直接在对话里贴 URL。
- 抓取内容计入上下文 — 长页可要求先摘要。
- 隔离环境可关闭联网。

相关：[办公 I/O](/zh-Hans/docs/tools/office-io) · [MCP](/zh-Hans/docs/settings/mcp)
