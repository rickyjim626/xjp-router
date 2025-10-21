use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::core::entities::{ContentPart, UnifiedChunk, UnifiedMessage, UnifiedRequest};

#[derive(Deserialize)]
pub struct AnthropicMessagesRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

pub fn to_unified(req: AnthropicMessagesRequest) -> UnifiedRequest {
    let mut messages = Vec::new();
    if let Some(sys) = req.system {
        messages.push(UnifiedMessage {
            role: "system".into(),
            content: vec![ContentPart::Text { text: sys }],
            name: None,
        });
    }
    for m in req.messages {
        let role = m
            .get("role")
            .and_then(|x| x.as_str())
            .unwrap_or("user")
            .to_string();
        let content = m.get("content").cloned().unwrap_or(json!(""));
        let mut parts = Vec::new();
        match content {
            serde_json::Value::String(s) => parts.push(ContentPart::Text { text: s }),
            serde_json::Value::Array(arr) => {
                for c in arr {
                    let t = c.get("type").and_then(|x| x.as_str()).unwrap_or("text");
                    if t == "text" {
                        if let Some(txt) = c.get("text").and_then(|x| x.as_str()) {
                            parts.push(ContentPart::Text {
                                text: txt.to_string(),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
        messages.push(UnifiedMessage {
            role,
            content: parts,
            name: None,
        });
    }

    UnifiedRequest {
        logical_model: req.model,
        messages,
        tools: None,
        tool_choice: None,
        max_output_tokens: req.max_tokens,
        temperature: req.temperature,
        top_p: req.top_p,
        stream: req.stream.unwrap_or(false),
        extra: req.extra,
    }
}

pub fn final_message_json(model: &str, chunk: UnifiedChunk) -> serde_json::Value {
    let id = format!("msg_{}", Uuid::new_v4());
    json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{
            "type": "text",
            "text": chunk.text_delta.unwrap_or_default()
        }],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": null, "output_tokens": null }
    })
}
