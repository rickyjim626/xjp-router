use std::time::Duration;
use futures_util::{StreamExt, TryStreamExt};
use futures_util::stream::BoxStream;
use reqwest::{Client, header};
use reqwest_eventsource::{EventSource, RequestBuilderExt};
use serde_json::json;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorResponse, ConnectorError};
use crate::registry::EgressRoute;
use crate::core::entities::{UnifiedRequest, UnifiedMessage, ContentPart, UnifiedChunk};

pub struct OpenRouterConnector {
    client: Client,
    base: String,
    api_key: Option<String>,
}

impl OpenRouterConnector {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        let base = std::env::var("OPENROUTER_BASE_URL").unwrap_or_else(|_| "https://openrouter.ai/api".to_string());
        let api_key = std::env::var("OPENROUTER_API_KEY").ok();
        Ok(Self { client, base, api_key })
    }

    fn map_messages(messages: &[UnifiedMessage]) -> anyhow::Result<Vec<serde_json::Value>> {
        let mut out = Vec::new();
        for m in messages {
            // OpenAI 风格 content 可以是 string 或 array; 我们统一用 array
            let mut parts = Vec::new();
            for p in &m.content {
                match p {
                    ContentPart::Text { text } => parts.push(json!({"type":"text", "text": text})),
                    ContentPart::ImageUrl { url, .. } => parts.push(json!({"type":"image_url","image_url":{"url": url}})),
                    ContentPart::ImageB64 { b64, mime } => parts.push(json!({"type":"image_url","image_url":{"url": format!("data:{};base64,{}", mime, b64)}})),
                    ContentPart::VideoUrl { url, .. } => parts.push(json!({"type":"input_text","text": format!("(video) {}", url)})), // 简化：当作文本提示
                }
            }
            if parts.is_empty() {
                // 兼容只有纯文本的情况（content 为空时跳过）
                continue;
            }
            out.push(json!({
                "role": m.role,
                "content": parts
            }));
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl Connector for OpenRouterConnector {
    fn name(&self) -> &'static str { "openrouter" }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities { text: true, vision: true, video: false, tools: false, stream: true }
    }

    async fn invoke(&self, route: &EgressRoute, req: UnifiedRequest) -> Result<ConnectorResponse, ConnectorError> {
        let model = &route.provider_model_id;
        let url = format!("{}/v1/chat/completions", self.base);
        let mut body = json!({
            "model": model,
            "messages": Self::map_messages(&req.messages).map_err(|e| ConnectorError::Invalid(e.to_string()))?,
            "stream": req.stream,
        });
        if let Some(t) = req.max_output_tokens { body["max_tokens"] = json!(t); }
        if let Some(t) = req.temperature { body["temperature"] = json!(t); }
        if let Some(t) = req.top_p { body["top_p"] = json!(t); }
        // 透传 extra
        for (k, v) in req.extra.as_object().unwrap_or(&serde_json::Map::new()).iter() {
            body[k] = v.clone();
        }

        let mut rb = self.client.post(&url)
            .header(header::CONTENT_TYPE, "application/json");

        // auth
        if let Some(k) = self.api_key.clone().or_else(|| std::env::var("OPENROUTER_API_KEY").ok()) {
            rb = rb.bearer_auth(k);
        } else {
            return Err(ConnectorError::Auth("missing OPENROUTER_API_KEY".into()));
        }

        if req.stream {
            // SSE 流式
            let es = EventSource::new(rb.json(&body))?;
            let stream = es
                .map_err(|e| ConnectorError::Upstream(e.to_string()))
                .try_filter_map(|event| async move {
                    match event {
                        reqwest_eventsource::Event::Open => Ok(None),
                        reqwest_eventsource::Event::Message(m) => {
                            if m.data == "[DONE]" {
                                let done = UnifiedChunk { text_delta: None, tool_call_delta: None, done: true, provider_events: None };
                                return Ok(Some(done));
                            }
                            // 解析 OpenAI/OR 的 chunk
                            let v: serde_json::Value = match serde_json::from_str(&m.data) {
                                Ok(v) => v,
                                Err(_) => return Ok(None),
                            };
                            let delta = v.pointer("/choices/0/delta/content")
                                .and_then(|x| x.as_str()).map(|s| s.to_string());
                            let chunk = UnifiedChunk {
                                text_delta: delta,
                                tool_call_delta: None,
                                done: false,
                                provider_events: Some(v),
                            };
                            Ok(Some(chunk))
                        }
                    }
                });
            let boxed: BoxStream<'static, Result<UnifiedChunk, ConnectorError>> = Box::pin(stream);
            Ok(ConnectorResponse::Streaming(boxed))
        } else {
            let resp = rb.json(&body).send().await?;
            let status = resp.status();
            if !status.is_success() {
                return Err(ConnectorError::Upstream(format!("status {}", status)));
            }
            let v: serde_json::Value = resp.json().await?;
            let content = v.pointer("/choices/0/message/content")
                .and_then(|x| x.as_str()).unwrap_or("").to_string();
            let chunk = UnifiedChunk { text_delta: Some(content), tool_call_delta: None, done: true, provider_events: Some(v) };
            Ok(ConnectorResponse::NonStreaming(chunk))
        }
    }
}
