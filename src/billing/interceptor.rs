use crate::billing::{PricingCache, CostCalculator, TokenUsage};
use crate::core::entities::UnifiedRequest;
use crate::connectors::ConnectorResponse;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

/// Context for billing a single request
#[derive(Clone, Debug)]
pub struct BillingContext {
    pub request_id: String,
    pub tenant_id: String,
    pub api_key_id: Uuid,
    pub logical_model: String,
    pub provider: String,
    pub provider_model_id: String,
    pub start_time: Instant,
}

/// A single billing transaction record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingTransaction {
    pub id: Uuid,
    pub tenant_id: String,
    pub api_key_id: Uuid,
    pub request_id: String,
    pub logical_model: String,
    pub provider: String,
    pub provider_model_id: String,
    pub prompt_tokens: i64,
    pub completion_tokens: i64,
    pub reasoning_tokens: i64,
    pub cached_prompt_tokens: i64,
    pub total_tokens: i64,
    pub prompt_cost: f64,
    pub completion_cost: f64,
    pub reasoning_cost: f64,
    pub cache_read_cost: f64,
    pub request_cost: f64,
    pub total_cost: f64,
    pub pricing_snapshot: serde_json::Value,
    pub response_time_ms: i32,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: time::OffsetDateTime,
}

/// Billing interceptor for tracking usage and costs
pub struct BillingInterceptor {
    pricing_cache: Arc<PricingCache>,
}

impl BillingInterceptor {
    pub fn new(pricing_cache: Arc<PricingCache>) -> Self {
        Self { pricing_cache }
    }

    /// Create billing context before request
    pub fn before_request(
        &self,
        req: &UnifiedRequest,
        tenant_id: String,
        api_key_id: Uuid,
        provider: String,
        provider_model_id: String,
    ) -> BillingContext {
        BillingContext {
            request_id: Uuid::new_v4().to_string(),
            tenant_id,
            api_key_id,
            logical_model: req.logical_model.clone(),
            provider,
            provider_model_id,
            start_time: Instant::now(),
        }
    }

    /// Process billing after request completes
    pub async fn after_request(
        &self,
        ctx: BillingContext,
        response: &ConnectorResponse,
        status: &str,
        error_message: Option<String>,
    ) -> anyhow::Result<BillingTransaction> {
        // 1. Extract usage from response
        let usage = self.extract_usage(response).unwrap_or_default();

        // 2. Fetch pricing
        let pricing = self.pricing_cache.get(&ctx.provider_model_id).await?;

        // 3. Calculate cost breakdown
        let breakdown = CostCalculator::compute(&usage, &pricing);

        // 4. Build transaction record
        let transaction = BillingTransaction {
            id: Uuid::new_v4(),
            tenant_id: ctx.tenant_id,
            api_key_id: ctx.api_key_id,
            request_id: ctx.request_id,
            logical_model: ctx.logical_model,
            provider: ctx.provider,
            provider_model_id: ctx.provider_model_id,

            prompt_tokens: breakdown.prompt_tokens as i64,
            completion_tokens: breakdown.completion_tokens as i64,
            reasoning_tokens: breakdown.reasoning_tokens as i64,
            cached_prompt_tokens: breakdown.cached_prompt_tokens as i64,
            total_tokens: (breakdown.prompt_tokens + breakdown.completion_tokens) as i64,

            prompt_cost: breakdown.prompt_cost,
            completion_cost: breakdown.completion_cost,
            reasoning_cost: breakdown.internal_reasoning_cost,
            cache_read_cost: breakdown.cache_read_cost,
            request_cost: breakdown.request_cost,
            total_cost: breakdown.total_cost,

            pricing_snapshot: serde_json::to_value(&pricing)?,
            response_time_ms: ctx.start_time.elapsed().as_millis() as i32,
            status: status.to_string(),
            error_message,
            created_at: time::OffsetDateTime::now_utc(),
        };

        Ok(transaction)
    }

    /// Extract usage from connector response
    fn extract_usage(&self, response: &ConnectorResponse) -> anyhow::Result<TokenUsage> {
        match response {
            ConnectorResponse::NonStreaming(chunk) => {
                if let Some(events) = &chunk.provider_events {
                    // Try OpenRouter format
                    if let Some(usage_obj) = events.get("usage") {
                        let or_usage: crate::billing::OrUsage = serde_json::from_value(
                            serde_json::json!({"usage": usage_obj})
                        )?;
                        return Ok(or_usage.into_token_usage());
                    }

                    // Try Vertex AI format
                    if let Some(metadata) = events.get("usageMetadata") {
                        let prompt = metadata.get("promptTokenCount")
                            .and_then(|v| v.as_u64()).unwrap_or(0);
                        let completion = metadata.get("candidatesTokenCount")
                            .and_then(|v| v.as_u64()).unwrap_or(0);
                        let reasoning = metadata.get("thoughts_token_count")
                            .and_then(|v| v.as_u64()).unwrap_or(0);

                        return Ok(TokenUsage {
                            prompt_tokens: prompt,
                            completion_tokens: completion,
                            reasoning_tokens: reasoning,
                            cached_prompt_tokens: 0,
                        });
                    }
                }

                // Fallback: no usage data
                Ok(TokenUsage::default())
            }
            ConnectorResponse::Streaming(_) => {
                // For streaming, usage should be accumulated in the last chunk
                // This is handled at the routing layer
                Ok(TokenUsage::default())
            }
        }
    }
}
