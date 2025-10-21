use eventsource_stream::Eventsource;
use futures_util::stream::BoxStream;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::{header, Client};
use serde_json::json;
use std::time::Duration;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorError, ConnectorResponse};
use crate::core::entities::{ContentPart, UnifiedChunk, UnifiedMessage, UnifiedRequest};
use crate::registry::EgressRoute;

pub struct OpenRouterConnector {
    client: Client,
    api_key: Option<String>,
}

impl OpenRouterConnector {
    pub fn new() -> Result<Self, ConnectorError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ConnectorError::Internal(e.to_string()))?;
        Ok(Self {
            client,
            api_key: std::env::var("OPENROUTER_API_KEY").ok(),
        })
    }
}

#[async_trait::async_trait]
impl Connector for OpenRouterConnector {
    fn name(&self) -> &'static str {
        "OpenRouter"
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
        let base_url = std::env::var("OPENROUTER_BASE_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());
        let url = format!("{}/chat/completions", base_url);

        // 转换消息
        let messages: Vec<serde_json::Value> = req
            .messages
            .iter()
            .map(|msg| {
                let content = if msg.content.len() == 1 {
                    match &msg.content[0] {
                        ContentPart::Text { text } => json!(text),
                        _ => {
                            let parts: Vec<serde_json::Value> = msg
                                .content
                                .iter()
                                .map(|part| match part {
                                    ContentPart::Text { text } => json!({"type":"text","text":text}),
                                    ContentPart::ImageUrl { url, .. } => {
                                        json!({"type":"image_url","image_url":{"url":url}})
                                    }
                                    ContentPart::ImageB64 { b64, mime } => {
                                        json!({"type":"image_url","image_url":{"url":format!("data:{};base64,{}", mime, b64)}})
                                    }
                                    ContentPart::VideoUrl { url, .. } => {
                                        json!({"type":"text","text":format!("[Video: {}]", url)})
                                    }
                                })
                                .collect();
                            json!(parts)
                        }
                    }
                } else {
                    let parts: Vec<serde_json::Value> = msg
                        .content
                        .iter()
                        .map(|part| match part {
                            ContentPart::Text { text } => json!({"type":"text","text":text}),
                            ContentPart::ImageUrl { url, .. } => {
                                json!({"type":"image_url","image_url":{"url":url}})
                            }
                            ContentPart::ImageB64 { b64, mime } => {
                                json!({"type":"image_url","image_url":{"url":format!("data:{};base64,{}", mime, b64)}})
                            }
                            ContentPart::VideoUrl { url, .. } => {
                                json!({"type":"text","text":format!("[Video: {}]", url)})
                            }
                        })
                        .collect();
                    json!(parts)
                };

                json!({"role": msg.role, "content": content})
            })
            .collect();

        let mut body = json!({
            "model": route.provider_model_id,
            "messages": messages,
            "stream": req.stream,
        });
        if let Some(t) = req.max_output_tokens {
            body["max_tokens"] = json!(t);
        }
        if let Some(t) = req.temperature {
            body["temperature"] = json!(t);
        }
        if let Some(t) = req.top_p {
            body["top_p"] = json!(t);
        }
        for (k, v) in req
            .extra
            .as_object()
            .unwrap_or(&serde_json::Map::new())
            .iter()
        {
            body[k] = v.clone();
        }

        let mut rb = self
            .client
            .post(&url)
            .header(header::CONTENT_TYPE, "application/json");

        if let Some(k) = self
            .api_key
            .clone()
            .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        {
            rb = rb.bearer_auth(k);
        } else {
            return Err(ConnectorError::Auth("missing OPENROUTER_API_KEY".into()));
        }

        if req.stream {
            let response = rb
                .json(&body)
                .send()
                .await
                .map_err(|e| ConnectorError::Upstream(e.to_string()))?;

            let stream = response
                .bytes_stream()
                .eventsource()
                .map(|event_result| match event_result {
                    Ok(event) => {
                        let data = event.data;
                        if data == "[DONE]" {
                            return Ok(UnifiedChunk {
                                text_delta: None,
                                tool_call_delta: None,
                                done: true,
                                provider_events: None,
                            });
                        }
                        let json_val: serde_json::Value =
                            serde_json::from_str(&data).unwrap_or_default();
                        let delta = &json_val["choices"][0]["delta"];
                        let text_delta = delta["content"].as_str().map(String::from);
                        Ok(UnifiedChunk {
                            text_delta,
                            tool_call_delta: None,
                            done: false,
                            provider_events: Some(json_val),
                        })
                    }
                    Err(e) => Err(ConnectorError::Upstream(e.to_string())),
                });

            Ok(ConnectorResponse::Streaming(Box::pin(stream)))
        } else {
            let resp = rb
                .json(&body)
                .send()
                .await
                .map_err(|e| ConnectorError::Upstream(e.to_string()))?;

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| ConnectorError::Upstream(e.to_string()))?;

            let text = json
                .pointer("/choices/0/message/content")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();

            let chunk = UnifiedChunk {
                text_delta: Some(text),
                tool_call_delta: None,
                done: true,
                provider_events: Some(json),
            };

            Ok(ConnectorResponse::NonStreaming(chunk))
        }
    }
}
