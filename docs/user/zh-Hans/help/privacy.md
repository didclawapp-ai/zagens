# 隐私摘要

Zagens 在**本机**运行。本文为产品行为摘要；法律全文见[隐私政策](/zh-Hans/privacy)。

## 留在本机的数据

- API Key 与用户目录下的 `config.toml`
- 聊天会话、工作区文件、符号索引缓存
- [用量面板](/zh-Hans/docs/settings/usage)中的统计
- 办公工作区 `deliverables/` 中的交付物

## 会离开本机的数据

- 发往你所配置提供商的 **LLM 请求**（DeepSeek、NIM、OpenRouter、自建等）
- 启用时的 **Web 工具** — URL 与搜索词，受[网络策略](/zh-Hans/docs/settings/network)约束
- 使用 `describe_image` 时的**图像**
- **应用内更新** — 向 `zagens.com` 检查版本（不含聊天内容）

## 默认不做

- 核心聊天无需 Zagens 云账号
- 默认不上传仓库到 zagens.com

## 你的控制项

- 在[执行策略](/zh-Hans/docs/settings/exec-policy)关闭联网与 shell
- 对不可信工作区使用只读沙箱
- 通过 [API 设置](/zh-Hans/docs/settings/api-key) 选择提供商与区域

完整条款：[隐私政策](/zh-Hans/privacy) · [服务条款](/zh-Hans/terms)
