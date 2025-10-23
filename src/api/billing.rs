use axum::{extract::{State, Query}, Json, response::IntoResponse};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{routing::AppState, billing::{CostCalculator, OrUsage}};

#[derive(Deserialize)]
pub struct QuoteBody {
    pub provider_model_id: String,
    #[serde(default)]
    pub usage: Option<OrUsage>,
}

pub async fn quote(
    State(app): State<AppState>,
    Json(body): Json<QuoteBody>,
) -> impl IntoResponse {
    let pricing = match app.pricing.get(&body.provider_model_id).await {
        Ok(p) => p,
        Err(e) => return axum::Json(serde_json::json!({ "error": e.to_string() })),
    };

    if let Some(or_usage) = body.usage {
        let usage = or_usage.into_token_usage();
        let breakdown = CostCalculator::compute(&usage, &pricing);
        axum::Json(serde_json::json!({
            "pricing": {
                "prompt": pricing.prompt,
                "completion": pricing.completion,
                "request": pricing.request,
                "image": pricing.image,
                "internal_reasoning": pricing.internal_reasoning,
                "input_cache_read": pricing.input_cache_read,
                "input_cache_write": pricing.input_cache_write
            },
            "usage": usage,
            "breakdown": breakdown
        }))
    } else {
        axum::Json(serde_json::json!({
            "pricing_only": {
                "prompt": pricing.prompt,
                "completion": pricing.completion,
                "request": pricing.request,
                "image": pricing.image,
                "internal_reasoning": pricing.internal_reasoning,
                "input_cache_read": pricing.input_cache_read,
                "input_cache_write": pricing.input_cache_write
            }
        }))
    }
}

// Query billing transactions
#[derive(Deserialize)]
pub struct TransactionQueryParams {
    pub tenant_id: Option<String>,
    pub api_key_id: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    100
}

pub async fn get_transactions(
    State(app): State<AppState>,
    Query(params): Query<TransactionQueryParams>,
) -> impl IntoResponse {
    if let Some(tenant_id) = params.tenant_id {
        match app
            .billing_store()
            .get_transactions_by_tenant(&tenant_id, params.limit, params.offset)
            .await
        {
            Ok(transactions) => axum::Json(serde_json::json!({
                "transactions": transactions,
                "limit": params.limit,
                "offset": params.offset
            })),
            Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
        }
    } else if let Some(api_key_id_str) = params.api_key_id {
        let api_key_id = match Uuid::parse_str(&api_key_id_str) {
            Ok(id) => id,
            Err(e) => {
                return axum::Json(serde_json::json!({ "error": format!("Invalid API key ID: {}", e) }))
            }
        };
        match app
            .billing_store()
            .get_transactions_by_api_key(api_key_id, params.limit, params.offset)
            .await
        {
            Ok(transactions) => axum::Json(serde_json::json!({
                "transactions": transactions,
                "limit": params.limit,
                "offset": params.offset
            })),
            Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
        }
    } else {
        axum::Json(serde_json::json!({ "error": "Must provide tenant_id or api_key_id" }))
    }
}

// Get cost summary
#[derive(Deserialize)]
pub struct SummaryQueryParams {
    pub tenant_id: String,
    pub start: String, // ISO 8601
    pub end: String,   // ISO 8601
}

pub async fn get_summary(
    State(app): State<AppState>,
    Query(params): Query<SummaryQueryParams>,
) -> impl IntoResponse {
    // Parse timestamps
    let start = match time::OffsetDateTime::parse(
        &params.start,
        &time::format_description::well_known::Iso8601::DEFAULT,
    ) {
        Ok(t) => t,
        Err(e) => {
            return axum::Json(
                serde_json::json!({ "error": format!("Invalid start time: {}", e) }),
            )
        }
    };
    let end = match time::OffsetDateTime::parse(
        &params.end,
        &time::format_description::well_known::Iso8601::DEFAULT,
    ) {
        Ok(t) => t,
        Err(e) => {
            return axum::Json(serde_json::json!({ "error": format!("Invalid end time: {}", e) }))
        }
    };

    match app
        .billing_store()
        .get_cost_summary(&params.tenant_id, start, end)
        .await
    {
        Ok(summary) => axum::Json(serde_json::json!(summary)),
        Err(e) => axum::Json(serde_json::json!({ "error": e.to_string() })),
    }
}
