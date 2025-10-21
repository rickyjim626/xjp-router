use std::time::Duration;
use reqwest::{Client, header};
use serde_json::json;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorResponse, ConnectorError};
use crate::registry::EgressRoute;
use crate::core::entities::{UnifiedRequest, UnifiedMessage, ContentPart, UnifiedChunk};

pub struct ClewdrConnector {
    client: Client,
    base: String,
    api_key: Option<String>,
}

impl ClewdrConnector {
    pub fn new() -> anyhow::Result<Self> {
        let client = Client::builder().timeout(Duration::from_secs(120)).build()?;
        let base = std::env::var("CLEWDR_BASE_URL").unwrap_or_else(|_| "http://localhost:9000".to_string());
        let api_key = std::env::var("CLEWDR_API_KEY").ok();
        Ok(Self { client, base, api_key })
    }

    fn map_messages(messages: &[UnifiedMessage]) -> anyhow::Result<Vec<serde_json::Value>> {
        let mut out = Vec::new();
        for m in messages {
            let mut parts = Vec::new();
            for p in &m.content {
                match p {
                    ContentPart::Text { text } => parts.push(serde_json::json!({"type":"text","text":text})),
                    ContentPart::ImageUrl { url, .. } => parts.push(serde_json::json!({"type":"image_url","image_url":{"url":url}})),
                    ContentPart::ImageB64 { b64, mime } => parts.push(serde_json::json!({"type":"image_url","image_url":{"url": format!("data:{};base64,{}", mime, b64)}})),
                    ContentPart::VideoUrl { url, .. } => parts.push(serde_json::json!({"type":"input_text","text": format!("(video) {}", url)})),
                }
            }
            if !parts.is_empty() {
                out.push(serde_json::json!({"role": m.role, "content": parts}));
            }
        }
        Ok(out)
    }
}

#[async_trait::async_trait]
impl Connector for ClewdrConnector {
    fn name(&self) -> &'static str { "clewdr" }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities { text: true, vision: true, video: false, tools: false, stream: false }
    }

    async fn invoke(&self, route: &EgressRoute, req: UnifiedRequest) -> Result<ConnectorResponse, ConnectorError> {
        let url = format!("{}/v1/chat/completions", self.base);
        let mut body = json!({
            "model": route.provider_model_id,
            "messages": Self::map_messages(&req.messages).map_err(|e| ConnectorError::Invalid(e.to_string()))?,
            "stream": false
        });
        if let Some(t) = req.max_output_tokens { body["max_tokens"] = json!(t); }
        if let Some(t) = req.temperature { body["temperature"] = json!(t); }
        if let Some(t) = req.top_p { body["top_p"] = json!(t); }

        let mut rb = self.client.post(&url).header(header::CONTENT_TYPE, "application/json");
        if let Some(k) = self.api_key.clone().or_else(|| std::env::var("CLEWDR_API_KEY").ok()) {
            rb = rb.bearer_auth(k);
        }

        let resp = rb.json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::Upstream(format!("status {}: {}", status, text)));
        }
        let v: serde_json::Value = resp.json().await?;
        let content = v.pointer("/choices/0/message/content").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let chunk = UnifiedChunk { text_delta: Some(content), tool_call_delta: None, done: true, provider_events: Some(v) };
        Ok(ConnectorResponse::NonStreaming(chunk))
    }
}
