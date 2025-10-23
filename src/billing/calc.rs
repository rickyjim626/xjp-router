use serde::{Serialize, Deserialize};
use crate::billing::price::ModelPricing;
use crate::billing::tokens::TokenUsage;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub reasoning_tokens: u64,
    pub cached_prompt_tokens: u64,
    pub prompt_cost: f64,
    pub completion_cost: f64,
    pub internal_reasoning_cost: f64,
    pub cache_read_cost: f64,
    pub request_cost: f64,
    pub total_cost: f64,
    pub unit: &'static str,
}

pub struct CostCalculator;

impl CostCalculator {
    pub fn compute(usage: &TokenUsage, price: &ModelPricing) -> CostBreakdown {
        let prompt_non_cached = usage.prompt_tokens.saturating_sub(usage.cached_prompt_tokens);
        let prompt_cost = (prompt_non_cached as f64) * price.prompt;
        let cache_read_cost = (usage.cached_prompt_tokens as f64) * price.input_cache_read;

        let completion_cost = (usage.completion_tokens as f64) * price.completion;
        let internal_reasoning_cost = (usage.reasoning_tokens as f64) * if price.internal_reasoning > 0.0 { price.internal_reasoning } else { price.completion };

        let request_cost = price.request;

        let total_cost = prompt_cost + cache_read_cost + completion_cost + internal_reasoning_cost + request_cost;

        CostBreakdown {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            cached_prompt_tokens: usage.cached_prompt_tokens,
            prompt_cost,
            completion_cost,
            internal_reasoning_cost,
            cache_read_cost,
            request_cost,
            total_cost,
            unit: "USD",
        }
    }
}
