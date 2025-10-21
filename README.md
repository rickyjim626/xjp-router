# XiaojinPro Gateway (Rust)

**一个生产级的多供应商 AI 网关，支持 OpenAI 和 Anthropic 兼容 API**

[![Build Status](https://github.com/rickyjim626/xjp-router/workflows/CI/badge.svg)](https://github.com/rickyjim626/xjp-router/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)

## 🎯 特性

- ✅ **统一 API**: 同时支持 OpenAI 和 Anthropic 兼容接口
- ✅ **多供应商**: OpenRouter, Vertex AI (Gemini), 自建 Clewdr
- ✅ **流式支持**: 完整的 SSE (Server-Sent Events) 实现
- ✅ **多模态**: 文本、图片 (URL/Base64)、视频输入
- ⚠️ **工具调用**: 已规划 (待实现)
- ⚠️ **速率限制**: 已规划 (待实现)
- ⚠️ **可观测性**: 已规划 (Prometheus + OpenTelemetry)

## 📊 项目状态

**当前版本**: 0.1.0 (Alpha)
**完成度**: ~60%

查看详细进度: [DEVELOPMENT_STATUS.md](./DEVELOPMENT_STATUS.md) | [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md)

### 连接器状态

| 供应商 | 文本 | 图片 | 视频 | 流式 | 工具调用 | 状态 |
|--------|------|------|------|------|----------|------|
| **OpenRouter** | ✅ | ✅ | ✅ | ✅ | ✅ | 生产可用 |
| **Vertex AI** | ✅ | ✅ | ✅ | ✅ | ⚠️ | 生产可用 |
| **Clewdr** | ✅ | ✅ | ⚠️ | ✅ | ❌ | 生产可用 |

## 🚀 快速开始

### 前置要求

- Rust 1.75+ ([安装](https://rustup.rs/))
- PostgreSQL 15+ (可选, 用于生产鉴权)
- 至少一个 AI 提供商的 API 密钥

### 安装

```bash
# 克隆仓库
git clone https://github.com/rickyjim626/xjp-router.git
cd xjp-router

# 编译
cargo build --release

# 运行
cargo run
```

### 配置

1. **创建配置文件**:
```bash
cp config/xjp.example.toml config/xjp.toml
```

2. **编辑模型路由** (`config/xjp.toml`):
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

3. **设置环境变量**:
```bash
# OpenRouter
export OPENROUTER_API_KEY=sk-or-...

# Vertex AI
export VERTEX_API_KEY=AIza...
export VERTEX_PROJECT=your-gcp-project
export VERTEX_REGION=us-central1

# Clewdr (自建)
export CLEWDR_BASE_URL=http://localhost:9000
export CLEWDR_API_KEY=optional

# 日志级别
export RUST_LOG=info,xjp_gateway=debug
```

### 使用示例

#### OpenAI 兼容 API

```bash
# 非流式
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [
      {"role": "user", "content": "Hello, how are you?"}
    ]
  }'

# 流式
curl -N -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [
      {"role": "user", "content": "Count to 10 slowly"}
    ],
    "stream": true
  }'
```

#### Anthropic 兼容 API

```bash
curl -X POST http://localhost:8080/v1/messages \
  -H "x-api-key: XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [
      {"role": "user", "content": "Explain SSE in simple terms"}
    ],
    "max_tokens": 1024,
    "stream": true
  }'
```

#### 多模态 (图片)

```bash
curl -X POST http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{
      "role": "user",
      "content": [
        {"type": "text", "text": "What is in this image?"},
        {
          "type": "image_url",
          "image_url": {
            "url": "https://example.com/image.jpg"
          }
        }
      ]
    }]
  }'
```

## 🏗️ 架构

```
┌─────────────────── XiaojinPro Gateway ───────────────────┐
│                                                           │
│  Ingress Layer                                            │
│  ├─ POST /v1/chat/completions (OpenAI)                    │
│  ├─ POST /v1/messages (Anthropic)                         │
│  └─ GET  /healthz                                         │
│                                                           │
│  Middleware Layer                                         │
│  ├─ XJPkey 鉴权 (Bearer/x-api-key)                        │
│  ├─ 速率限制 (待实现)                                      │
│  └─ 请求追踪 (tracing)                                     │
│                                                           │
│  Adapter Layer                                            │
│  ├─ OpenAI → UnifiedRequest                               │
│  └─ Anthropic → UnifiedRequest                            │
│                                                           │
│  Routing Layer (Molecular)                                │
│  ├─ ModelRegistry (logical_model → EgressRoute)           │
│  └─ 路由策略: 主/备, AB实验, 回退 (待实现)                   │
│                                                           │
│  Connector Layer (Atomic)                                 │
│  ├─ OpenRouterConnector ✅                                 │
│  ├─ VertexConnector ⚠️                                     │
│  └─ ClewdrConnector ⚠️                                     │
│                                                           │
│  Observability (待实现)                                    │
│  ├─ Prometheus 指标                                        │
│  ├─ OpenTelemetry 追踪                                     │
│  └─ 结构化日志                                             │
└───────────────────────────────────────────────────────────┘
```

## 📦 部署

### Docker

```bash
# 构建镜像
docker build -t xjp-gateway:latest .

# 运行
docker run -d \
  -p 8080:8080 \
  -e OPENROUTER_API_KEY=sk-or-... \
  -e RUST_LOG=info \
  --name xjp-gateway \
  xjp-gateway:latest
```

### Kubernetes

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
        ports:
        - containerPort: 8080
        env:
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
```

## 🧪 测试

```bash
# 运行所有测试
cargo test

# 运行特定测试
cargo test test_openai_adapter

# 代码覆盖率 (需要 cargo-tarpaulin)
cargo tarpaulin --verbose --all-features --workspace

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# 格式化
cargo fmt
```

## 📖 文档

- [实施计划](./IMPLEMENTATION_PLAN.md) - 完整的开发路线图 (15-29天)
- [开发状态](./DEVELOPMENT_STATUS.md) - 当前进度与待办事项
- [API 文档](./docs/API.md) - 待创建
- [贡献指南](./CONTRIBUTING.md) - 待创建

## 🛠️ 技术栈

- **语言**: Rust 2021 Edition
- **Web 框架**: Axum 0.7
- **异步运行时**: Tokio 1.x
- **HTTP 客户端**: Reqwest 0.12 (Rustls)
- **配置**: TOML
- **日志**: tracing/tracing-subscriber
- **CI/CD**: GitHub Actions

### 依赖项

查看 [Cargo.toml](./Cargo.toml) 获取完整列表。主要依赖:
- `axum` - Web 框架
- `tokio` - 异步运行时
- `reqwest` - HTTP 客户端
- `serde` / `serde_json` - 序列化
- `eventsource-stream` - SSE 解析
- `tracing` - 结构化日志

## 🚧 待办事项

### 短期 (P0 - 阻塞生产)
- [ ] PostgreSQL 鉴权系统
- [ ] 速率限制中间件
- [ ] Prometheus 指标
- [ ] 工具调用 (Function Calling)

### 中期 (P1 - 高优先级)
- [ ] Vertex AI 流式支持
- [ ] 重试与熔断机制
- [ ] 完整的单元测试 (>80% 覆盖)
- [ ] 集成测试

### 长期 (P2 - 可选)
- [ ] OpenTelemetry 分布式追踪
- [ ] 请求验证
- [ ] 幂等性支持 (Redis)
- [ ] 多模态增强 (Anthropic)
- [ ] 对象存储集成 (媒资托管)
- [ ] Web 管理控制台

详见 [IMPLEMENTATION_PLAN.md](./IMPLEMENTATION_PLAN.md)

## 🤝 贡献

欢迎贡献！请查看 [贡献指南](./CONTRIBUTING.md) (待创建)。

## 📄 许可证

MIT License - 详见 [LICENSE](./LICENSE) 文件

## 🙏 致谢

- [Axum](https://github.com/tokio-rs/axum) - 快速且符合人体工程学的 Web 框架
- [OpenRouter](https://openrouter.ai/) - 统一的 LLM API
- [Anthropic](https://www.anthropic.com/) - Claude API
- [Google Vertex AI](https://cloud.google.com/vertex-ai) - Gemini API

## 📞 联系

- **GitHub**: https://github.com/rickyjim626/xjp-router
- **Issues**: https://github.com/rickyjim626/xjp-router/issues

---

**项目状态**: 🟡 Alpha - 核心功能可用，生产环境需谨慎

最后更新: 2025-10-21
