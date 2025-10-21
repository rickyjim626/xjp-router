use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    ImageUrl { url: String, #[serde(default)] mime: Option<String> },
    #[serde(rename = "image_b64")]
    ImageB64 { b64: String, mime: String },
    #[serde(rename = "video_url")]
    VideoUrl { url: String, #[serde(default)] mime: Option<String> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnifiedMessage {
    pub role: String, // "system" | "user" | "assistant" | "tool"
    #[serde(default)]
    pub content: Vec<ContentPart>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub json_schema: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnifiedRequest {
    pub logical_model: String,
    pub messages: Vec<UnifiedMessage>,
    #[serde(default)]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(default)]
    pub tool_choice: Option<String>,
    #[serde(default)]
    pub max_output_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UnifiedChunk {
    #[serde(default)]
    pub text_delta: Option<String>,
    #[serde(default)]
    pub tool_call_delta: Option<serde_json::Value>,
    pub done: bool,
    #[serde(default)]
    pub provider_events: Option<serde_json::Value>,
}
