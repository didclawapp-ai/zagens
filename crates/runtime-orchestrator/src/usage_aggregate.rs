//! Helpers for usage aggregation and cache-hit telemetry (#584).

use crate::models::Usage;

/// Whether the model id is expected to return DeepSeek-style cache telemetry.
#[must_use]
pub fn model_cache_telemetry_supported(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("deepseek-ai/") {
        return true;
    }
    lower.contains("deepseek")
}

/// Miss tokens for one usage row (infers from input − hit when miss omitted).
#[must_use]
pub fn miss_tokens_from_usage(usage: &Usage) -> u64 {
    let hit = usage.prompt_cache_hit_tokens.unwrap_or(0);
    usage
        .prompt_cache_miss_tokens
        .map(u64::from)
        .unwrap_or_else(|| u64::from(usage.input_tokens.saturating_sub(hit)))
}

/// Cache-hit percentage for one API usage row (0–100).
#[must_use]
pub fn usage_cache_hit_percent(usage: &Usage) -> f64 {
    let hit = usage.prompt_cache_hit_tokens.unwrap_or(0);
    if usage.input_tokens == 0 {
        return 0.0;
    }
    (f64::from(hit) * 100.0) / f64::from(usage.input_tokens)
}

/// Session/bucket cache-hit rate from summed hit and input tokens (0–100).
#[must_use]
pub fn aggregate_cache_hit_percent(cached_tokens: u64, input_tokens: u64) -> Option<f64> {
    if input_tokens == 0 {
        None
    } else {
        Some((cached_tokens as f64 * 100.0) / input_tokens as f64)
    }
}

/// Input cost if every input token were billed at cache-miss rate.
#[must_use]
pub fn cost_usd_if_no_cache(model: &str, usage: &Usage) -> Option<f64> {
    crate::pricing::calculate_turn_cost_estimate(model, usage.input_tokens, usage.output_tokens)
        .map(|e| e.usd)
}

/// Accumulate one turn's usage into session/bucket totals.
pub fn accumulate_turn_usage(
    totals: &mut crate::runtime_threads::UsageTotals,
    bucket: &mut crate::runtime_threads::UsageBucket,
    model: &str,
    usage: &Usage,
    cache_telemetry_incomplete: &mut bool,
) {
    if !model_cache_telemetry_supported(model) {
        *cache_telemetry_incomplete = true;
    }

    let cached = usage.prompt_cache_hit_tokens.unwrap_or(0) as u64;
    let miss = miss_tokens_from_usage(usage);
    let reasoning = usage.reasoning_tokens.unwrap_or(0) as u64;
    let input = usage.input_tokens as u64;
    let output = usage.output_tokens as u64;
    let cost = crate::pricing::calculate_turn_cost_from_usage(model, usage).unwrap_or(0.0);
    let cost_no_cache = cost_usd_if_no_cache(model, usage).unwrap_or(0.0);

    totals.input_tokens += input;
    totals.output_tokens += output;
    totals.cached_tokens += cached;
    totals.miss_tokens += miss;
    totals.reasoning_tokens += reasoning;
    totals.cost_usd += cost;
    totals.cost_usd_without_cache += cost_no_cache;
    totals.turns += 1;

    bucket.input_tokens += input;
    bucket.output_tokens += output;
    bucket.cached_tokens += cached;
    bucket.miss_tokens += miss;
    bucket.reasoning_tokens += reasoning;
    bucket.cost_usd += cost;
    bucket.cost_usd_without_cache += cost_no_cache;
    bucket.turns += 1;
}

/// Derive hit-rate and savings fields after summing raw counters.
pub fn finalize_usage_totals(totals: &mut crate::runtime_threads::UsageTotals) {
    totals.cache_hit_rate = aggregate_cache_hit_percent(totals.cached_tokens, totals.input_tokens);
    totals.cache_savings_usd = (totals.cost_usd_without_cache - totals.cost_usd).max(0.0);
}

pub fn finalize_usage_bucket(bucket: &mut crate::runtime_threads::UsageBucket) {
    bucket.cache_hit_rate = aggregate_cache_hit_percent(bucket.cached_tokens, bucket.input_tokens);
    bucket.cache_savings_usd = (bucket.cost_usd_without_cache - bucket.cost_usd).max(0.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Usage;

    #[test]
    fn miss_tokens_infers_from_input_minus_hit() {
        let usage = Usage {
            input_tokens: 100,
            output_tokens: 10,
            prompt_cache_hit_tokens: Some(70),
            prompt_cache_miss_tokens: None,
            reasoning_tokens: None,
            reasoning_replay_tokens: None,
            server_tool_use: None,
        };
        assert_eq!(miss_tokens_from_usage(&usage), 30);
    }

    #[test]
    fn aggregate_hit_rate_none_when_no_input() {
        assert!(aggregate_cache_hit_percent(50, 0).is_none());
        assert!((aggregate_cache_hit_percent(80, 100).unwrap() - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn openrouter_model_lacks_cache_telemetry() {
        assert!(!model_cache_telemetry_supported("openai/gpt-4"));
    }
}
