# KV 缓存（前缀缓存）观测指南

| 字段 | 值 |
|------|-----|
| **读者** | Zagens / runtime 维护者、费用优化 |
| **API 参考** | [DeepSeek 上下文硬盘缓存](https://api-docs.deepseek.com/zh-cn/guides/kv_cache) |
| **相关代码** | `crates/runtime-orchestrator/src/pricing.rs`、`usage_aggregate.rs`；`GET /v1/usage` |

## 命中率怎么算

每个 API 回合的 `usage` 可能包含：

- `prompt_cache_hit_tokens`
- `prompt_cache_miss_tokens`
- `input_tokens`

**推荐口径**（与 compaction `/cache`、页脚 chip 一致）：

```text
cache_hit_rate = prompt_cache_hit_tokens / input_tokens × 100%
```

分母用 `input_tokens`，不用 `hit + miss`（部分提供商只上报 hit 时会虚高到 100%）。

若未上报 `prompt_cache_miss_tokens`，聚合时用 `input_tokens − hit` 推算 miss（仅用于展示与「若无缓存」估价）。

## 在哪里看

| 入口 | 说明 |
|------|------|
| **Zagens 用量面板** | 命中率 %、miss token、估算节省（USD） |
| **Composer 页脚** | 上一轮完成后的 `cache XX%`（&lt;40% 警示色） |
| **`GET /v1/usage`** | `totals.cache_hit_rate`、`miss_tokens`、`cache_savings_usd`、`cache_telemetry_incomplete` |
| **`/cache`（CLI）** | 最近 N 轮逐轮 hit/miss（桌面 HTTP 对等 API 暂缓） |

## 会话级 `cached_tokens / input_tokens` 的局限

`GET /v1/usage` 对**每个 turn 的 `input_tokens` 求和**。同一 turn 内多轮 tool/API 时，每轮 `input_tokens` 都含完整前缀，因此：

- 会话级 `cache_hit_rate` 是**粗指标**，常略低于逐轮 `/cache`；
- **计费**仍按每轮 `usage` 的 hit/miss 拆分（`pricing::calculate_turn_cost_from_usage`），与展示聚合无关。

精确排查前缀抖动请用逐轮遥测或日志 `target=compaction` 的 `cache_hit_pct`。

## 提供商与遥测

| 提供商 | `cache_telemetry_incomplete` |
|--------|------------------------------|
| DeepSeek / DeepSeek CN / NVIDIA NIM（DeepSeek 模型） | 通常 `false` |
| OpenRouter、Ollama 等 | `true` — 成本按全 miss 估算，实际账单可能更低 |

用量面板在 `cache_telemetry_incomplete=true` 时会显示提示。

## 子代理 / RLM

子回合的 `child_prompt_cache_*` 写在工具元数据中，**尚未** rollup 到 `GET /v1/usage` 会话总账。大 audit 场景主会话面板可能低估命中 token。

## 何时会打碎前缀（命中率骤降）

- `/compact`、`.deepseek/handoff.md` 改写
- 改写或重排历史 messages
- 子代理新 session（冷启动）
- 同一文件重复 Read（tool 结果信封变化）

产品策略：静态内容在 system prompt（静→动分层）；工作集在 `<turn_meta>`（见 [`prompt-architecture.md`](../prompt-architecture.md)）。

## 费用敏感度（V4-Pro，折扣期内示例）

| 命中率 | 输入侧相对全 miss |
|--------|-------------------|
| 80% | 约省 **79%** |
| 50% | 约省 **50%** |
| 40% | 页脚 chip 进入警示色 |

折扣截止以 `pricing.rs` 中 `v4_pro_discount_ends_at` 为准（当前文档编写时为 2026-05-31 15:59 UTC）。

## 运维检查清单

1. 第 3 轮后平均命中率是否 **&gt; 70%**（`/cache` 建议）。
2. 连续多轮 **&lt; 40%** 再考虑 `/compact`，勿为小幅省 token 频繁压缩。
3. 确认模型走 DeepSeek 原生端点（有 hit/miss 字段）。
4. 对比 `cost_usd` 与 `cost_usd_without_cache` 理解缓存带来的节省。
