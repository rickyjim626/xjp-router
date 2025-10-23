use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::{header, Client};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorError, ConnectorResponse};
use crate::core::entities::{ContentPart, UnifiedChunk, UnifiedMessage, UnifiedRequest};
use crate::registry::EgressRoute;
use crate::secret_store::SecretProvider;

pub struct VertexConnector {
    client: Client,
    api_key: Option<String>,
    access_token: Option<String>,
    project: Option<String>,
    region: Option<String>,
}

impl VertexConnector {
    pub async fn new(
        _secret_provider: Arc<dyn SecretProvider>,
        preloaded_secrets: &HashMap<String, String>,
    ) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()?;

        // Get API key from preloaded secrets or environment
        let api_key = preloaded_secrets
            .get("providers/vertex/api-key")
            .cloned()
            .or_else(|| std::env::var("VERTEX_API_KEY").ok());

        // Get access token
        let access_token = preloaded_secrets
            .get("providers/vertex/access-token")
            .cloned()
            .or_else(|| std::env::var("VERTEX_ACCESS_TOKEN").ok());

        // Get project ID
        let project = preloaded_secrets
            .get("providers/vertex/project")
            .cloned()
            .or_else(|| std::env::var("VERTEX_PROJECT").ok());

        // Get region
        let region = preloaded_secrets
            .get("providers/vertex/region")
            .cloned()
            .or_else(|| std::env::var("VERTEX_REGION").ok());

        // Validate: at least one auth method must be present
        if api_key.is_none() && access_token.is_none() {
            tracing::warn!("Vertex AI: Neither API key nor access token found");
        } else {
            tracing::info!("Vertex AI connector initialized with credentials");
        }

        Ok(Self {
            client,
            api_key,
            access_token,
            project,
            region,
        })
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
            stream: true,
        }
    }

    async fn invoke(
        &self,
        route: &EgressRoute,
        req: UnifiedRequest,
    ) -> Result<ConnectorResponse, ConnectorError> {
        // Get project from route, connector config, or fail
        let project = route
            .project
            .clone()
            .or_else(|| self.project.clone())
            .ok_or_else(|| ConnectorError::Invalid("Vertex: missing project ID".into()))?;

        // Get region from route, connector config, or fail
        let region = route
            .region
            .clone()
            .or_else(|| self.region.clone())
            .ok_or_else(|| ConnectorError::Invalid("Vertex: missing region".into()))?;

        let model = &route.provider_model_id;

        // Choose endpoint based on streaming mode
        let endpoint = if req.stream {
            "streamGenerateContent"
        } else {
            "generateContent"
        };
        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/{}:{}",
            region, project, region, model, endpoint
        );

        // Validate authentication
        if self.api_key.is_none() && self.access_token.is_none() {
            return Err(ConnectorError::Auth(
                "Vertex: neither API key nor access token configured".into(),
            ));
        }

        let mut rb = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json");

        if let Some(k) = &self.api_key {
            rb = rb.header("x-goog-api-key", k);
        }
        if let Some(tk) = &self.access_token {
            rb = rb.bearer_auth(tk);
        }

        // Request body
        let mut body = json!({
            "contents": Self::map_messages(&req.messages)
        });

        let mut gen_config = serde_json::Map::new();
        if let Some(t) = req.max_output_tokens {
            gen_config.insert("maxOutputTokens".to_string(), json!(t));
        }
        if let Some(t) = req.temperature {
            gen_config.insert("temperature".to_string(), json!(t));
        }
        if let Some(t) = req.top_p {
            gen_config.insert("topP".to_string(), json!(t));
        }
        if !gen_config.is_empty() {
            body["generationConfig"] = json!(gen_config);
        }

        if req.stream {
            // Streaming mode
            let response = rb
                .json(&body)
                .send()
                .await
                .map_err(|e| ConnectorError::Upstream(e.to_string()))?;

            let status = response.status();
            if !status.is_success() {
                let text = response.text().await.unwrap_or_default();
                return Err(ConnectorError::Upstream(format!(
                    "status {}: {}",
                    status, text
                )));
            }

            let stream =
                response
                    .bytes_stream()
                    .eventsource()
                    .map(|event_result| match event_result {
                        Ok(event) => {
                            let data = event.data;

                            // Vertex AI sends JSON chunks in SSE data field
                            let json_val: serde_json::Value =
                                serde_json::from_str(&data).unwrap_or_default();

                            // Extract text from candidates[0].content.parts[].text
                            let mut text_delta = None;
                            if let Some(parts) = json_val
                                .pointer("/candidates/0/content/parts")
                                .and_then(|x| x.as_array())
                            {
                                let mut text_out = String::new();
                                for p in parts {
                                    if let Some(t) = p.get("text").and_then(|x| x.as_str()) {
                                        text_out.push_str(t);
                                    }
                                }
                                if !text_out.is_empty() {
                                    text_delta = Some(text_out);
                                }
                            }

                            // Check if this is the final chunk
                            let done = json_val.pointer("/candidates/0/finishReason").is_some();

                            Ok(UnifiedChunk {
                                text_delta,
                                tool_call_delta: None,
                                done,
                                provider_events: Some(json_val),
                            })
                        }
                        Err(e) => Err(ConnectorError::Upstream(e.to_string())),
                    });

            Ok(ConnectorResponse::Streaming(Box::pin(stream)))
        } else {
            // Non-streaming mode
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
}
