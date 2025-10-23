use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use reqwest::{header, Client};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::connectors::{Connector, ConnectorCapabilities, ConnectorError, ConnectorResponse};
use crate::core::entities::{ContentPart, UnifiedChunk, UnifiedRequest};
use crate::registry::EgressRoute;
use crate::secret_store::SecretProvider;

pub struct OpenRouterConnector {
    client: Client,
    api_key: Option<String>,
    base_url: String,
}

impl OpenRouterConnector {
    pub fn new(
        _secret_provider: Arc<dyn SecretProvider>,
        preloaded_secrets: &HashMap<String, String>,
    ) -> Result<Self, ConnectorError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| ConnectorError::Internal(e.to_string()))?;

        // Get API key from preloaded secrets or environment
        let api_key = preloaded_secrets
            .get("providers/openrouter/api-key")
            .cloned()
            .or_else(|| std::env::var("OPENROUTER_API_KEY").ok());

        if api_key.is_none() {
            tracing::warn!("OpenRouter API key not found in preloaded secrets or environment");
        } else {
            tracing::info!("OpenRouter connector initialized with API key");
        }

        // Get base URL (optional, has default)
        let base_url = preloaded_secrets
            .get("providers/openrouter/base-url")
            .cloned()
            .or_else(|| std::env::var("OPENROUTER_BASE_URL").ok())
            .unwrap_or_else(|| "https://openrouter.ai/api/v1".to_string());

        Ok(Self {
            client,
            api_key,
            base_url,
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
            tools: true,
            stream: true,
        }
    }

    async fn invoke(
        &self,
        route: &EgressRoute,
        req: UnifiedRequest,
    ) -> Result<ConnectorResponse, ConnectorError> {
        let url = format!("{}/chat/completions", self.base_url);

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
        // Add tools if present (OpenAI format)
        if let Some(tools) = &req.tools {
            let tools_json: Vec<serde_json::Value> = tools
                .iter()
                .map(|tool| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": tool.json_schema
                        }
                    })
                })
                .collect();
            body["tools"] = json!(tools_json);
        }
        if let Some(choice) = &req.tool_choice {
            body["tool_choice"] = json!(choice);
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

        if let Some(k) = &self.api_key {
            rb = rb.bearer_auth(k);
        } else {
            return Err(ConnectorError::Auth("OpenRouter API key not configured".into()));
        }

        if req.stream {
            let response = rb
                .json(&body)
                .send()
                .await
                .map_err(|e| ConnectorError::Upstream(e.to_string()))?;

            let stream =
                response
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

                            // Check for tool_calls in delta
                            let tool_call_delta = delta.get("tool_calls").cloned();

                            Ok(UnifiedChunk {
                                text_delta,
                                tool_call_delta,
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

            // Check for tool_calls in message
            let tool_call_delta = json.pointer("/choices/0/message/tool_calls").cloned();

            let chunk = UnifiedChunk {
                text_delta: if text.is_empty() { None } else { Some(text) },
                tool_call_delta,
                done: true,
                provider_events: Some(json),
            };

            Ok(ConnectorResponse::NonStreaming(chunk))
        }
    }
}
