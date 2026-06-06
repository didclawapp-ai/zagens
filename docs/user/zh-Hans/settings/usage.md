# 用量与费用

**Usage** 检查器汇总各会话的 token 消耗与估算费用。

## 打开

Runtime 连接后，侧栏 → **Usage**（图表图标）。

## 内容

- 合计：输入/输出/缓存 token、估算 **USD** 费用
- **分组：** 按日、模型、提供商、会话
- 启用 prompt 缓存时显示命中率

**设置 → 系统** 中 `cost_currency` 可切换显示货币标签（USD/CNY）— 后端费用仍以 USD 计价。

## 数据来源

每回合完成后记录提供商返回的 usage；估算使用内置价目表，自建网关可能显示 $0 或近似值。

## 建议

- 用 [路由](/zh-Hans/docs/settings/routing) 对比 Flash 与 Pro 成本。
- 长 LHT 运行同时关注[上下文用量](/zh-Hans/docs/chat/context)。
- 用量仅存本机，不上传 zagens.com。

相关：[API Key](/zh-Hans/docs/settings/api-key) · [上下文](/zh-Hans/docs/chat/context)
