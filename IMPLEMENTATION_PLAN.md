# XiaojinPro Gateway 实施计划

**生成时间**: 2025-10-21
**目标**: 从 MVP 状态升级到生产就绪的多供应商 AI 网关

---

## 📊 当前实施状态总结

### ✅ 已完成的核心功能 (约 40%)

#### 1. 架构基础 (100%)
- ✅ 统一数据模型 (UnifiedRequest/Message/Chunk)
- ✅ Connector trait 抽象层
- ✅ 模型注册表与路由机制
- ✅ TOML + 环境变量配置系统
- ✅ 异步 Axum 服务器框架

#### 2. 连接器实现 (33%)
- ✅ **OpenRouter**: 完全实现 (文本+图片+视频, 流式+非流式) - **生产可用**
- ⚠️ **Vertex AI**: 基础实现 (仅非流式, 缺少 streamGenerateContent)
- ⚠️ **Clewdr**: 简化实现 (仅非流式, 缺少 SSE 解析)

#### 3. API 适配器 (80%)
- ✅ OpenAI 兼容接口 (/v1/chat/completions) - 流式+非流式完整
- ✅ Anthropic 兼容接口 (/v1/messages) - 流式+非流式完整
- ❌ 但两者均未实现工具调用支持
- ⚠️ Anthropic 适配器缺少多模态内容解析

#### 4. 鉴权 (30%)
- ✅ XJPkey 提取 (Bearer/x-api-key 两种形式)
- ❌ 仅做前缀验证 (生产需要数据库验证)
- ❌ 无速率限制或配额管理

#### 5. 可观测性 (20%)
- ✅ 基础 tracing 日志
- ✅ /healthz 健康检查
- ❌ 无 Prometheus 指标
- ❌ 无 OpenTelemetry 分布式追踪

---

## 🎯 缺失的关键功能

### 🔴 阻塞生产部署的问题

1. **流式支持不完整** - Vertex 和 Clewdr 无法流式输出
2. **工具调用完全缺失** - 实体已定义但未实现
3. **鉴权系统仅为 stub** - 无法防止滥用
4. **无速率限制** - 无防护措施
5. **无可观测性** - 无法监控生产系统
6. **无重试/熔断** - 首次失败即丢失请求

### 🟡 重要但非紧急

7. 请求验证与大小限制
8. 超时配置未使用
9. CORS 支持
10. 幂等性支持
11. 审计日志

---

## 📅 四阶段实施路线图

### 阶段 0: 代码质量提升 (1-2 天)

**目标**: 修复技术债务，为后续开发打好基础

#### 任务清单
- [ ] 添加 .gitignore (排除 target/, .env, config/*.toml 非示例文件)
- [ ] 添加 CI/CD 配置 (GitHub Actions: cargo check, test, clippy, fmt)
- [ ] 添加单元测试框架
  - [ ] auth.rs 的 key 提取测试
  - [ ] openai_adapter/anthropic_adapter 的转换测试
  - [ ] registry.rs 的路由解析测试
- [ ] 添加集成测试目录 tests/
  - [ ] 端到端测试 (mock 上游 API)
- [ ] 文档完善
  - [ ] README.md 添加快速开始指南
  - [ ] API.md 文档化所有端点
  - [ ] 环境变量清单
- [ ] 代码规范
  - [ ] 运行 cargo clippy --fix
  - [ ] 运行 cargo fmt
  - [ ] 添加 rustfmt.toml 配置

**预期产出**:
- 可维护的代码库
- 基础测试覆盖 (>40%)
- 自动化代码质量检查

---

### 阶段 1: 生产基础设施 (3-5 天)

**目标**: 补充生产环境必需的基础设施组件

#### M1.1 真实鉴权系统 (1-2 天)

**当前状态**: 仅检查 "XJP" 前缀
**目标**: 数据库驱动的密钥管理

实施步骤:
```rust
// 1. 数据库 Schema (PostgreSQL)
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash TEXT UNIQUE NOT NULL,          -- SHA-256 hash
    tenant_id UUID NOT NULL,
    scopes TEXT[] DEFAULT '{}',             -- 可访问的模型列表
    rate_limit_rpm INTEGER DEFAULT 60,      -- 每分钟请求数
    quota_tokens_daily BIGINT,              -- 每日 token 配额
    enabled BOOLEAN DEFAULT true,
    expires_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE usage_logs (
    id BIGSERIAL PRIMARY KEY,
    tenant_id UUID NOT NULL,
    api_key_id UUID REFERENCES api_keys(id),
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    prompt_tokens INTEGER,
    completion_tokens INTEGER,
    stream_bytes BIGINT,
    latency_ms INTEGER,
    status_code SMALLINT,
    error_type TEXT,
    timestamp TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX idx_usage_tenant_time ON usage_logs(tenant_id, timestamp DESC);
```

代码改动:
- [ ] 添加 `sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "uuid", "time"] }`
- [ ] 创建 `src/db/mod.rs` - 数据库连接池
- [ ] 创建 `src/db/keys.rs` - KeyStore trait
  ```rust
  #[async_trait]
  pub trait KeyStore {
      async fn validate(&self, key: &str) -> Result<ApiKeyInfo, AuthError>;
      async fn record_usage(&self, usage: UsageRecord) -> Result<(), DbError>;
  }
  ```
- [ ] 实现 PostgresKeyStore
- [ ] 更新 auth.rs 调用 KeyStore::validate()
- [ ] 在 AppState 中注入 KeyStore
- [ ] 添加密钥生成 CLI 工具 (`cargo run --bin keygen`)

**测试要点**:
- 无效密钥返回 401
- 过期密钥返回 403
- 禁用密钥返回 403
- 正确记录使用量

---

#### M1.2 速率限制中间件 (1 天)

**依赖**: governor crate (已安装)

实施步骤:
- [ ] 创建 `src/middleware/rate_limit.rs`
  ```rust
  use governor::{Quota, RateLimiter};
  use axum::middleware::Next;

  pub struct RateLimitLayer {
      limiters: DashMap<String, RateLimiter<...>>,
  }

  impl RateLimitLayer {
      pub fn from_config(default_rpm: u32) -> Self;
      async fn check(&self, tenant_id: &str, quota: Quota) -> Result<(), TooManyRequests>;
  }
  ```
- [ ] 在 auth 中间件后提取 tenant_id
- [ ] 使用 DashMap 缓存每租户的 RateLimiter
- [ ] 超出限制时返回 429 (Retry-After header)
- [ ] 支持突发流量 (burst bucket)
- [ ] 添加配置: 全局默认限制 + 租户覆盖

**配置示例**:
```toml
[rate_limiting]
default_rpm = 60
burst_size = 10
```

**测试要点**:
- 连续请求触发 429
- Retry-After header 正确
- 不同租户独立计数

---

#### M1.3 Prometheus 指标 (1 天)

**依赖**: 添加 `prometheus = "0.13"`, `axum-prometheus = "0.6"`

实施步骤:
- [ ] 创建 `src/observability/metrics.rs`
  ```rust
  lazy_static! {
      pub static ref REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
          "xjp_requests_total",
          "Total requests by model, provider, tenant, status",
          &["model", "provider", "tenant", "status"]
      ).unwrap();

      pub static ref REQUEST_DURATION: HistogramVec = register_histogram_vec!(
          "xjp_request_duration_seconds",
          "Request latency in seconds",
          &["model", "provider", "endpoint"],
          vec![0.1, 0.5, 1.0, 2.0, 5.0, 10.0]
      ).unwrap();

      pub static ref TOKENS_TOTAL: IntCounterVec = register_int_counter_vec!(
          "xjp_tokens_total",
          "Total tokens consumed",
          &["model", "tenant", "token_type"] // token_type: prompt/completion
      ).unwrap();

      pub static ref STREAMING_BYTES: IntCounterVec = register_int_counter_vec!(
          "xjp_streaming_bytes_total",
          "Total streaming bytes",
          &["model", "provider"]
      ).unwrap();
  }
  ```
- [ ] 在 main.rs 添加 `GET /metrics` 端点
- [ ] 在 routing.rs 的 invoke() 中记录指标
- [ ] 在 connectors 中记录上游错误率

**测试要点**:
- /metrics 返回 Prometheus 格式
- 计数器正确递增
- 直方图桶分布合理

---

#### M1.4 结构化日志 + 追踪 (1 天)

**当前状态**: 基础 tracing
**目标**: 端到端请求追踪 + 脱敏

实施步骤:
- [ ] 添加 `opentelemetry = "0.21"`, `opentelemetry-otlp = "0.14"`, `tracing-opentelemetry = "0.22"`
- [ ] 创建 `src/observability/telemetry.rs`
  ```rust
  pub fn init_telemetry() -> Result<()> {
      let tracer = opentelemetry_otlp::new_pipeline()
          .tracing()
          .with_exporter(opentelemetry_otlp::new_exporter().tonic())
          .install_batch(opentelemetry::runtime::Tokio)?;

      let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);

      tracing_subscriber::registry()
          .with(telemetry)
          .with(EnvFilter::from_default_env())
          .with(fmt::layer().json())
          .init();

      Ok(())
  }
  ```
- [ ] 为每个请求生成 request_id (UUID)
- [ ] 使用 `#[instrument]` 宏标注关键函数
- [ ] 脱敏敏感信息 (API key 只记录前 8 位)
- [ ] 记录结构化字段: tenant_id, model, provider, latency_ms
- [ ] 可选采样 (生产中仅采样 1% Prompt 内容)

**环境变量**:
```bash
OTEL_EXPORTER_OTLP_ENDPOINT=http://jaeger:4317
OTEL_SERVICE_NAME=xjp-gateway
```

---

### 阶段 2: 连接器完善 (4-6 天)

**目标**: 所有连接器支持流式 + 工具调用

#### M2.1 Vertex 流式支持 (2 天)

**当前缺陷**: 仅实现 `generateContent` (非流式)

实施步骤:
- [ ] 研究 Vertex AI `streamGenerateContent` API
  ```
  POST https://{region}-aiplatform.googleapis.com/v1/projects/{project}/locations/{region}/publishers/google/models/{model}:streamGenerateContent
  ```
- [ ] 实现 SSE 解析 (Vertex 返回的是 `data: {...}` 格式)
- [ ] 处理多个 candidates
- [ ] 处理 finishReason 映射 (STOP → done: true)
- [ ] 更新 capabilities() 返回 stream: true
- [ ] 修改 invoke() 方法返回 ConnectorResponse::Streaming
  ```rust
  let stream = reqwest_client
      .post(&stream_url)
      .bearer_auth(&access_token)
      .json(&body)
      .send()
      .await?
      .bytes_stream();

  let sse_stream = eventsource_stream::EventStream::new(stream);

  let unified_stream = sse_stream.filter_map(|event| {
      // 解析 Vertex 的 SSE 事件
      // 提取 candidates[0].content.parts[0].text
      // 转换为 UnifiedChunk
  });

  Ok(ConnectorResponse::Streaming(Box::pin(unified_stream)))
  ```

**测试**:
- [ ] 单元测试 SSE 解析器
- [ ] 集成测试流式响应 (需 Vertex 测试账号)
- [ ] 测试长响应 (>10K tokens)
- [ ] 测试网络中断重连

---

#### M2.2 Clewdr 流式支持 (1 天)

**前提**: 确认 Clewdr 后端支持 SSE

实施步骤:
- [ ] 检查 Clewdr 是否支持 `stream: true`
- [ ] 如果支持 OpenAI 兼容格式:
  ```rust
  // 复用 OpenRouter 的 SSE 解析逻辑
  let event_stream = EventSource::new(response)?;
  // 与 OpenRouter 相同的解析
  ```
- [ ] 如果不支持: 与 Clewdr 团队协商添加 SSE 端点
- [ ] 更新 capabilities() 返回 stream: true

---

#### M2.3 工具调用 (函数调用) 实现 (2-3 天)

**当前状态**: 实体定义但未实现

实施步骤:

**1. 请求端**:
- [ ] OpenAI 适配器提取 tools[] 字段
  ```rust
  // openai_adapter.rs
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
  }
  ```
- [ ] Anthropic 适配器提取 tools[] (格式略有不同)
- [ ] 传递 tools 到 UnifiedRequest

**2. 连接器映射**:
- [ ] OpenRouter: 直接透传 tools (OpenAI 格式)
  ```rust
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
- [ ] Vertex: 映射到 functionDeclarations
  ```rust
  "tools": [{
      "functionDeclarations": [
          {
              "name": "get_weather",
              "description": "...",
              "parameters": { /* JSON Schema */ }
          }
      ]
  }]
  ```
- [ ] Clewdr: 取决于其协议 (可能与 OpenAI 相同)

**3. 响应端**:
- [ ] 解析 tool_calls (OpenRouter/OpenAI)
  ```rust
  // 非流式
  if let Some(tool_calls) = response["choices"][0]["message"]["tool_calls"].as_array() {
      chunk.tool_call_delta = Some(json!(tool_calls));
  }

  // 流式
  if let Some(delta_tools) = delta["tool_calls"].as_array() {
      chunk.tool_call_delta = Some(json!(delta_tools));
  }
  ```
- [ ] 解析 functionCall (Vertex)
  ```rust
  if let Some(fc) = candidate["content"]["parts"][0]["functionCall"].as_object() {
      chunk.tool_call_delta = Some(json!({
          "name": fc["name"],
          "arguments": fc["args"]
      }));
  }
  ```
- [ ] OpenAI 适配器回传 tool_calls
  ```rust
  "choices": [{
      "message": {
          "role": "assistant",
          "content": null,
          "tool_calls": chunk.tool_call_delta
      }
  }]
  ```
- [ ] Anthropic 适配器回传 tool_use content blocks
  ```rust
  "content": [
      {
          "type": "tool_use",
          "id": "toolu_...",
          "name": "get_weather",
          "input": { "location": "Beijing" }
      }
  ]
  ```

**4. 工具结果提交**:
- [ ] 支持 role: "tool" 的消息
- [ ] OpenAI 格式: `{ role: "tool", tool_call_id: "...", content: "..." }`
- [ ] Anthropic 格式: `{ role: "user", content: [{ type: "tool_result", tool_use_id: "...", content: "..." }] }`

**测试**:
- [ ] 单轮工具调用 (get_weather)
- [ ] 多轮对话 (调用 → 结果 → 继续)
- [ ] 并行工具调用 (一次返回多个 tool_calls)
- [ ] 流式工具调用 (逐步构建 arguments)
- [ ] 拒绝调用工具 (finish_reason: "stop" 而非 "tool_calls")

---

### 阶段 3: 弹性与可靠性 (3-4 天)

**目标**: 生产级错误处理与容错

#### M3.1 重试与退避 (1 天)

**策略**: 仅对幂等请求重试 (可选传 Idempotency-Key)

实施步骤:
- [ ] 添加 `tower = { version = "0.5", features = ["retry", "timeout"] }`
- [ ] 创建 `src/middleware/retry.rs`
  ```rust
  pub struct RetryPolicy {
      pub max_retries: u8,
      pub base_backoff_ms: u64,
      pub retryable_status: Vec<StatusCode>,
  }

  impl Default for RetryPolicy {
      fn default() -> Self {
          Self {
              max_retries: 3,
              base_backoff_ms: 500,
              retryable_status: vec![
                  StatusCode::TOO_MANY_REQUESTS,      // 429
                  StatusCode::INTERNAL_SERVER_ERROR,  // 500
                  StatusCode::BAD_GATEWAY,            // 502
                  StatusCode::SERVICE_UNAVAILABLE,    // 503
                  StatusCode::GATEWAY_TIMEOUT,        // 504
              ],
          }
      }
  }

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
- [ ] 在 connectors 中集成:
  ```rust
  let response = retry_with_backoff(
      || client.post(&url).json(&body).send(),
      &route.retry.unwrap_or_default(),
  ).await?;
  ```
- [ ] 记录重试次数到日志
- [ ] 在配置中支持 retry.max_retries 和 retry.backoff_ms

**测试**:
- [ ] 429 触发重试
- [ ] 503 触发重试
- [ ] 400 不重试
- [ ] 达到 max_retries 后失败

---

#### M3.2 熔断器 (1 天)

**策略**: 出口级熔断 (不影响其他出口)

实施步骤:
- [ ] 添加 failsafe crate 或手动实现
- [ ] 创建 `src/routing/circuit_breaker.rs`
  ```rust
  pub struct CircuitBreaker {
      state: Arc<RwLock<CircuitState>>,
      config: CircuitConfig,
  }

  pub struct CircuitConfig {
      pub failure_threshold: u32,      // 连续失败次数阈值
      pub timeout_duration: Duration,  // 半开状态超时
      pub success_threshold: u32,      // 半开时需要的成功次数
  }

  enum CircuitState {
      Closed,
      Open { opened_at: Instant },
      HalfOpen { successes: u32 },
  }

  impl CircuitBreaker {
      pub async fn call<F, T>(&self, func: F) -> Result<T, CircuitError>
      where F: Future<Output = Result<T, ConnectorError>> {
          // 检查状态
          // 如果 Open 且未超时 → 返回 CircuitOpen 错误
          // 如果 Open 且超时 → 转到 HalfOpen
          // 如果 HalfOpen → 调用，成功则 success++，失败则 Open
          // 如果 Closed → 调用，失败则 failure++
      }
  }
  ```
- [ ] 在 AppState 中为每个出口创建独立的熔断器
- [ ] 熔断时返回 503 Service Unavailable
- [ ] 记录熔断事件到日志
- [ ] 暴露熔断状态到 /healthz 或 /status

**测试**:
- [ ] 连续失败触发熔断
- [ ] 半开状态恢复
- [ ] 半开时失败重新打开

---

#### M3.3 回退路由 (1 天)

**策略**: 主出口失败时自动切到备份出口

实施步骤:
- [ ] 扩展 ModelRegistry 支持多出口
  ```toml
  [models."claude-sonnet-4.5".primary]
  provider = "OpenRouter"
  provider_model_id = "anthropic/claude-3.5-sonnet"

  [models."claude-sonnet-4.5".fallback]
  provider = "Vertex"
  provider_model_id = "publishers/google/models/gemini-1.5-pro-002"
  ```
- [ ] 修改 registry.rs
  ```rust
  pub struct ModelRoutes {
      pub primary: EgressRoute,
      pub fallback: Option<EgressRoute>,
  }

  impl ModelRegistry {
      pub fn resolve(&self, model: &str) -> Result<&ModelRoutes>;
  }
  ```
- [ ] 修改 routing.rs
  ```rust
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

**测试**:
- [ ] 主出口 500 → 回退成功
- [ ] 主出口 429 → 回退成功
- [ ] 主备均失败 → 返回主出口错误

---

#### M3.4 超时配置 (0.5 天)

**当前问题**: timeouts_ms 字段未使用

实施步骤:
- [ ] 在 connectors 中读取 route.timeouts_ms
  ```rust
  let timeout = Duration::from_millis(route.timeouts_ms.unwrap_or(120_000));

  let response = tokio::time::timeout(
      timeout,
      client.post(&url).json(&body).send()
  ).await??;
  ```
- [ ] 超时返回 ConnectorError::Timeout
- [ ] 映射为 504 Gateway Timeout

---

### 阶段 4: 高级特性 (4-5 天)

#### M4.1 请求验证 (1 天)

实施步骤:
- [ ] 添加 validator crate
- [ ] 创建 `src/middleware/validation.rs`
  ```rust
  pub struct RequestValidator {
      max_body_size: usize,      // 10MB
      max_messages: usize,        // 100
      max_tokens: u32,            // 8192
      allowed_content_types: Vec<String>,
  }

  impl RequestValidator {
      pub fn validate_openai(&self, req: &OpenAiChatRequest) -> Result<(), ValidationError> {
          if req.messages.len() > self.max_messages {
              return Err(ValidationError::TooManyMessages);
          }
          if let Some(max) = req.max_tokens {
              if max > self.max_tokens {
                  return Err(ValidationError::InvalidMaxTokens);
              }
          }
          // 验证 temperature 范围 [0.0, 2.0]
          // 验证 top_p 范围 [0.0, 1.0]
          Ok(())
      }
  }
  ```
- [ ] 在 Axum router 添加 body size 限制
  ```rust
  .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB
  ```
- [ ] 在 handler 中验证后再调用 invoke()

---

#### M4.2 幂等性支持 (1 天)

实施步骤:
- [ ] 添加 `redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }`
- [ ] 创建 `src/middleware/idempotency.rs`
  ```rust
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
- [ ] 提取 Idempotency-Key header
- [ ] 对非流式响应缓存
- [ ] 流式请求不支持幂等 (返回 409 Conflict)

---

#### M4.3 多模态增强 (1 天)

**问题**: Anthropic 适配器不解析 image_url

实施步骤:
- [ ] 修改 anthropic_adapter.rs
  ```rust
  fn parse_content_parts(content: &serde_json::Value) -> Vec<ContentPart> {
      if let Some(arr) = content.as_array() {
          arr.iter().filter_map(|part| {
              match part["type"].as_str()? {
                  "text" => Some(ContentPart::Text { text: part["text"].as_str()?.to_string() }),
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
      } else if let Some(text) = content.as_str() {
          vec![ContentPart::Text { text: text.to_string() }]
      } else {
          vec![]
      }
  }
  ```

---

#### M4.4 对象存储集成 (可选, 2 天)

**用例**: 代理下载外部图片/视频到 S3/GCS，生成临时 URL

实施步骤:
- [ ] 添加 `aws-sdk-s3 = "1.0"` 或 `object_store = "0.9"`
- [ ] 创建 `src/storage/media.rs`
  ```rust
  pub struct MediaProxy {
      s3_client: aws_sdk_s3::Client,
      bucket: String,
      ttl: Duration,
  }

  impl MediaProxy {
      pub async fn upload_from_url(&self, url: &str) -> Result<String> {
          // 下载 URL 内容
          let bytes = reqwest::get(url).await?.bytes().await?;

          // 上传到 S3
          let key = format!("media/{}/{}", Uuid::new_v4(), hash);
          self.s3_client.put_object()
              .bucket(&self.bucket)
              .key(&key)
              .body(bytes.into())
              .send().await?;

          // 生成预签名 URL
          let presigned = self.s3_client.presign_get_object()
              .bucket(&self.bucket)
              .key(&key)
              .expires_in(self.ttl)
              .await?;

          Ok(presigned.uri().to_string())
      }
  }
  ```
- [ ] 在 connectors 中检测外部 URL
- [ ] 自动转换为 S3 URL 后再发送给上游
- [ ] 设置 TTL (24小时后自动删除)

---

### 阶段 5: 管理与可视化 (5-7 天, 可选)

#### M5.1 管理 API (3 天)

实施步骤:
- [ ] 创建 Admin API 端点
  ```
  POST /admin/keys             - 创建新密钥
  GET  /admin/keys/:id         - 查询密钥信息
  PATCH /admin/keys/:id        - 更新密钥 (配额/限制/禁用)
  DELETE /admin/keys/:id       - 删除密钥

  GET  /admin/usage?tenant_id=&start=&end= - 查询用量
  GET  /admin/models           - 列出所有模型路由
  POST /admin/models           - 添加新模型路由
  PATCH /admin/models/:name    - 更新路由 (主/备出口)
  DELETE /admin/models/:name   - 删除路由
  ```
- [ ] 管理端点需要额外的 Admin token 鉴权
- [ ] 支持 JSON 格式导入/导出配置

---

#### M5.2 Web 控制台 (4 天)

使用 React + Vite 构建:
- [ ] 密钥管理页面 (CRUD)
- [ ] 模型路由配置页面
- [ ] 用量仪表板 (图表: tokens/day, requests/model)
- [ ] 实时监控 (WebSocket 推送指标)
- [ ] 审计日志查看器

---

## 🧪 测试策略

### 单元测试 (每个模块 >80% 覆盖)
```bash
cargo test --lib
```

### 集成测试
```bash
cargo test --test '*'
```

测试用例:
- [ ] OpenAI 兼容端到端 (mock OpenRouter)
- [ ] Anthropic 兼容端到端 (mock Vertex)
- [ ] 流式响应完整性 (10K tokens)
- [ ] 工具调用多轮对话
- [ ] 速率限制触发
- [ ] 熔断器状态转换
- [ ] 回退路由切换

### 性能测试
```bash
# 使用 wrk 或 k6
wrk -t12 -c400 -d30s --latency http://localhost:8080/v1/chat/completions
```

目标:
- [ ] P50 延迟 < 100ms (不含上游)
- [ ] P95 延迟 < 300ms
- [ ] P99 延迟 < 500ms
- [ ] 1K 并发请求稳定
- [ ] 10K RPS 吞吐量

### 安全测试
- [ ] SQL 注入 (参数化查询)
- [ ] Header 注入
- [ ] 大负载 DOS (body size 限制)
- [ ] 密钥泄漏 (日志脱敏)

---

## 📦 部署架构

### Docker 多阶段构建

```dockerfile
# Build stage
FROM rust:1.75 AS builder
WORKDIR /app
COPY Cargo.* ./
COPY src ./src
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/xjp-gateway /usr/local/bin/
EXPOSE 8080
CMD ["xjp-gateway"]
```

### Kubernetes 部署

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: xjp-gateway
spec:
  replicas: 3
  template:
    spec:
      containers:
      - name: gateway
        image: xjp-gateway:latest
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: xjp-secrets
              key: database-url
        - name: OPENROUTER_API_KEY
          valueFrom:
            secretKeyRef:
              name: xjp-secrets
              key: openrouter-key
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 2000m
            memory: 2Gi
        livenessProbe:
          httpGet:
            path: /healthz
            port: 8080
          periodSeconds: 10
---
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: xjp-gateway
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: xjp-gateway
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Pods
    pods:
      metric:
        name: http_requests_per_second
      target:
        type: AverageValue
        averageValue: "1000"
```

---

## 📊 进度追踪

| 阶段 | 任务数 | 预估时间 | 依赖 | 优先级 |
|------|--------|----------|------|--------|
| 阶段 0: 代码质量 | 6 | 1-2 天 | 无 | P0 |
| 阶段 1: 基础设施 | 4 | 3-5 天 | 阶段 0 | P0 |
| 阶段 2: 连接器 | 3 | 4-6 天 | 阶段 1 | P1 |
| 阶段 3: 弹性 | 4 | 3-4 天 | 阶段 2 | P1 |
| 阶段 4: 高级特性 | 4 | 4-5 天 | 阶段 3 | P2 |
| 阶段 5: 管理 | 2 | 5-7 天 | 阶段 4 | P3 |

**总计**: 15-29 天 (取决于团队规模与并行能力)

---

## ✅ 完成标准

### 阶段 1 完成标准 (MVP → Beta)
- [x] 所有单元测试通过 (>60% 覆盖)
- [ ] 真实密钥验证 (PostgreSQL)
- [ ] 速率限制工作 (429 正确返回)
- [ ] Prometheus 指标可查询
- [ ] 结构化日志输出 JSON

### 阶段 2 完成标准 (Beta → RC)
- [ ] Vertex 流式响应成功
- [ ] Clewdr 流式响应成功 (如支持)
- [ ] 工具调用端到端测试通过
- [ ] 所有连接器支持 text/vision/video

### 阶段 3 完成标准 (RC → Production)
- [ ] 重试逻辑正确处理 429/503
- [ ] 熔断器在连续失败时打开
- [ ] 回退路由自动切换
- [ ] 性能测试达标 (P95 < 300ms)

### 阶段 4 完成标准 (Production → Enterprise)
- [ ] 请求验证拒绝非法输入
- [ ] 幂等性支持 (Redis 缓存)
- [ ] 多模态内容全支持
- [ ] 安全测试无高危漏洞

---

## 🚀 快速开始 (开发者)

```bash
# 1. 克隆并安装依赖
git clone <repo>
cd xjp-router
cargo build

# 2. 启动 PostgreSQL (Docker)
docker run -d --name xjp-db \
  -e POSTGRES_PASSWORD=dev \
  -e POSTGRES_DB=xjp \
  -p 5432:5432 postgres:15

# 3. 运行迁移
sqlx migrate run

# 4. 配置环境变量
cat > .env <<EOF
DATABASE_URL=postgres://postgres:dev@localhost/xjp
OPENROUTER_API_KEY=sk-or-...
VERTEX_API_KEY=AIza...
VERTEX_PROJECT=my-gcp-project
VERTEX_REGION=us-central1
RUST_LOG=debug
EOF

# 5. 运行服务
cargo run

# 6. 测试
curl http://localhost:8080/healthz
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test" \
  -H "Content-Type: application/json" \
  -d '{"model":"claude-sonnet-4.5","messages":[{"role":"user","content":"Hello"}]}'
```

---

## 📚 参考资料

### API 文档
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)
- [Anthropic API Reference](https://docs.anthropic.com/en/api)
- [Vertex AI Gemini API](https://cloud.google.com/vertex-ai/docs/generative-ai/model-reference/gemini)
- [OpenRouter API Docs](https://openrouter.ai/docs)

### Rust 生态
- [Axum Book](https://docs.rs/axum/latest/axum/)
- [SQLx Documentation](https://docs.rs/sqlx/latest/sqlx/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Prometheus Rust Client](https://docs.rs/prometheus/latest/prometheus/)

### 最佳实践
- [12-Factor App](https://12factor.net/)
- [OpenTelemetry Rust](https://opentelemetry.io/docs/instrumentation/rust/)
- [Kubernetes Best Practices](https://kubernetes.io/docs/concepts/configuration/overview/)

---

**最后更新**: 2025-10-21
**维护者**: XiaojinPro Team
**状态**: 🟡 执行中 (当前阶段: 0)
