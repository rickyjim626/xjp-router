use std::{collections::HashMap, time::{Duration, Instant}};
use serde::Deserialize;
use tokio::sync::RwLock;
use reqwest::Client;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Pricing {
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub completion: Option<String>,
    #[serde(default)]
    pub request: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub web_search: Option<String>,
    #[serde(default)]
    pub internal_reasoning: Option<String>,
    #[serde(default)]
    pub input_cache_read: Option<String>,
    #[serde(default)]
    pub input_cache_write: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct ModelPricing {
    pub prompt: f64,
    pub completion: f64,
    pub request: f64,
    pub image: f64,
    pub web_search: f64,
    pub internal_reasoning: f64,
    pub input_cache_read: f64,
    pub input_cache_write: f64,
}

impl From<Pricing> for ModelPricing {
    fn from(p: Pricing) -> Self {
        fn parse(s: &Option<String>) -> f64 {
            s.as_ref().and_then(|x| x.parse::<f64>().ok()).unwrap_or(0.0)
        }
        Self {
            prompt: parse(&p.prompt),
            completion: parse(&p.completion),
            request: parse(&p.request),
            image: parse(&p.image),
            web_search: parse(&p.web_search),
            internal_reasoning: parse(&p.internal_reasoning),
            input_cache_read: parse(&p.input_cache_read),
            input_cache_write: parse(&p.input_cache_write),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
struct ModelEntry {
    id: String,
    #[serde(default)]
    pricing: Option<Pricing>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelEntry>,
}

pub struct PricingCache {
    client: Client,
    cache: RwLock<HashMap<String, (ModelPricing, Instant)>>,
    ttl: Duration,
}

impl PricingCache {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: Client::builder().timeout(Duration::from_secs(30)).build()?,
            cache: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(900),
        })
    }

    pub async fn get(&self, model_id: &str) -> anyhow::Result<ModelPricing> {
        {
            let map = self.cache.read().await;
            if let Some((mp, ts)) = map.get(model_id) {
                if ts.elapsed() < self.ttl {
                    return Ok(mp.clone());
                }
            }
        }
        let api_key = std::env::var("OPENROUTER_API_KEY")
            .map_err(|_| anyhow::anyhow!("OPENROUTER_API_KEY not set for pricing fetch"))?;

        let resp = self.client
            .get("https://openrouter.ai/api/v1/models")
            .bearer_auth(api_key)
            .send().await?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!("fetch models failed: {}", resp.status()));
        }
        let v: ModelsResponse = resp.json().await?;
        let mut map = self.cache.write().await;
        for m in v.data {
            if let Some(p) = m.pricing {
                map.insert(m.id.clone(), (ModelPricing::from(p), Instant::now()));
            }
        }
        if let Some((mp, _)) = map.get(model_id) {
            Ok(mp.clone())
        } else {
            Err(anyhow::anyhow!("pricing not found for model {}", model_id))
        }
    }
}
