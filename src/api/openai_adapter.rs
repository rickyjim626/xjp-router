use serde::{Deserialize, Serialize};
use serde_json::json;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::core::entities::{UnifiedMessage, ContentPart, UnifiedRequest, UnifiedChunk};

#[derive(Deserialize)]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Option<Vec<serde_json::Value>>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

pub fn to_unified(req: OpenAiChatRequest) -> UnifiedRequest {
    let mut messages = Vec::new();
    for m in req.messages {
        let role = m.get("role").and_then(|x| x.as_str()).unwrap_or("user").to_string();
        let content = m.get("content").cloned().unwrap_or(json!(""));
        let mut parts = Vec::new();
        match content {
            serde_json::Value::String(s) => {
                parts.push(ContentPart::Text { text: s });
            }
            serde_json::Value::Array(arr) => {
                for c in arr {
                    let t = c.get("type").and_then(|x| x.as_str()).unwrap_or("text");
                    match t {
                        "text" => {
                            if let Some(txt) = c.get("text").and_then(|x| x.as_str()) {
                                parts.push(ContentPart::Text { text: txt.to_string() });
                            }
                        }
                        "image_url" => {
                            if let Some(u) = c.get("image_url").and_then(|x| x.get("url")).and_then(|x| x.as_str()) {
                                parts.push(ContentPart::ImageUrl { url: u.to_string(), mime: None });
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        messages.push(UnifiedMessage { role, content: parts, name: None });
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

#[derive(Serialize)]
struct OpenAiDelta {
    pub role: Option<String>,
    pub content: Option<String>,
}

#[derive(Serialize)]
struct OpenAiChoiceDelta {
    pub index: u32,
    pub delta: OpenAiDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

#[derive(Serialize)]
struct OpenAiStreamChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<OpenAiChoiceDelta>,
}

pub fn from_unified_chunk(model: &str, chunk: UnifiedChunk) -> serde_json::Value {
    let id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = OffsetDateTime::now_utc().unix_timestamp();
    let delta = OpenAiDelta {
        role: None,
        content: chunk.text_delta.clone(),
    };
    let choice = OpenAiChoiceDelta { index: 0, delta, finish_reason: if chunk.done { Some("stop".into()) } else { None } };
    serde_json::to_value(OpenAiStreamChunk {
        id, object: "chat.completion.chunk".into(), created, model: model.into(), choices: vec![choice]
    }).unwrap()
}

pub fn from_unified_final(model: &str, chunk: UnifiedChunk) -> serde_json::Value {
    let id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = OffsetDateTime::now_utc().unix_timestamp();
    json!({
        "id": id,
        "object": "chat.completion",
        "created": created,
        "model": model,
        "choices": [{
            "index": 0,
            "finish_reason": "stop",
            "message": {
                "role": "assistant",
                "content": chunk.text_delta.unwrap_or_default()
            }
        }]
    })
}
