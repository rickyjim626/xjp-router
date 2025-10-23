use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;
use serde::{Serialize, Deserialize};

// Re-export BillingTransaction from billing module
pub use crate::billing::BillingTransaction;

/// Summary of costs for a time period
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CostSummary {
    pub total_requests: i64,
    pub successful_requests: i64,
    pub failed_requests: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
}

/// Trait for billing data storage
#[async_trait]
pub trait BillingStore: Send + Sync {
    /// Insert a billing transaction (idempotent by request_id)
    async fn insert_transaction(&self, tx: BillingTransaction) -> Result<(), sqlx::Error>;

    /// Get transactions for a tenant with pagination
    async fn get_transactions_by_tenant(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BillingTransaction>, sqlx::Error>;

    /// Get cost summary for a tenant within a time range
    async fn get_cost_summary(
        &self,
        tenant_id: &str,
        start: time::OffsetDateTime,
        end: time::OffsetDateTime,
    ) -> Result<CostSummary, sqlx::Error>;

    /// Get transactions by API key
    async fn get_transactions_by_api_key(
        &self,
        api_key_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BillingTransaction>, sqlx::Error>;
}

/// PostgreSQL implementation of BillingStore
pub struct PgBillingStore {
    pool: PgPool,
}

impl PgBillingStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BillingStore for PgBillingStore {
    async fn insert_transaction(&self, tx: BillingTransaction) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO billing_transactions (
                id, tenant_id, api_key_id, request_id, logical_model, provider, provider_model_id,
                prompt_tokens, completion_tokens, reasoning_tokens, cached_prompt_tokens, total_tokens,
                prompt_cost, completion_cost, reasoning_cost, cache_read_cost, request_cost, total_cost,
                pricing_snapshot, response_time_ms, status, error_message, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
            ON CONFLICT (request_id) DO NOTHING
            "#,
            tx.id,
            tx.tenant_id,
            tx.api_key_id,
            tx.request_id,
            tx.logical_model,
            tx.provider,
            tx.provider_model_id,
            tx.prompt_tokens,
            tx.completion_tokens,
            tx.reasoning_tokens,
            tx.cached_prompt_tokens,
            tx.total_tokens,
            tx.prompt_cost,
            tx.completion_cost,
            tx.reasoning_cost,
            tx.cache_read_cost,
            tx.request_cost,
            tx.total_cost,
            tx.pricing_snapshot,
            tx.response_time_ms,
            tx.status,
            tx.error_message,
            tx.created_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_transactions_by_tenant(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BillingTransaction>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id, tenant_id, api_key_id, request_id, logical_model, provider, provider_model_id,
                prompt_tokens, completion_tokens, reasoning_tokens, cached_prompt_tokens, total_tokens,
                prompt_cost, completion_cost, reasoning_cost, cache_read_cost, request_cost, total_cost,
                pricing_snapshot, response_time_ms, status, error_message, created_at
            FROM billing_transactions
            WHERE tenant_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            tenant_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let transactions = rows
            .into_iter()
            .map(|row| BillingTransaction {
                id: row.id,
                tenant_id: row.tenant_id,
                api_key_id: row.api_key_id,
                request_id: row.request_id,
                logical_model: row.logical_model,
                provider: row.provider,
                provider_model_id: row.provider_model_id,
                prompt_tokens: row.prompt_tokens,
                completion_tokens: row.completion_tokens,
                reasoning_tokens: row.reasoning_tokens,
                cached_prompt_tokens: row.cached_prompt_tokens,
                total_tokens: row.total_tokens,
                prompt_cost: row.prompt_cost,
                completion_cost: row.completion_cost,
                reasoning_cost: row.reasoning_cost,
                cache_read_cost: row.cache_read_cost,
                request_cost: row.request_cost,
                total_cost: row.total_cost,
                pricing_snapshot: row.pricing_snapshot,
                response_time_ms: row.response_time_ms.unwrap_or(0),
                status: row.status,
                error_message: row.error_message,
                created_at: row.created_at,
            })
            .collect();

        Ok(transactions)
    }

    async fn get_cost_summary(
        &self,
        tenant_id: &str,
        start: time::OffsetDateTime,
        end: time::OffsetDateTime,
    ) -> Result<CostSummary, sqlx::Error> {
        let row = sqlx::query!(
            r#"
            SELECT
                COUNT(*) as total_requests,
                COALESCE(SUM(total_tokens), 0) as total_tokens,
                COALESCE(SUM(total_cost), 0) as total_cost,
                COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as successful_requests,
                COALESCE(SUM(CASE WHEN status != 'success' THEN 1 ELSE 0 END), 0) as failed_requests
            FROM billing_transactions
            WHERE tenant_id = $1
              AND created_at >= $2
              AND created_at < $3
            "#,
            tenant_id,
            start,
            end
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(CostSummary {
            total_requests: row.total_requests.unwrap_or(0),
            successful_requests: row.successful_requests.unwrap_or(0),
            failed_requests: row.failed_requests.unwrap_or(0),
            total_tokens: row.total_tokens.unwrap_or(0),
            total_cost: row.total_cost.unwrap_or(0.0),
        })
    }

    async fn get_transactions_by_api_key(
        &self,
        api_key_id: Uuid,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BillingTransaction>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT
                id, tenant_id, api_key_id, request_id, logical_model, provider, provider_model_id,
                prompt_tokens, completion_tokens, reasoning_tokens, cached_prompt_tokens, total_tokens,
                prompt_cost, completion_cost, reasoning_cost, cache_read_cost, request_cost, total_cost,
                pricing_snapshot, response_time_ms, status, error_message, created_at
            FROM billing_transactions
            WHERE api_key_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            api_key_id,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        let transactions = rows
            .into_iter()
            .map(|row| BillingTransaction {
                id: row.id,
                tenant_id: row.tenant_id,
                api_key_id: row.api_key_id,
                request_id: row.request_id,
                logical_model: row.logical_model,
                provider: row.provider,
                provider_model_id: row.provider_model_id,
                prompt_tokens: row.prompt_tokens,
                completion_tokens: row.completion_tokens,
                reasoning_tokens: row.reasoning_tokens,
                cached_prompt_tokens: row.cached_prompt_tokens,
                total_tokens: row.total_tokens,
                prompt_cost: row.prompt_cost,
                completion_cost: row.completion_cost,
                reasoning_cost: row.reasoning_cost,
                cache_read_cost: row.cache_read_cost,
                request_cost: row.request_cost,
                total_cost: row.total_cost,
                pricing_snapshot: row.pricing_snapshot,
                response_time_ms: row.response_time_ms.unwrap_or(0),
                status: row.status,
                error_message: row.error_message,
                created_at: row.created_at,
            })
            .collect();

        Ok(transactions)
    }
}
