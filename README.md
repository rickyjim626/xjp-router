# XiaojinPro AI Gateway (Rust, Minimal)

一个最小可运行（MVP）的多供应商 AI 网关，支持：
- **OpenAI 兼容** `/v1/chat/completions`
- **Anthropic 兼容** `/v1/messages`
- **原子层连接器**：OpenRouter（流式）、Vertex（非流式最小实现）、Clewdr（按 OpenAI 兼容形态）
- **XJPkey 鉴权**（`Authorization: Bearer XJP...` 或 `x-api-key: XJP...`）

> 说明：为了最小可用，Vertex 与 Clewdr 实现按“非流式/简化”交付；OpenRouter 支持 SSE 流式。你可以据此扩展。

## 快速开始

```bash
# 1) 环境变量（至少设置一个上游）
export OPENROUTER_API_KEY=or_************************
# 可选：
export CLEWDR_BASE_URL=http://localhost:9000
export CLEWDR_API_KEY=cl_****************************

# Vertex（任选其一）
export VERTEX_API_KEY=AIza***********************************
# 或者使用 OAuth 访问令牌（通过 gcloud 生成）
# export VERTEX_ACCESS_TOKEN="$(gcloud auth print-access-token)"
export VERTEX_PROJECT=your-gcp-project
export VERTEX_REGION=us-central1

# 2) 配置模型路由
cp config/xjp.example.toml config/xjp.toml

# 3) 运行
cargo run

# 4) 测试（OpenAI 兼容）
curl -N http://127.0.0.1:8080/v1/chat/completions \
  -H "Authorization: Bearer XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role":"user","content":"用一句话介绍你自己"}],
    "stream": true
  }'
```

### Anthropic 兼容（非流式/最小）
```bash
curl http://127.0.0.1:8080/v1/messages \
  -H "x-api-key: XJP_test_key" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4.5",
    "messages": [{"role":"user","content":"解释一下 SSE 是什么？"}],
    "stream": false
  }'
```

## 路由配置（config/xjp.toml）

示例：把逻辑模型 `claude-sonnet-4.5` 走 OpenRouter；`gemini-1.5-pro` 走 Vertex；`my-clewdr-model` 走 clewdr。

```toml
[models."claude-sonnet-4.5".primary]
provider = "OpenRouter"
provider_model_id = "anthropic/claude-3.5-sonnet"

[models."gemini-1.5-pro".primary]
provider = "Vertex"
provider_model_id = "publishers/google/models/gemini-1.5-pro-002"
region = "us-central1"
project = "your-gcp-project"

[models."my-clewdr-model".primary]
provider = "Clewdr"
provider_model_id = "gpt-4o-like"
```

> 运行时可通过环境变量覆盖某些参数（如 OPENROUTER_API_KEY、VERTEX_API_KEY、CLEWDR_BASE_URL, CLEWDR_API_KEY）。

## 设计取舍（MVP）

- **统一请求/流抽象**：`UnifiedRequest` / `UnifiedChunk`。
- **OpenRouter**：完整支持 ChatCompletions + SSE；SSE 解析使用 `reqwest-eventsource`。
- **Vertex**：使用 `generateContent`（非流式）；若你设置 `VERTEX_API_KEY` 或 `VERTEX_ACCESS_TOKEN` 则可用。
- **Clewdr**：按 OpenAI 兼容 `/v1/chat/completions` 最小实现（非流式）。
- **XJPkey**：只校验前缀为 `XJP` 的 Key 即通过（你可替换为 DB 校验）。

## Docker

```dockerfile
# docker build -t xjp-gateway:latest .
# docker run -p 8080:8080 --env OPENROUTER_API_KEY=or_xxx xjp-gateway:latest
```

## 进一步扩展
- 给 Vertex 增加 `streamGenerateContent` SSE 解析。
- 为 Clewdr 增加 SSE 与工具调用映射。
- 接入 Prometheus/OpenTelemetry、限流与计量。
- Admin 控制台与模型/Key 管理。

MIT License.
