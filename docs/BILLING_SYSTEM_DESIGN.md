# 基于 API Key 的实时计费系统设计方案

## 📋 系统概述

设计一个实时计费系统，在每次 API 调用时自动记录：
- **租户识别**：通过 API Key 区分不同用户
- **用量追踪**：记录输入/输出 token、缓存命中、推理 token
- **成本计算**：实时计算并存储每次请求的成本
- **账单生成**：支持按 API Key 查询历史账单与统计

## 🏗️ 架构设计

### 1. 数据流程

```
用户请求 (带 API Key)
    ↓
认证中间件 (提取 API Key & Tenant ID)
    ↓
路由层 (调用模型提供商)
    ↓
计费拦截器 (BillingInterceptor)
    ├── 请求前: 记录请求元数据
    ├── 请求后: 提取 usage、计算成本
    └── 异步落库: 写入 billing_transactions 表
    ↓
返回响应给用户
```

### 2. 数据库设计

#### 2.1 计费事务表 (billing_transactions)

```sql
CREATE TABLE billing_transactions (
    -- 主键
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 租户与 API Key
    tenant_id VARCHAR(255) NOT NULL,           -- 从 api_keys 表关联
    api_key_id UUID NOT NULL REFERENCES api_keys(id),

    -- 请求元数据
    request_id VARCHAR(255) NOT NULL UNIQUE,   -- 唯一请求 ID (用于幂等)
    logical_model VARCHAR(100) NOT NULL,       -- 用户请求的逻辑模型 (如 claude-sonnet-4.5)
    provider VARCHAR(50) NOT NULL,             -- 实际使用的提供商 (OpenRouter/Vertex/Clewdr)
    provider_model_id VARCHAR(255) NOT NULL,   -- 提供商的模型 ID

    -- Token 用量
    prompt_tokens BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens BIGINT NOT NULL DEFAULT 0,
    cached_prompt_tokens BIGINT NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,

    -- 成本明细 (USD)
    prompt_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    completion_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    reasoning_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    cache_read_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    request_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,
    total_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,

    -- 价格快照 (记录请求时的单价，用于审计)
    pricing_snapshot JSONB NOT NULL,           -- ModelPricing 序列化

    -- 响应元数据
    response_time_ms INTEGER,                  -- 响应时间 (毫秒)
    status VARCHAR(20) NOT NULL,               -- success / error / timeout
    error_message TEXT,                        -- 错误信息 (如有)

    -- 时间戳
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 索引优化
    CONSTRAINT positive_tokens CHECK (total_tokens >= 0),
    CONSTRAINT positive_cost CHECK (total_cost >= 0)
);

-- 索引
CREATE INDEX idx_billing_tenant_time ON billing_transactions(tenant_id, created_at DESC);
CREATE INDEX idx_billing_api_key_time ON billing_transactions(api_key_id, created_at DESC);
CREATE INDEX idx_billing_created_at ON billing_transactions(created_at DESC);
CREATE INDEX idx_billing_request_id ON billing_transactions(request_id);
CREATE INDEX idx_billing_status ON billing_transactions(status);
```

#### 2.2 租户账单汇总表 (tenant_billing_summary)

用于加速按月/按天查询：

```sql
CREATE TABLE tenant_billing_summary (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id VARCHAR(255) NOT NULL,
    api_key_id UUID NOT NULL REFERENCES api_keys(id),

    -- 时间维度
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    period_type VARCHAR(20) NOT NULL,          -- 'daily' / 'monthly'

    -- 统计
    total_requests BIGINT NOT NULL DEFAULT 0,
    successful_requests BIGINT NOT NULL DEFAULT 0,
    failed_requests BIGINT NOT NULL DEFAULT 0,

    total_tokens BIGINT NOT NULL DEFAULT 0,
    total_cost DECIMAL(12, 8) NOT NULL DEFAULT 0,

    -- 按模型分组 (JSON)
    model_breakdown JSONB NOT NULL,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(tenant_id, api_key_id, period_type, period_start)
);

CREATE INDEX idx_summary_tenant_period ON tenant_billing_summary(tenant_id, period_type, period_start DESC);
```

### 3. 代码实现

#### 3.1 计费拦截器 (src/billing/interceptor.rs)

```rust
use crate::billing::{PricingCache, CostCalculator, TokenUsage};
use crate::db::BillingStore;
use crate::core::entities::UnifiedRequest;
use crate::connectors::ConnectorResponse;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

pub struct BillingInterceptor {
    pricing_cache: Arc<PricingCache>,
    billing_store: Arc<dyn BillingStore>,
}

#[derive(Clone)]
pub struct BillingContext {
    pub request_id: String,
    pub tenant_id: String,
    pub api_key_id: Uuid,
    pub logical_model: String,
    pub provider: String,
    pub provider_model_id: String,
    pub start_time: Instant,
}

impl BillingInterceptor {
    pub fn new(pricing_cache: Arc<PricingCache>, billing_store: Arc<dyn BillingStore>) -> Self {
        Self { pricing_cache, billing_store }
    }

    /// 请求前：创建计费上下文
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

    /// 请求后：提取 usage、计算成本、落库
    pub async fn after_request(
        &self,
        ctx: BillingContext,
        response: &ConnectorResponse,
        status: &str,
        error_message: Option<String>,
    ) -> anyhow::Result<()> {
        // 1. 提取 usage
        let usage = self.extract_usage(response)?;

        // 2. 获取价格
        let pricing = self.pricing_cache.get(&ctx.provider_model_id).await?;

        // 3. 计算成本
        let breakdown = CostCalculator::compute(&usage, &pricing);

        // 4. 构建事务记录
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

        // 5. 异步写入数据库（不阻塞响应）
        let store = self.billing_store.clone();
        tokio::spawn(async move {
            if let Err(e) = store.insert_transaction(transaction).await {
                tracing::error!("Failed to record billing transaction: {}", e);
            }
        });

        Ok(())
    }

    fn extract_usage(&self, response: &ConnectorResponse) -> anyhow::Result<TokenUsage> {
        // 从 provider_events 中提取 usage
        match response {
            ConnectorResponse::Streaming(stream) => {
                // 流式：需要累计最后一个 chunk 的 provider_events
                // (实际实现中应在 routing 层累计)
                Err(anyhow::anyhow!("Streaming usage extraction not implemented yet"))
            }
            ConnectorResponse::NonStreaming(chunk) => {
                if let Some(events) = &chunk.provider_events {
                    // OpenRouter format
                    if let Some(usage_obj) = events.get("usage") {
                        let or_usage: crate::billing::OrUsage = serde_json::from_value(
                            serde_json::json!({"usage": usage_obj})
                        )?;
                        return Ok(or_usage.into_token_usage());
                    }

                    // Vertex AI format
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

                // Fallback: 无 usage 时返回 0
                Ok(TokenUsage::default())
            }
        }
    }
}

#[derive(Debug, Clone)]
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
```

#### 3.2 数据库存储层 (src/db/billing.rs)

```rust
use crate::billing::interceptor::BillingTransaction;
use sqlx::PgPool;
use async_trait::async_trait;

#[async_trait]
pub trait BillingStore: Send + Sync {
    async fn insert_transaction(&self, tx: BillingTransaction) -> Result<(), sqlx::Error>;
    async fn get_transactions_by_tenant(
        &self,
        tenant_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BillingTransaction>, sqlx::Error>;
    async fn get_cost_summary(
        &self,
        tenant_id: &str,
        start: time::OffsetDateTime,
        end: time::OffsetDateTime,
    ) -> Result<CostSummary, sqlx::Error>;
}

pub struct PgBillingStore {
    pool: PgPool,
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
            tx.prompt_cost as f64,
            tx.completion_cost as f64,
            tx.reasoning_cost as f64,
            tx.cache_read_cost as f64,
            tx.request_cost as f64,
            tx.total_cost as f64,
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
        let rows = sqlx::query_as!(
            BillingTransactionRow,
            r#"
            SELECT * FROM billing_transactions
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

        Ok(rows.into_iter().map(|r| r.into()).collect())
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
                COALESCE(SUM(CASE WHEN status = 'success' THEN 1 ELSE 0 END), 0) as successful_requests
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
            total_tokens: row.total_tokens.unwrap_or(0),
            total_cost: row.total_cost.unwrap_or(0.0),
        })
    }
}

pub struct CostSummary {
    pub total_requests: i64,
    pub successful_requests: i64,
    pub total_tokens: i64,
    pub total_cost: f64,
}
```

#### 3.3 集成到路由层 (src/routing.rs)

```rust
use crate::billing::BillingInterceptor;
use crate::db::BillingStore;

pub struct AppState {
    registry: Arc<ModelRegistry>,
    openrouter: Arc<dyn Connector>,
    vertex: Arc<dyn Connector>,
    clewdr: Arc<dyn Connector>,
    key_store: Arc<dyn KeyStore>,
    pub pricing: Arc<PricingCache>,
    billing_interceptor: Arc<BillingInterceptor>,
}

impl AppState {
    pub async fn new(
        registry: ModelRegistry,
        key_store: Arc<dyn KeyStore>,
        secret_provider: Arc<dyn SecretProvider>,
        preloaded_secrets: HashMap<String, String>,
        billing_store: Arc<dyn BillingStore>,
    ) -> anyhow::Result<Self> {
        let pricing = Arc::new(PricingCache::new()?);
        Ok(Self {
            registry: Arc::new(registry),
            openrouter: Arc::new(connectors::openrouter::OpenRouterConnector::new(
                secret_provider.clone(),
                &preloaded_secrets,
            )?),
            vertex: Arc::new(connectors::vertex::VertexConnector::new(
                secret_provider.clone(),
                &preloaded_secrets,
            ).await?),
            clewdr: Arc::new(connectors::clewdr::ClewdrConnector::new(
                secret_provider,
                &preloaded_secrets,
            )?),
            key_store,
            pricing: pricing.clone(),
            billing_interceptor: Arc::new(BillingInterceptor::new(pricing, billing_store)),
        })
    }

    pub async fn invoke_with_billing(
        &self,
        req: UnifiedRequest,
        tenant_id: String,
        api_key_id: Uuid,
    ) -> Result<ConnectorResponse, ConnectorError> {
        let route = self
            .registry
            .resolve(&req.logical_model)
            .map_err(|e| ConnectorError::Invalid(e.to_string()))?;

        // 创建计费上下文
        let billing_ctx = self.billing_interceptor.before_request(
            &req,
            tenant_id,
            api_key_id,
            route.provider.to_string(),
            route.provider_model_id.clone(),
        );

        // 执行实际调用
        let result = match route.provider {
            ProviderKind::OpenRouter => self.openrouter.invoke(route, req).await,
            ProviderKind::Vertex => self.vertex.invoke(route, req).await,
            ProviderKind::Clewdr => self.clewdr.invoke(route, req).await,
        };

        // 记录计费（异步，不阻塞）
        match &result {
            Ok(response) => {
                let _ = self.billing_interceptor.after_request(
                    billing_ctx,
                    response,
                    "success",
                    None,
                ).await;
            }
            Err(e) => {
                // 即使失败也记录（用于统计错误率）
                let _ = self.billing_interceptor.after_request(
                    billing_ctx,
                    &ConnectorResponse::NonStreaming(Default::default()),
                    "error",
                    Some(e.to_string()),
                ).await;
            }
        }

        result
    }
}
```

#### 3.4 查询 API (src/api/billing.rs 扩展)

```rust
#[derive(Deserialize)]
pub struct BillingQueryParams {
    pub tenant_id: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 { 100 }

pub async fn get_transactions(
    State(app): State<AppState>,
    Query(params): Query<BillingQueryParams>,
) -> impl IntoResponse {
    match app.billing_store.get_transactions_by_tenant(
        &params.tenant_id,
        params.limit,
        params.offset,
    ).await {
        Ok(transactions) => Json(json!({ "transactions": transactions })),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}

#[derive(Deserialize)]
pub struct SummaryQueryParams {
    pub tenant_id: String,
    pub start: String,  // ISO 8601 timestamp
    pub end: String,
}

pub async fn get_summary(
    State(app): State<AppState>,
    Query(params): Query<SummaryQueryParams>,
) -> impl IntoResponse {
    let start = match time::OffsetDateTime::parse(&params.start, &time::format_description::well_known::Iso8601::DEFAULT) {
        Ok(t) => t,
        Err(e) => return Json(json!({ "error": format!("Invalid start time: {}", e) })),
    };
    let end = match time::OffsetDateTime::parse(&params.end, &time::format_description::well_known::Iso8601::DEFAULT) {
        Ok(t) => t,
        Err(e) => return Json(json!({ "error": format!("Invalid end time: {}", e) })),
    };

    match app.billing_store.get_cost_summary(&params.tenant_id, start, end).await {
        Ok(summary) => Json(json!(summary)),
        Err(e) => Json(json!({ "error": e.to_string() })),
    }
}
```

### 4. API 端点

#### 4.1 查询历史交易

```bash
GET /internal/billing/transactions?tenant_id=tenant-123&limit=50&offset=0
```

**响应：**
```json
{
  "transactions": [
    {
      "id": "uuid",
      "tenant_id": "tenant-123",
      "request_id": "req-uuid",
      "logical_model": "claude-sonnet-4.5",
      "provider": "OpenRouter",
      "total_tokens": 2000,
      "total_cost": 0.0123,
      "status": "success",
      "created_at": "2025-10-23T10:30:00Z"
    }
  ]
}
```

#### 4.2 查询成本汇总

```bash
GET /internal/billing/summary?tenant_id=tenant-123&start=2025-10-01T00:00:00Z&end=2025-10-31T23:59:59Z
```

**响应：**
```json
{
  "total_requests": 1500,
  "successful_requests": 1480,
  "total_tokens": 3000000,
  "total_cost": 45.67
}
```

## 🔧 实施步骤

### Phase 1: 数据库迁移
1. 创建 `migrations/005_billing_transactions.sql`
2. 创建 `migrations/006_tenant_billing_summary.sql`
3. 运行 `sqlx migrate run`

### Phase 2: 核心代码
1. 实现 `BillingInterceptor` (src/billing/interceptor.rs)
2. 实现 `BillingStore` trait (src/db/billing.rs)
3. 实现 `PgBillingStore` (src/db/billing.rs)

### Phase 3: 集成
1. 修改 `AppState::new()` 添加 `billing_interceptor`
2. 修改 API handlers 调用 `invoke_with_billing()`
3. 添加查询 API endpoints

### Phase 4: 测试
1. 单元测试：计费逻辑
2. 集成测试：端到端计费流程
3. 性能测试：异步写入不阻塞响应

## 📊 监控与优化

### 1. 性能优化
- **异步写入**：计费记录异步落库，不阻塞响应
- **批量写入**：高并发场景可考虑批量插入（每 100 条或 1 秒）
- **索引优化**：按 tenant_id + created_at 创建复合索引

### 2. 数据保留策略
- 保留原始交易记录 90 天
- 汇总数据永久保留
- 定期归档到对象存储 (S3/GCS)

### 3. 告警规则
- 单个租户成本突增 (>上一周同期 200%)
- 失败率异常 (>10%)
- 数据库写入延迟 (>500ms)

## 🔒 安全考虑

1. **API Key 隔离**：每个查询 API 必须验证 tenant 权限
2. **数据脱敏**：日志中不记录完整 API Key
3. **审计日志**：计费数据修改必须记录操作人

## 📈 扩展功能

### 未来可添加
1. **预付费账户**：租户余额管理，不足时拒绝请求
2. **配额限制**：按 API Key 设置月度预算
3. **发票生成**：自动生成月度账单 PDF
4. **实时 Dashboard**：Web UI 展示实时消费
5. **Webhook 通知**：成本超阈值时发送告警

---

**实施优先级**：高 (P0)
**预计工期**：3-5 天
**依赖**：现有 billing 模块、数据库迁移
