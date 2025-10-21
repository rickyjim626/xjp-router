use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Usage log entry
#[derive(Debug, Clone)]
pub struct UsageLog {
    pub api_key_id: Uuid,
    pub tenant_id: String,
    pub logical_model: String,
    pub provider: String,
    pub provider_model_id: String,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub total_tokens: i32,
    pub latency_ms: Option<i32>,
    pub status_code: i32,
    pub error_message: Option<String>,
    pub request_id: String,
}

/// Trait for usage logging
#[async_trait::async_trait]
pub trait UsageStore: Send + Sync {
    /// Log a usage event
    async fn log_usage(&self, log: UsageLog) -> Result<(), UsageStoreError>;

    /// Get usage summary for a tenant
    async fn get_tenant_usage(
        &self,
        tenant_id: &str,
        start_date: time::OffsetDateTime,
        end_date: time::OffsetDateTime,
    ) -> Result<Vec<UsageSummary>, UsageStoreError>;
}

/// Usage summary aggregation
#[derive(Debug, Clone)]
pub struct UsageSummary {
    pub date: time::Date,
    pub logical_model: String,
    pub provider: String,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub avg_latency_ms: f64,
    pub error_count: i64,
}

/// Errors that can occur during usage operations
#[derive(Debug, thiserror::Error)]
pub enum UsageStoreError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// PostgreSQL implementation of UsageStore
pub struct PgUsageStore {
    pool: PgPool,
}

impl PgUsageStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UsageStore for PgUsageStore {
    async fn log_usage(&self, log: UsageLog) -> Result<(), UsageStoreError> {
        sqlx::query(
            r#"
            INSERT INTO usage_logs (
                api_key_id, tenant_id, logical_model, provider, provider_model_id,
                input_tokens, output_tokens, total_tokens, latency_ms,
                status_code, error_message, request_id
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
            "#,
        )
        .bind(log.api_key_id)
        .bind(&log.tenant_id)
        .bind(&log.logical_model)
        .bind(&log.provider)
        .bind(&log.provider_model_id)
        .bind(log.input_tokens)
        .bind(log.output_tokens)
        .bind(log.total_tokens)
        .bind(log.latency_ms)
        .bind(log.status_code)
        .bind(&log.error_message)
        .bind(&log.request_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_tenant_usage(
        &self,
        tenant_id: &str,
        start_date: time::OffsetDateTime,
        end_date: time::OffsetDateTime,
    ) -> Result<Vec<UsageSummary>, UsageStoreError> {
        let rows = sqlx::query(
            r#"
            SELECT
                DATE(created_at) as date,
                logical_model,
                provider,
                COUNT(*) as request_count,
                SUM(input_tokens) as total_input_tokens,
                SUM(output_tokens) as total_output_tokens,
                SUM(total_tokens) as total_tokens,
                AVG(latency_ms) as avg_latency_ms,
                COUNT(CASE WHEN status_code >= 400 THEN 1 END) as error_count
            FROM usage_logs
            WHERE tenant_id = $1
              AND created_at >= $2
              AND created_at < $3
            GROUP BY DATE(created_at), logical_model, provider
            ORDER BY date DESC, logical_model, provider
            "#,
        )
        .bind(tenant_id)
        .bind(start_date)
        .bind(end_date)
        .fetch_all(&self.pool)
        .await?;

        let summaries = rows
            .into_iter()
            .map(|row| {
                Ok(UsageSummary {
                    date: row.try_get("date")?,
                    logical_model: row.get("logical_model"),
                    provider: row.get("provider"),
                    request_count: row.get("request_count"),
                    total_input_tokens: row.get("total_input_tokens"),
                    total_output_tokens: row.get("total_output_tokens"),
                    total_tokens: row.get("total_tokens"),
                    avg_latency_ms: row.get("avg_latency_ms"),
                    error_count: row.get("error_count"),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(summaries)
    }
}
