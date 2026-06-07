# KV cache (prefix cache) observability guide

| Field | Value |
|-------|-------|
| **Audience** | Zagens / runtime maintainers, cost optimization |
| **API reference** | [DeepSeek context disk cache](https://api-docs.deepseek.com/guides/kv_cache) |
| **Related code** | `crates/runtime-orchestrator/src/pricing.rs`, `usage_aggregate.rs`; `GET /v1/usage` |

## How hit rate is calculated

Each API turn's `usage` may include:

- `prompt_cache_hit_tokens`
- `prompt_cache_miss_tokens`
- `input_tokens`

**Recommended formula** (aligned with compaction `/cache` and footer chip):

```text
cache_hit_rate = prompt_cache_hit_tokens / input_tokens × 100%
```

Use `input_tokens` as the denominator, not `hit + miss` (some providers only report hits, which inflates to 100%).

When `prompt_cache_miss_tokens` is absent, aggregate with `input_tokens − hit` as inferred miss (display and "without cache" estimate only).

## Where to view

| Entry | Description |
|-------|-------------|
| **Zagens usage panel** | Hit rate %, miss tokens, estimated savings (USD) |
| **Composer footer** | Last turn `cache XX%` (&lt;40% warning color) |
| **`GET /v1/usage`** | `totals.cache_hit_rate`, `miss_tokens`, `cache_savings_usd`, `cache_telemetry_incomplete` |
| **`/cache` (CLI)** | Per-turn hit/miss for last N turns (desktop HTTP parity deferred) |

## Session-level `cached_tokens / input_tokens` limitation

`GET /v1/usage` **sums `input_tokens` per turn**. When a turn has multiple tool/API rounds, each round's `input_tokens` includes the full prefix, so:

- Session-level `cache_hit_rate` is a **coarse metric**, often slightly below per-turn `/cache`;
- **Billing** still uses per-round `usage` hit/miss split (`pricing::calculate_turn_cost_from_usage`), independent of display aggregation.

For precise prefix jitter, use per-turn telemetry or log `target=compaction` `cache_hit_pct`.

## Providers and telemetry

| Provider | `cache_telemetry_incomplete` |
|----------|------------------------------|
| DeepSeek / DeepSeek CN / NVIDIA NIM (DeepSeek models) | Usually `false` |
| OpenRouter, Ollama, etc. | `true` — cost estimated as all miss; actual bill may be lower |

Usage panel shows a notice when `cache_telemetry_incomplete=true`.

## Sub-agents / RLM

Child-round `child_prompt_cache_*` is stored in tool metadata and is **not** rolled up to `GET /v1/usage` session totals. Large audit runs may under-report hit tokens on the main session panel.

## When prefix cache breaks (hit rate drops)

- `/compact`, `.deepseek/handoff.md` rewrite
- Rewriting or reordering history messages
- Sub-agent new session (cold start)
- Re-reading the same file (tool result envelope changes)

Product strategy: static content in system prompt (static→dynamic layering); working set in `<turn_meta>` (see [`prompt-architecture.md`](../prompt-architecture.md)).

## Cost sensitivity (V4-Pro, example during discount window)

| Hit rate | Input-side savings vs all miss |
|----------|--------------------------------|
| 80% | ~**79%** |
| 50% | ~**50%** |
| 40% | Footer chip enters warning color |

Discount end date: `pricing.rs` `v4_pro_discount_ends_at` (was 2026-05-31 15:59 UTC when this doc was written).

## Ops checklist

1. After turn 3, is average hit rate **&gt; 70%** (`/cache` recommended)?
2. Only consider `/compact` after **&lt; 40%** for several consecutive turns — do not compact frequently for small token savings.
3. Confirm model uses native DeepSeek endpoint (hit/miss fields present).
4. Compare `cost_usd` vs `cost_usd_without_cache` to understand cache savings.
