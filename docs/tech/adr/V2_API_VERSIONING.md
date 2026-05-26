# `/v2` HTTP API 版本策略（草案）

| 字段 | 值 |
|------|-----|
| Status | **Draft** (D8 文档交付) |
| 当前生产 | **`/v1/*` only** |

## 原则

1. **破坏性变更** 必须新路径前缀（`/v2/...`）或显式 `Accept-Version` 协商；禁止 silent break `/v1` 已发布字段。
2. **D8 之后** 任何 `/v1` 形状变更须同步 [`zagens-runtime-v1.openapi.json`](../openapi/zagens-runtime-v1.openapi.json) 与 `web-ui` 生成类型。
3. **桌面壳** 与 sidecar 同版本发布；旧 Zagens + 新 sidecar 不在首版支持矩阵内。

## 何时引入 `/v2`

| 触发 | 示例 |
|------|------|
| 删除或重命名 JSON 字段 | `SessionMetadata` 去掉 `title` |
| 枚举值语义变更 | `RuntimeTurnStatus` 合并状态 |
| 错误体统一换形 | `ErrorEnvelope` 替代 `{ error: string }` |
| SSE 事件名 breaking | 重命名 `thinking.delta` |

非 breaking（新增可选字段、新端点）留在 `/v1` 并更新 OpenAPI。

## 迁移流程（未来）

1. 并行暴露 `/v2` 子集 + 文档化 diff。
2. `web-ui` 生成 `runtime-api-v2.ts`（或单 spec 多 server url）。
3. 至少 **一个 minor** Zagens 版本同时支持 v1+v2。
4. 遥测/日志确认 v1 流量为零后 deprecate v1（CHANGELOG + Assessment）。

## 与 Harness 提案

[`HARNESS_INTEGRATION_PROPOSAL.md`](./HARNESS_INTEGRATION_PROPOSAL.md) Phase 2 依赖 D8 生成的 TS；新 harness 端点优先 `/v1` 扩容，直到 v2 策略签收后再批量提升。
