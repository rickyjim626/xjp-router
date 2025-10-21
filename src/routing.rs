use crate::connectors::{self, Connector, ConnectorError, ConnectorResponse};
use crate::core::entities::UnifiedRequest;
use crate::registry::{ModelRegistry, ProviderKind};
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    registry: Arc<ModelRegistry>,
    openrouter: Arc<dyn Connector>,
    vertex: Arc<dyn Connector>,
    clewdr: Arc<dyn Connector>,
}

impl AppState {
    pub async fn new(registry: ModelRegistry) -> anyhow::Result<Self> {
        Ok(Self {
            registry: Arc::new(registry),
            openrouter: Arc::new(connectors::openrouter::OpenRouterConnector::new()?),
            vertex: Arc::new(connectors::vertex::VertexConnector::new().await?),
            clewdr: Arc::new(connectors::clewdr::ClewdrConnector::new()?),
        })
    }

    pub async fn invoke(&self, req: UnifiedRequest) -> Result<ConnectorResponse, ConnectorError> {
        let route = self
            .registry
            .resolve(&req.logical_model)
            .map_err(|e| ConnectorError::Invalid(e.to_string()))?;
        let connector: &Arc<dyn Connector> = match route.provider {
            ProviderKind::OpenRouter => &self.openrouter,
            ProviderKind::Vertex => &self.vertex,
            ProviderKind::Clewdr => &self.clewdr,
        };
        connector.invoke(route, req).await
    }
}
