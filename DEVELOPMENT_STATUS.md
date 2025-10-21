# XiaojinPro Gateway - 开发状态报告

**生成时间**: 2025-10-21
**项目状态**: 🟡 开发中 (阶段 0 完成，阶段 1-4 进行中)

---

## 📈 总体进度

| 阶段 | 状态 | 完成度 | 说明 |
|------|------|--------|------|
| 阶段 0: 代码质量 | ✅ 完成 | 100% | .gitignore, CI/CD, 代码格式化 |
| 阶段 1: 生产基础设施 | 🟡 进行中 | 0% | PostgreSQL, 限流, 指标, 追踪 |
| 阶段 2: 连接器完善 | 🟡 进行中 | 33% | OpenRouter完成,Vertex/Clewdr待实现 |
| 阶段 3: 弹性与可靠性 | ⚪ 未开始 | 0% | 重试, 熔断, 回退, 超时 |
| 阶段 4: 高级特性 | ⚪ 未开始 | 0% | 验证, 幂等性, 多模态增强 |

**整体完成度**: 约 20%

---

## ✅ 已完成功能

### 阶段 0: 代码质量与基础设施 (100%)

#### 1. 项目结构改进
- ✅ **`.gitignore`**: 排除 `target/`, `.env`, 非示例配置文件
- ✅ **`rustfmt.toml`**: 代码格式化配置
- ✅ **`src/core/mod.rs`**: 修复模块导出问题

#### 2. CI/CD 自动化
- ✅ **GitHub Actions CI** (`.github/workflows/ci.yml`):
  - 自动运行 `cargo fmt --check`
  - 自动运行 `cargo clippy` (警告视为错误)
  - 自动运行 `cargo test`
  - PostgreSQL 服务集成 (用于测试)
  - 代码覆盖率报告 (cargo-tarpaulin)
  - 安全审计 (rustsec)

#### 3. 代码质量修复
- ✅ **编译错误修复**:
  - 修复 `main.rs` 中的 `axum::Server` API 变更 (axum 0.7)
  - 修复 `registry.rs` 中的 async/await 逻辑
  - 修复 `routing.rs` 缺少 `Clone` derive
  - 修复 API 适配器中的 SSE 流类型推断
  - 修复 OpenRouter 连接器的 SSE 实现 (改用 eventsource-stream)

#### 4. 代码格式化
- ✅ **运行 `cargo fmt`**: 代码符合 Rust 风格指南
- ✅ **移除未使用导入**: 清理警告

---

## 🚧 进行中的功能

### 核心架构 (已稳定)

#### 统一数据模型 ✅
```rust
// src/core/entities.rs
pub struct UnifiedRequest {
    pub logical_model: String,
    pub messages: Vec<UnifiedMessage>,
    pub tools: Option<Vec<ToolSpec>>,        // 已定义但未使用
    pub max_output_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stream: bool,
    pub extra: serde_json::Value,
}

pub enum ContentPart {
    Text { text: String },
    ImageUrl { url: String, mime: Option<String> },
    ImageB64 { b64: String, mime: String },
    VideoUrl { url: String, mime: Option<String> },
}
```

#### 连接器 Trait ✅
```rust
// src/connectors/mod.rs
#[async_trait]
pub trait Connector: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ConnectorCapabilities;
    async fn invoke(&self, route: &EgressRoute, req: UnifiedRequest)
        -> Result<ConnectorResponse, ConnectorError>;
}
```

### 已实现的连接器

#### 1. OpenRouter Connector ✅ (生产可用)
**文件**: `src/connectors/openrouter.rs`

**功能**:
- ✅ 完整的 SSE 流式支持 (使用 eventsource-stream)
- ✅ 非流式响应
- ✅ 文本、图片 (URL + Base64)、视频支持
- ✅ Bearer token 认证
- ✅ 参数透传 (max_tokens, temperature, top_p, extra)
- ✅ 错误处理与标准化

**环境变量**:
- `OPENROUTER_API_KEY` (必需)
- `OPENROUTER_BASE_URL` (可选, 默认 https://openrouter.ai/api/v1)

**状态**: ✅ **完全实现, 可立即使用**

#### 2. Vertex AI Connector ⚠️ (部分实现)
**文件**: `src/connectors/vertex.rs`

**已实现**:
- ✅ 基础 `generateContent` 端点 (非流式)
- ✅ 文本、图片、视频内容映射
- ✅ OAuth / API Key 认证
- ✅ 角色转换 (assistant → model)

**缺失**:
- ❌ **流式支持** (`streamGenerateContent` 端点)
- ❌ 工具调用支持

**待办**:
```rust
// TODO: 实现流式支持
let url = format!(
    "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent",
    region, project, region, model_id
);
let response = client.post(&url).json(&body).send().await?;
let stream = response.bytes_stream().eventsource();
// 解析 Vertex SSE 事件...
```

#### 3. Clewdr Connector ⚠️ (简化实现)
**文件**: `src/connectors/clewdr.rs`

**已实现**:
- ✅ OpenAI 兼容的 `/v1/chat/completions` 端点
- ✅ 文本、图片内容支持
- ✅ Bearer token 认证

**缺失**:
- ❌ **流式支持**
- ❌ 工具调用支持

**环境变量**:
- `CLEWDR_API_KEY` (可选)
- `CLEWDR_BASE_URL` (可选, 默认 http://localhost:9000)

---

## 🔴 未实现的关键功能

### 阶段 1: 生产基础设施 (0%)

#### 1.1 PostgreSQL 鉴权系统 ❌
**优先级**: P0 (阻塞生产)

**当前状态**: 仅检查 "XJP" 前缀 (`src/auth.rs:10-20`)

**需要实现**:
```bash
# 数据库 Schema
CREATE TABLE api_keys (
    id UUID PRIMARY KEY,
    key_hash TEXT UNIQUE NOT NULL,
    tenant_id UUID NOT NULL,
    scopes TEXT[],
    rate_limit_rpm INTEGER DEFAULT 60,
    quota_tokens_daily BIGINT,
    enabled BOOLEAN DEFAULT true,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE usage_logs (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID,
    model TEXT,
    provider TEXT,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    latency_ms INTEGER,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);
```

**Rust 实现**:
```toml
# Cargo.toml
[dependencies]
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "uuid", "time"] }
```

```rust
// src/db/mod.rs
pub struct KeyStore {
    pool: sqlx::PgPool,
}

impl KeyStore {
    pub async fn validate(&self, key: &str) -> Result<ApiKeyInfo, AuthError> {
        let hash = sha256(key);
        sqlx::query_as!(
            ApiKeyInfo,
            "SELECT * FROM api_keys WHERE key_hash = $1 AND enabled = true",
            hash
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|_| AuthError::Invalid)
    }
}
```

**测试命令**:
```bash
# 生成测试密钥
cargo run --bin keygen -- --tenant test-user
```

---

#### 1.2 速率限制中间件 ❌
**优先级**: P0 (阻塞生产)

**依赖**: governor crate (已安装)

**实现**:
```rust
// src/middleware/rate_limit.rs
use governor::{Quota, RateLimiter, DefaultDirectRateLimiter};
use dashmap::DashMap;

pub struct RateLimitLayer {
    limiters: Arc<DashMap<String, DefaultDirectRateLimiter>>,
    default_quota: Quota,
}

impl RateLimitLayer {
    pub async fn check(&self, tenant_id: &str) -> Result<(), TooManyRequests> {
        let limiter = self.limiters
            .entry(tenant_id.to_string())
            .or_insert_with(|| RateLimiter::direct(self.default_quota));

        limiter.check().map_err(|_| TooManyRequests {
            retry_after: Duration::from_secs(60),
        })
    }
}
```

**配置** (`config/xjp.toml`):
```toml
[rate_limiting]
default_rpm = 60
burst_size = 10
```

---

#### 1.3 Prometheus 指标 ❌
**优先级**: P1 (高)

**依赖**:
```toml
[dependencies]
prometheus = "0.13"
lazy_static = "1.4"
```

**实现**:
```rust
// src/observability/metrics.rs
use prometheus::{IntCounterVec, HistogramVec, register_int_counter_vec, register_histogram_vec};

lazy_static! {
    pub static ref REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xjp_requests_total",
        "Total requests",
        &["model", "provider", "tenant", "status"]
    ).unwrap();

    pub static ref REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "xjp_request_duration_seconds",
        "Request latency",
        &["model", "provider"],
        vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
    ).unwrap();

    pub static ref TOKENS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xjp_tokens_total",
        "Total tokens",
        &["model", "tenant", "type"] // type: prompt/completion
    ).unwrap();
}
```

**端点**:
```rust
// src/main.rs
.route("/metrics", axum::routing::get(|| async {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}))
```

---

#### 1.4 OpenTelemetry 追踪 ❌
**优先级**: P2 (中)

**依赖**:
```toml
[dependencies]
opentelemetry = "0.21"
opentelemetry-otlp = "0.14"
tracing-opentelemetry = "0.22"
```

**实现**:
```rust
// src/observability/telemetry.rs
pub fn init_telemetry() -> Result<()> {
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(opentelemetry_otlp::new_exporter().tonic())
        .install_batch(opentelemetry::runtime::Tokio)?;

    tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(EnvFilter::from_default_env())
        .with(fmt::layer().json())
        .init();

    Ok(())
}
```

**环境变量**:
```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
OTEL_SERVICE_NAME=xjp-gateway
```

---

### 阶段 2: 连接器完善 (33%)

#### 2.1 Vertex 流式支持 ❌
**优先级**: P1

**参考实现** (OpenRouter 模式):
```rust
// src/connectors/vertex.rs
async fn invoke(&self, route: &EgressRoute, req: UnifiedRequest)
    -> Result<ConnectorResponse, ConnectorError>
{
    if req.stream {
        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:streamGenerateContent",
            region, project, region, model_id
        );

        let response = self.client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await?;

        let stream = response
            .bytes_stream()
            .eventsource()
            .map(|event| {
                // 解析 Vertex 的 candidates[0].content.parts[0].text
                let json = serde_json::from_str(&event.data)?;
                let text = json["candidates"][0]["content"]["parts"][0]["text"]
                    .as_str()
                    .map(String::from);
                Ok(UnifiedChunk {
                    text_delta: text,
                    done: json["candidates"][0]["finishReason"].is_string(),
                    ..Default::default()
                })
            });

        Ok(ConnectorResponse::Streaming(Box::pin(stream)))
    } else {
        // 现有非流式实现...
    }
}
```

---

#### 2.2 Clewdr 流式支持 ❌
**优先级**: P2

**前提**: 确认 Clewdr 后端支持 `stream: true`

**实现**: 如果 Clewdr 使用 OpenAI 兼容格式，可直接复用 OpenRouter 的 SSE 解析逻辑。

---

#### 2.3 工具调用 (Function Calling) ❌
**优先级**: P1

**当前状态**: 实体已定义 (`ToolSpec`, `tool_call_delta`) 但完全未实现

**实现步骤**:

**1. 请求端适配器**
```rust
// src/api/openai_adapter.rs
pub fn to_unified(req: OpenAiChatRequest) -> UnifiedRequest {
    let tools = req.tools.map(|t| {
        t.into_iter().map(|tool_json| {
            ToolSpec {
                name: tool_json["function"]["name"].as_str().unwrap().to_string(),
                description: tool_json["function"]["description"].as_str().map(String::from),
                json_schema: tool_json["function"]["parameters"].clone(),
            }
        }).collect()
    });
    // ...
    UnifiedRequest { tools, ..Default::default() }
}
```

**2. 连接器映射** (OpenRouter)
```rust
// src/connectors/openrouter.rs
if let Some(tools) = &req.tools {
    body["tools"] = json!(tools.iter().map(|t| {
        json!({
            "type": "function",
            "function": {
                "name": t.name,
                "description": t.description,
                "parameters": t.json_schema
            }
        })
    }).collect::<Vec<_>>());
}
```

**3. 响应解析**
```rust
// 流式
if let Some(tool_calls) = delta["tool_calls"].as_array() {
    chunk.tool_call_delta = Some(json!(tool_calls));
}

// 非流式
if let Some(tool_calls) = message["tool_calls"].as_array() {
    chunk.tool_call_delta = Some(json!(tool_calls));
}
```

**4. 响应端适配器**
```rust
// src/api/openai_adapter.rs
"choices": [{
    "message": {
        "role": "assistant",
        "content": null,
        "tool_calls": chunk.tool_call_delta
    }
}]
```

---

### 阶段 3: 弹性与可靠性 (0%)

#### 3.1 重试与退避 ❌
**优先级**: P1

```rust
// src/middleware/retry.rs
pub async fn retry_with_backoff<F, Fut, T, E>(
    mut func: F,
    policy: &RetryPolicy,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut attempt = 0;
    loop {
        match func().await {
            Ok(result) => return Ok(result),
            Err(e) if attempt < policy.max_retries => {
                let backoff = policy.base_backoff_ms * 2_u64.pow(attempt as u32);
                tokio::time::sleep(Duration::from_millis(backoff)).await;
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}
```

**配置**:
```toml
[models."claude-sonnet-4.5".primary.retry]
max_retries = 3
backoff_ms = 500
```

---

#### 3.2 熔断器 ❌
**优先级**: P1

```rust
// src/routing/circuit_breaker.rs
pub struct CircuitBreaker {
    state: Arc<RwLock<CircuitState>>,
    config: CircuitConfig,
}

pub struct CircuitConfig {
    pub failure_threshold: u32,      // 连续失败次数
    pub timeout_duration: Duration,  // 半开状态超时
    pub success_threshold: u32,      // 半开时需成功次数
}

enum CircuitState {
    Closed,
    Open { opened_at: Instant },
    HalfOpen { successes: u32 },
}
```

---

#### 3.3 回退路由 ❌
**优先级**: P2

**扩展配置**:
```toml
[models."claude-sonnet-4.5".primary]
provider = "OpenRouter"
provider_model_id = "anthropic/claude-3.5-sonnet"

[models."claude-sonnet-4.5".fallback]
provider = "Vertex"
provider_model_id = "publishers/google/models/gemini-1.5-pro-002"
```

**实现**:
```rust
// src/routing.rs
pub async fn invoke(&self, req: UnifiedRequest) -> Result<ConnectorResponse> {
    let routes = self.registry.resolve(&req.logical_model)?;

    match self.invoke_route(&routes.primary, req.clone()).await {
        Ok(resp) => Ok(resp),
        Err(e) if routes.fallback.is_some() => {
            tracing::warn!("Primary failed, trying fallback: {e}");
            self.invoke_route(routes.fallback.as_ref().unwrap(), req).await
        }
        Err(e) => Err(e),
    }
}
```

---

#### 3.4 超时配置 ❌
**优先级**: P2

**当前问题**: `timeouts_ms` 字段已定义但未使用

```rust
// src/connectors/openrouter.rs
let timeout = Duration::from_millis(route.timeouts_ms.unwrap_or(120_000));
let response = tokio::time::timeout(
    timeout,
    client.post(&url).json(&body).send()
).await??;
```

---

### 阶段 4: 高级特性 (0%)

#### 4.1 请求验证 ❌
**优先级**: P2

```rust
// src/middleware/validation.rs
pub struct RequestValidator {
    max_body_size: usize,      // 10MB
    max_messages: usize,        // 100
    max_tokens: u32,            // 8192
}

impl RequestValidator {
    pub fn validate_openai(&self, req: &OpenAiChatRequest) -> Result<(), ValidationError> {
        if req.messages.len() > self.max_messages {
            return Err(ValidationError::TooManyMessages);
        }
        if let Some(temp) = req.temperature {
            if temp < 0.0 || temp > 2.0 {
                return Err(ValidationError::InvalidTemperature);
            }
        }
        Ok(())
    }
}
```

---

#### 4.2 幂等性支持 ❌
**优先级**: P2

**依赖**: Redis

```toml
[dependencies]
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }
```

```rust
// src/middleware/idempotency.rs
pub struct IdempotencyLayer {
    redis: redis::aio::ConnectionManager,
    ttl: Duration,  // 24小时
}

impl IdempotencyLayer {
    pub async fn check(&self, key: &str) -> Option<CachedResponse> {
        let cached: Option<Vec<u8>> = self.redis.get(key).await.ok()?;
        bincode::deserialize(&cached).ok()
    }

    pub async fn store(&self, key: &str, resp: &Response) -> Result<()> {
        let serialized = bincode::serialize(resp)?;
        self.redis.set_ex(key, serialized, self.ttl.as_secs()).await?;
        Ok(())
    }
}
```

**Header**: `Idempotency-Key: <uuid>`

---

#### 4.3 多模态增强 (Anthropic) ❌
**优先级**: P2

**问题**: Anthropic 适配器不解析 `image_url`

```rust
// src/api/anthropic_adapter.rs
fn parse_content_parts(content: &serde_json::Value) -> Vec<ContentPart> {
    if let Some(arr) = content.as_array() {
        arr.iter().filter_map(|part| {
            match part["type"].as_str()? {
                "text" => Some(ContentPart::Text {
                    text: part["text"].as_str()?.to_string()
                }),
                "image" => {
                    if let Some(url) = part["source"]["url"].as_str() {
                        Some(ContentPart::ImageUrl { url: url.to_string(), mime: None })
                    } else if let Some(b64) = part["source"]["data"].as_str() {
                        Some(ContentPart::ImageB64 {
                            b64: b64.to_string(),
                            mime: part["source"]["media_type"].as_str()?.to_string(),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }).collect()
    } else {
        vec![ContentPart::Text { text: content.as_str()?.to_string() }]
    }
}
```

---

## 📝 测试计划

### 单元测试 (待实现)

创建 `tests/` 目录:
```bash
tests/
├── auth_tests.rs           # XJPkey 提取与验证
├── adapter_tests.rs        # OpenAI/Anthropic 转换
├── registry_tests.rs       # 模型路由解析
└── connector_tests.rs      # Mock 上游 API
```

**示例**:
```rust
// tests/auth_tests.rs
#[test]
fn test_extract_bearer_key() {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        "Bearer XJP_test_key".parse().unwrap()
    );
    let result = extract_xjpkey(&headers);
    assert_eq!(result.unwrap(), "XJP_test_key");
}
```

---

### 集成测试 (待实现)

**工具**: `wiremock` crate

```rust
// tests/integration/openai_endpoint.rs
#[tokio::test]
async fn test_openai_endpoint_with_mock_openrouter() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "Hello"}}]
        })))
        .mount(&mock_server)
        .await;

    std::env::set_var("OPENROUTER_BASE_URL", mock_server.uri());

    let response = app.request("/v1/chat/completions")
        .header("Authorization", "Bearer XJP_test")
        .json(&json!({"model": "test", "messages": [...]}))
        .send()
        .await;

    assert_eq!(response.status(), 200);
}
```

---

### 性能测试 (待实现)

**工具**: `wrk` 或 `k6`

```bash
# 本地性能测试
wrk -t12 -c400 -d30s --latency \
  -s scripts/load_test.lua \
  http://localhost:8080/v1/chat/completions
```

**目标**:
- P50 延迟 < 100ms (不含上游)
- P95 延迟 < 300ms
- P99 延迟 < 500ms
- 1K 并发稳定
- 10K RPS 吞吐量

---

## 🚀 快速开始 (开发环境)

### 1. 环境准备

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆仓库
cd xjp-router

# 启动 PostgreSQL (Docker)
docker run -d --name xjp-db \
  -e POSTGRES_PASSWORD=dev \
  -e POSTGRES_DB=xjp \
  -p 5432:5432 postgres:15

# 运行迁移 (待实施)
# sqlx migrate run
```

### 2. 配置环境变量

```bash
cat > .env <<EOF
# OpenRouter
OPENROUTER_API_KEY=sk-or-...

# Vertex AI
VERTEX_API_KEY=AIza...
VERTEX_PROJECT=my-gcp-project
VERTEX_REGION=us-central1

# Clewdr
CLEWDR_BASE_URL=http://localhost:9000
CLEWDR_API_KEY=optional

# Database (待实施)
# DATABASE_URL=postgres://postgres:dev@localhost/xjp

# Logging
RUST_LOG=info,xjp_gateway=debug
EOF
```

### 3. 配置模型路由

```bash
cp config/xjp.example.toml config/xjp.toml
```

编辑 `config/xjp.toml`:
```toml
[models."claude-sonnet-4.5".primary]
provider = "OpenRouter"
provider_model_id = "anthropic/claude-3.5-sonnet"

[models."gemini-pro".primary]
provider = "Vertex"
provider_model_id = "publishers/google/models/gemini-1.5-pro-002"
region = "us-central1"
project = "your-gcp-project"
```

### 4. 运行服务

```bash
cargo run
```

### 5. 测试

```bash
# 健康检查
curl http://localhost:8080/healthz

# OpenAI 兼容端点 (非流式)
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role": "user", "content": "Hello"}]
  }'

# OpenAI 兼容端点 (流式)
curl -N -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role": "user", "content": "Count to 10"}],
    "stream": true
  }'

# Anthropic 兼容端点
curl -X POST http://localhost:8080/v1/messages \
  -H "x-api-key: XJP_test" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role": "user", "content": "Explain SSE"}],
    "stream": true
  }'
```

---

## 📊 项目指标

### 代码统计

```
Language                     files          blank        comment           code
--------------------------------------------------------------------------------
Rust                            15            180             50           1260
Markdown                         3            120              0            850
TOML                             3             15             10             85
YAML                             1             10              5             95
--------------------------------------------------------------------------------
SUM:                            22            325             65           2290
```

### 依赖项

- **核心**: axum, tokio, reqwest, serde, serde_json
- **工具**: futures, async-stream, anyhow, thiserror
- **配置**: toml, uuid, base64, time
- **SSE**: eventsource-stream
- **日志**: tracing, tracing-subscriber
- **中间件**: tower, tower-http
- **已准备但未启用**: governor (限流)

### 技术栈

- **语言**: Rust 2021 Edition
- **Web 框架**: Axum 0.7
- **异步运行时**: Tokio 1.x
- **HTTP 客户端**: Reqwest 0.12 (Rustls)
- **配置**: TOML
- **日志**: tracing/tracing-subscriber
- **CI/CD**: GitHub Actions

---

## 🎯 下一步行动计划

### 短期 (1-2 周)

1. **PostgreSQL 鉴权** ⭐ 最高优先级
   - 创建数据库 schema
   - 实现 KeyStore trait
   - 集成到 AppState
   - 创建密钥生成工具

2. **速率限制** ⭐
   - 实现 RateLimitLayer
   - 集成 governor
   - 添加配置支持

3. **Prometheus 指标**
   - 定义核心指标
   - 添加 /metrics 端点
   - 在 routing::invoke() 中记录

### 中期 (2-4 周)

4. **工具调用 (Function Calling)**
   - 实现请求端适配
   - 实现连接器映射
   - 实现响应端适配
   - 端到端测试

5. **Vertex 流式支持**
   - 实现 streamGenerateContent
   - SSE 解析
   - 集成测试

6. **重试与熔断**
   - 实现重试逻辑
   - 实现熔断器
   - 添加回退路由

### 长期 (4-8 周)

7. **完整测试套件**
   - 单元测试 (>80% 覆盖)
   - 集成测试
   - 性能测试

8. **高级特性**
   - 请求验证
   - 幂等性支持
   - 多模态增强

9. **文档与工具**
   - API 文档
   - 运维手册
   - 性能调优指南

---

## 🐛 已知问题

1. **鉴权系统不安全**: 仅检查前缀，无数据库验证
2. **无速率限制**: 可被滥用
3. **无可观测性**: 生产环境无法监控
4. **工具调用缺失**: 影响功能完整性
5. **Vertex 无流式**: 限制使用场景
6. **无错误重试**: 首次失败即丢失请求
7. **无请求验证**: 可能导致上游错误

---

## 📚 参考资料

### API 文档
- [OpenAI API](https://platform.openai.com/docs/api-reference)
- [Anthropic API](https://docs.anthropic.com/en/api)
- [Vertex AI Gemini](https://cloud.google.com/vertex-ai/docs/generative-ai/model-reference/gemini)
- [OpenRouter Docs](https://openrouter.ai/docs)

### Rust 生态
- [Axum Guide](https://docs.rs/axum/latest/axum/)
- [SQLx Book](https://docs.rs/sqlx/latest/sqlx/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Prometheus Rust](https://docs.rs/prometheus/latest/prometheus/)

### 最佳实践
- [12-Factor App](https://12factor.net/)
- [OpenTelemetry Rust](https://opentelemetry.io/docs/instrumentation/rust/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/)

---

## 📞 联系方式

- **GitHub**: https://github.com/rickyjim626/xjp-router
- **问题反馈**: GitHub Issues
- **贡献指南**: CONTRIBUTING.md (待创建)

---

**最后更新**: 2025-10-21
**文档版本**: 1.0
**项目状态**: 🟡 开发中
