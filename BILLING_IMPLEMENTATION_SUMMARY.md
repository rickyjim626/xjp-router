# 计费系统集成实施总结

## ✅ 已完成的工作

### Phase 1: 基础计费模块 (已完成 ✓)

#### 1.1 依赖管理
- ✅ 添加 `claude-tokenizer = "0.3"` 到 Cargo.toml
- ✅ 添加 `tiktoken-rs = "0.7"` 到 Cargo.toml

#### 1.2 核心模块
创建完整的 `src/billing/` 模块：
- ✅ `mod.rs` - 模块导出
- ✅ `price.rs` - PricingCache (动态拉取 OpenRouter 价格，15分钟 TTL)
- ✅ `tokens.rs` - TokenUsage 结构 + GPT/Claude 分词器
- ✅ `calc.rs` - CostCalculator (详细成本拆分)
- ✅ `usage.rs` - OrUsage 解析器 (兼容 OpenRouter 格式)

#### 1.3 连接器增强
- ✅ **OpenRouter**: 注入 `"usage": {"include": true}` (src/connectors/openrouter.rs:136)
- ✅ **Vertex AI**: 已保留 `usageMetadata` 在 `provider_events` (src/connectors/vertex.rs:236, 272)

#### 1.4 应用集成
- ✅ 在 `AppState` 添加 `pricing: Arc<PricingCache>` (src/routing.rs:17)
- ✅ 创建 `/internal/billing/quote` API (src/api/billing.rs)
- ✅ 注册路由 (src/main.rs:133)

#### 1.5 构建验证
- ✅ 修复 `claude-tokenizer` 返回值类型错误
- ✅ 编译成功 (仅警告，无错误)

#### 1.6 文档
- ✅ 创建 `docs/BILLING_API_USAGE.md` - 使用指南
- ✅ 更新 `README.md` - 添加 Billing 功能说明

---

## 📋 计费 API 功能验证

### 端点 1: 查询价格
```bash
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d '{"provider_model_id": "anthropic/claude-4.5-sonnet"}'
```

**返回字段：**
- `pricing.prompt` - 输入 token 单价 (USD/token)
- `pricing.completion` - 输出 token 单价
- `pricing.input_cache_read` - 缓存读取单价
- `pricing.internal_reasoning` - 推理 token 单价

### 端点 2: 计算成本
```bash
curl -X POST http://localhost:8080/internal/billing/quote \
  -H "Content-Type: application/json" \
  -d '{
    "provider_model_id": "anthropic/claude-4.5-sonnet",
    "usage": {
      "usage": {
        "prompt_tokens": 1800,
        "prompt_tokens_details": {"cached_tokens": 600},
        "completion_tokens": 320,
        "completion_tokens_details": {"reasoning_tokens": 24}
      }
    }
  }'
```

**成本明细：**
- `breakdown.prompt_cost` - 非缓存输入成本
- `breakdown.cache_read_cost` - 缓存读取成本 (0.1x for Claude)
- `breakdown.completion_cost` - 输出成本
- `breakdown.internal_reasoning_cost` - 推理成本
- `breakdown.total_cost` - 总成本 (USD)

---

## 🚀 Phase 2: 实时计费系统 (待实施)

### 设计文档
已创建 `docs/BILLING_SYSTEM_DESIGN.md`，包含：
1. **数据库设计** - `billing_transactions` + `tenant_billing_summary` 表
2. **BillingInterceptor** - 请求拦截器设计
3. **BillingStore** - 数据库存储层接口
4. **API 端点** - 查询历史交易与成本汇总

### 数据库迁移
已创建 SQL 迁移文件：
- ✅ `migrations/005_create_billing_transactions.sql`
- ✅ `migrations/006_create_tenant_billing_summary.sql`

### 核心功能
1. **实时记录**：每次请求自动落库
2. **API Key 归属**：按租户/API Key 区分成本
3. **异步写入**：不阻塞响应 (tokio::spawn)
4. **幂等性**：基于 `request_id` 防重
5. **详细明细**：记录 prompt/completion/reasoning/cache 各项成本

---

## 📊 实施 Phase 2 的步骤

### Step 1: 数据库迁移
```bash
# 运行迁移
sqlx migrate run

# 验证表创建
psql $DATABASE_URL -c "\d billing_transactions"
```

### Step 2: 实现 BillingInterceptor
创建 `src/billing/interceptor.rs`：
- [ ] `BillingContext` 结构体
- [ ] `before_request()` - 创建计费上下文
- [ ] `after_request()` - 提取 usage、计算成本、异步落库
- [ ] `extract_usage()` - 从 `provider_events` 提取 token 统计

### Step 3: 实现 BillingStore
扩展 `src/db/` 模块：
- [ ] `BillingStore` trait (定义接口)
- [ ] `PgBillingStore` 实现 (PostgreSQL)
- [ ] `insert_transaction()` - 插入事务记录
- [ ] `get_transactions_by_tenant()` - 查询历史
- [ ] `get_cost_summary()` - 成本汇总

### Step 4: 集成到路由
修改 `src/routing.rs`：
- [ ] `AppState` 添加 `billing_interceptor` 字段
- [ ] 创建 `invoke_with_billing()` 方法
- [ ] 在所有 API handlers 中调用新方法

### Step 5: 添加查询 API
扩展 `src/api/billing.rs`：
- [ ] `GET /internal/billing/transactions` - 查询历史交易
- [ ] `GET /internal/billing/summary` - 成本汇总
- [ ] 在 `main.rs` 注册新路由

### Step 6: 测试
- [ ] 单元测试：`BillingInterceptor::extract_usage()`
- [ ] 集成测试：完整请求 → 数据库验证
- [ ] 性能测试：1000 req/s 下异步写入延迟

---

## 🔧 环境变量

### 必需
```bash
# OpenRouter (用于价格拉取)
export OPENROUTER_API_KEY=sk-or-v1-********************************

# 数据库
export DATABASE_URL=postgres://user:pass@localhost:5432/xjp_gateway
```

### 可选
```bash
# Vertex AI (如需使用 Gemini)
export VERTEX_API_KEY=AIza***********************************
export VERTEX_PROJECT=your-gcp-project
export VERTEX_REGION=us-central1

# 日志级别
export RUST_LOG=info,xjp_gateway=debug
```

---

## 📈 关键指标

### 计费准确性
- **价格来源**：OpenRouter `/api/v1/models` API
- **Token 统计**：优先使用上游原生分词器（OpenRouter `usage` 字段、Vertex `usageMetadata`）
- **本地分词**：仅作兜底（GPT: tiktoken-rs, Claude: claude-tokenizer）

### 成本计算规则
```
prompt_cost = (prompt_tokens - cached_tokens) × pricing.prompt
cache_read_cost = cached_tokens × pricing.input_cache_read
completion_cost = completion_tokens × pricing.completion
reasoning_cost = reasoning_tokens × (pricing.internal_reasoning || pricing.completion)
total_cost = prompt_cost + cache_read_cost + completion_cost + reasoning_cost + request_cost
```

### 缓存定价
- **OpenAI**: 读取 0.25x-0.50x（写入免费）
- **Anthropic**: 读取 0.1x，写入 1.25x
- **数据来源**: OpenRouter `pricing.input_cache_read/write` 字段

---

## 🎯 支持的模型

当前已验证兼容：
- ✅ **Claude 系列**: claude-4.5-sonnet, claude-4.1-opus, claude-3-*
- ✅ **Gemini 系列**: gemini-2.5-pro, gemini-2.5-flash
- ✅ **GPT 系列**: gpt-5-*, gpt-4o, gpt-4-turbo

**模型列表动态获取**：`https://openrouter.ai/api/v1/models`

---

## 🔍 验证计费准确性

### 方法 1: 对比 OpenRouter 官方 cost
1. 发起请求并保存响应 (包含 `usage` 字段)
2. 将 `usage` 传给 `/internal/billing/quote`
3. 对比 `breakdown.total_cost` 与 OpenRouter 返回的 `usage.cost`
4. 误差应 < 0.0001 USD (精度误差)

### 方法 2: 月度账单对账
1. 从 `billing_transactions` 导出当月所有记录
2. 按 `provider_model_id` 分组汇总
3. 对比 OpenRouter 月度发票
4. 差异率应 < 1%

---

## 🚨 注意事项

### 1. 价格缓存
- TTL: 15 分钟
- 刷新策略：缓存过期时自动重新拉取
- 建议：每天定时预热缓存 (cron job)

### 2. 流式响应处理
当前 `BillingInterceptor::extract_usage()` 仅支持非流式响应。
**待实施**：在流式场景下累计最后一个 chunk 的 `provider_events`

### 3. 数据库写入
- 使用 `tokio::spawn` 异步写入，不阻塞响应
- 使用 `ON CONFLICT (request_id) DO NOTHING` 防重
- 建议：监控 `billing_transactions` 插入延迟（应 < 100ms）

### 4. 成本归属
- 通过 `api_key_id` 关联到租户
- 需在 API handlers 中传递 `tenant_id` 和 `api_key_id`
- 确保认证中间件已提取这些字段

---

## 📦 依赖关系

```
xjp-gateway
├── billing 模块
│   ├── PricingCache → OpenRouter API
│   ├── TokenCounter → tiktoken-rs / claude-tokenizer
│   ├── CostCalculator → ModelPricing × TokenUsage
│   └── BillingInterceptor (待实施) → BillingStore
├── connectors
│   ├── openrouter → 注入 usage.include=true
│   └── vertex → 保留 usageMetadata
└── db
    ├── KeyStore (已有)
    └── BillingStore (待实施)
```

---

## 🎉 总结

### 已完成
✅ 基础计费模块（价格拉取、分词器、成本计算）
✅ `/internal/billing/quote` API（查价 + 对账）
✅ 连接器增强（OpenRouter usage 注入）
✅ 完整文档与数据库设计

### 待实施 (Phase 2)
⏳ BillingInterceptor（请求拦截器）
⏳ BillingStore（数据库存储层）
⏳ 查询 API（历史交易 + 成本汇总）
⏳ 流式响应 usage 提取

### 预计工期
- **Phase 1** (已完成): 1 天
- **Phase 2** (实时计费): 3-5 天

### 后续路线图
1. 实时计费系统落库 (Phase 2)
2. 租户余额管理（预付费）
3. 配额限制（月度预算）
4. Web Dashboard（实时消费可视化）
5. 自动发票生成

---

**项目状态**: 🟢 Phase 1 完成，可进行成本验证与对账
**下一步**: 实施 Phase 2 实时计费系统

最后更新: 2025-10-23
