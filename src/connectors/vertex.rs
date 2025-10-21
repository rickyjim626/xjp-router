use reqwest::{header, Client};
use serde_json::json;
use std::time::Duration;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorError, ConnectorResponse};
use crate::core::entities::{ContentPart, UnifiedChunk, UnifiedMessage, UnifiedRequest};
use crate::registry::EgressRoute;

pub struct VertexConnector {
    client: Client,
}

impl VertexConnector {
    pub async fn new() -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;
        Ok(Self { client })
    }

    fn map_messages(messages: &[UnifiedMessage]) -> serde_json::Value {
        // Vertex: contents: [{role, parts:[{text}|{fileData}|{inlineData}]}]
        let mut contents = Vec::new();
        for m in messages {
            let mut parts = Vec::new();
            for p in &m.content {
                match p {
                    ContentPart::Text { text } => parts.push(json!({"text": text})),
                    ContentPart::ImageUrl { url, mime } => parts.push(json!({"fileData": {"fileUri": url, "mimeType": mime.clone().unwrap_or("image/*".into())}})),
                    ContentPart::ImageB64 { b64, mime } => parts.push(json!({"inlineData": {"data": b64, "mimeType": mime}})),
                    ContentPart::VideoUrl { url, mime } => parts.push(json!({"fileData": {"fileUri": url, "mimeType": mime.clone().unwrap_or("video/*".into())}})),
                }
            }
            if !parts.is_empty() {
                contents.push(json!({
                    "role": if m.role == "assistant" { "model" } else { "user" },
                    "parts": parts
                }));
            }
        }
        json!(contents)
    }

    fn auth_headers() -> Result<(Option<String>, Option<String>), ConnectorError> {
        let api_key = std::env::var("VERTEX_API_KEY").ok();
        let access_token = std::env::var("VERTEX_ACCESS_TOKEN").ok();
        if api_key.is_none() && access_token.is_none() {
            return Err(ConnectorError::Auth(
                "set VERTEX_API_KEY or VERTEX_ACCESS_TOKEN".into(),
            ));
        }
        Ok((api_key, access_token))
    }
}

#[async_trait::async_trait]
impl Connector for VertexConnector {
    fn name(&self) -> &'static str {
        "vertex"
    }

    fn capabilities(&self) -> ConnectorCapabilities {
        ConnectorCapabilities {
            text: true,
            vision: true,
            video: true,
            tools: false,
            stream: false,
        }
    }

    async fn invoke(
        &self,
        route: &EgressRoute,
        req: UnifiedRequest,
    ) -> Result<ConnectorResponse, ConnectorError> {
        // Non-streaming minimal implementation (generateContent)
        let project = route
            .project
            .clone()
            .or_else(|| std::env::var("VERTEX_PROJECT").ok())
            .ok_or_else(|| ConnectorError::Invalid("missing project".into()))?;
        let region = route
            .region
            .clone()
            .or_else(|| std::env::var("VERTEX_REGION").ok())
            .ok_or_else(|| ConnectorError::Invalid("missing region".into()))?;
        let model = &route.provider_model_id;
        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/{}:generateContent",
            region, project, region, model
        );

        let (api_key, access_token) = Self::auth_headers()?;

        let mut rb = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json");
        if let Some(k) = api_key {
            rb = rb.header("x-goog-api-key", k);
        }
        if let Some(tk) = access_token {
            rb = rb.bearer_auth(tk);
        }

        // Request body
        let mut body = json!({
            "contents": Self::map_messages(&req.messages)
        });
        if let Some(t) = req.max_output_tokens {
            body["generationConfig"] = json!({"maxOutputTokens": t});
        }
        if let Some(t) = req.temperature {
            body["generationConfig"]["temperature"] = json!(t);
        }
        if let Some(t) = req.top_p {
            body["generationConfig"]["topP"] = json!(t);
        }

        let resp = rb.json(&body).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(ConnectorError::Upstream(format!(
                "status {}: {}",
                status, text
            )));
        }
        let v: serde_json::Value = resp.json().await?;
        // Aggregate text
        let mut text_out = String::new();
        if let Some(parts) = v
            .pointer("/candidates/0/content/parts")
            .and_then(|x| x.as_array())
        {
            for p in parts {
                if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                    text_out.push_str(t);
                }
            }
        }
        let chunk = UnifiedChunk {
            text_delta: Some(text_out),
            tool_call_delta: None,
            done: true,
            provider_events: Some(v),
        };
        Ok(ConnectorResponse::NonStreaming(chunk))
    }
}
