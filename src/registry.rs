use std::collections::HashMap;
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub enum ProviderKind { OpenRouter, Vertex, Clewdr }

#[derive(Clone, Debug, Deserialize)]
pub struct EgressRoute {
    pub provider: ProviderKind,
    pub provider_model_id: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub extra: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub timeouts_ms: Option<u64>,
}

#[derive(Default, Clone)]
pub struct ModelRegistry {
    routes: HashMap<String, Vec<EgressRoute>>,
}

impl ModelRegistry {
    pub fn resolve(&self, logical_model: &str) -> anyhow::Result<&EgressRoute> {
        self.routes.get(logical_model)
            .and_then(|v| v.first())
            .ok_or_else(|| anyhow::anyhow!("model '{}' not found", logical_model))
    }
}

#[derive(Deserialize)]
struct FileModel {
    primary: EgressRoute
}

#[derive(Deserialize)]
struct FileConfig {
    #[serde(rename = "models")]
    models: HashMap<String, FileModel>,
}

pub async fn load_from_toml(path: &str) -> anyhow::Result<ModelRegistry> {
    let text = tokio::fs::read_to_string(path).await
        .or_else(|_| tokio::fs::read_to_string("config/xjp.example.toml")).await?;
    let cfg: FileConfig = toml::from_str(&text)?;
    let mut map = HashMap::new();
    for (k, v) in cfg.models.into_iter() {
        map.insert(k, vec![v.primary]);
    }
    Ok(ModelRegistry { routes: map })
}
