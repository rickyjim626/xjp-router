use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageDetails {
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
    #[serde(default)]
    pub cached_tokens: Option<u64>,
    #[serde(default)]
    pub audio_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UsageFields {
    #[serde(default)]
    pub completion_tokens: Option<u64>,
    #[serde(default)]
    pub completion_tokens_details: Option<UsageDetails>,
    #[serde(default)]
    pub prompt_tokens: Option<u64>,
    #[serde(default)]
    pub prompt_tokens_details: Option<UsageDetails>,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OrUsage {
    #[serde(default)]
    pub usage: Option<UsageFields>
}

impl OrUsage {
    pub fn into_token_usage(self) -> crate::billing::tokens::TokenUsage {
        let mut u = crate::billing::tokens::TokenUsage::default();
        if let Some(usage) = self.usage {
            u.prompt_tokens = usage.prompt_tokens.unwrap_or(0);
            u.completion_tokens = usage.completion_tokens.unwrap_or(0);
            if let Some(det) = usage.completion_tokens_details {
                u.reasoning_tokens = det.reasoning_tokens.unwrap_or(0);
            }
            if let Some(det) = usage.prompt_tokens_details {
                u.cached_prompt_tokens = det.cached_tokens.unwrap_or(0);
            }
        }
        u
    }
}
