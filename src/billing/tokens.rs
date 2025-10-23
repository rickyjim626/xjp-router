use crate::core::entities::{UnifiedMessage, ContentPart};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub cached_prompt_tokens: u64,
}

#[async_trait::async_trait]
pub trait TokenCounter: Send + Sync {
    async fn count_prompt(&self, model_tokenizer: &str, messages: &[UnifiedMessage]) -> anyhow::Result<u64>;
}

pub struct GptTokenCounter;

#[async_trait::async_trait]
impl TokenCounter for GptTokenCounter {
    async fn count_prompt(&self, model_tokenizer: &str, messages: &[UnifiedMessage]) -> anyhow::Result<u64> {
        use tiktoken_rs::{o200k_base, cl100k_base, CoreBPE};
        let enc: CoreBPE = match model_tokenizer {
            "o200k_base" | "gpt-4o" | "gpt-4.1" | "gpt-5" => o200k_base()?,
            _ => cl100k_base()?,
        };
        let mut text = String::new();
        for m in messages {
            for p in &m.content {
                if let ContentPart::Text { text: t } = p {
                    text.push_str(t);
                    text.push('\n');
                }
            }
        }
        let n = enc.encode_with_special_tokens(&text).len() as u64;
        Ok(n)
    }
}

pub struct ClaudeTokenCounter;

#[async_trait::async_trait]
impl TokenCounter for ClaudeTokenCounter {
    async fn count_prompt(&self, _model_tokenizer: &str, messages: &[UnifiedMessage]) -> anyhow::Result<u64> {
        let mut text = String::new();
        for m in messages {
            for p in &m.content {
                if let ContentPart::Text { text: t } = p {
                    text.push_str(t);
                    text.push('\n');
                }
            }
        }
        let n = claude_tokenizer::count_tokens(&text)? as u64;
        Ok(n)
    }
}
