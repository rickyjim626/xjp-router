use crate::connectors::{self, Connector, ConnectorError, ConnectorResponse};
use crate::core::entities::UnifiedRequest;
use crate::db::{KeyStore, BillingStore};
use crate::registry::{ModelRegistry, ProviderKind};
use crate::secret_store::SecretProvider;
use crate::billing::{PricingCache, BillingInterceptor};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    registry: Arc<ModelRegistry>,
    openrouter: Arc<dyn Connector>,
    vertex: Arc<dyn Connector>,
    clewdr: Arc<dyn Connector>,
    key_store: Arc<dyn KeyStore>,
    pub pricing: Arc<PricingCache>,
    billing_store: Arc<dyn BillingStore>,
    billing_interceptor: Arc<BillingInterceptor>,
}

impl AppState {
    pub async fn new(
        registry: ModelRegistry,
        key_store: Arc<dyn KeyStore>,
        secret_provider: Arc<dyn SecretProvider>,
        preloaded_secrets: HashMap<String, String>,
        billing_store: Arc<dyn BillingStore>,
    ) -> anyhow::Result<Self> {
        let pricing = Arc::new(PricingCache::new()?);
        Ok(Self {
            registry: Arc::new(registry),
            openrouter: Arc::new(connectors::openrouter::OpenRouterConnector::new(
                secret_provider.clone(),
                &preloaded_secrets,
            )?),
            vertex: Arc::new(connectors::vertex::VertexConnector::new(
                secret_provider.clone(),
                &preloaded_secrets,
            ).await?),
            clewdr: Arc::new(connectors::clewdr::ClewdrConnector::new(
                secret_provider,
                &preloaded_secrets,
            )?),
            key_store,
            pricing: pricing.clone(),
            billing_store: billing_store.clone(),
            billing_interceptor: Arc::new(BillingInterceptor::new(pricing)),
        })
    }

    pub fn key_store(&self) -> Arc<dyn KeyStore> {
        Arc::clone(&self.key_store)
    }

    pub fn billing_store(&self) -> Arc<dyn BillingStore> {
        Arc::clone(&self.billing_store)
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

    /// Invoke with billing tracking (for authenticated requests)
    pub async fn invoke_with_billing(
        &self,
        req: UnifiedRequest,
        tenant_id: String,
        api_key_id: Uuid,
    ) -> Result<ConnectorResponse, ConnectorError> {
        let route = self
            .registry
            .resolve(&req.logical_model)
            .map_err(|e| ConnectorError::Invalid(e.to_string()))?;

        // Create billing context before request
        let billing_ctx = self.billing_interceptor.before_request(
            &req,
            tenant_id,
            api_key_id,
            route.provider.to_string(),
            route.provider_model_id.clone(),
        );

        // Execute actual request
        let connector: &Arc<dyn Connector> = match route.provider {
            ProviderKind::OpenRouter => &self.openrouter,
            ProviderKind::Vertex => &self.vertex,
            ProviderKind::Clewdr => &self.clewdr,
        };
        let result = connector.invoke(route, req).await;

        // Record billing (async, non-blocking)
        let interceptor = self.billing_interceptor.clone();
        let billing_store = self.billing_store.clone();
        match &result {
            Ok(response) => {
                tokio::spawn(async move {
                    match interceptor.after_request(
                        billing_ctx,
                        response,
                        "success",
                        None,
                    ).await {
                        Ok(transaction) => {
                            if let Err(e) = billing_store.insert_transaction(transaction).await {
                                tracing::error!("Failed to insert billing transaction: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to process billing: {}", e);
                        }
                    }
                });
            }
            Err(e) => {
                // Record failed request
                tokio::spawn(async move {
                    match interceptor.after_request(
                        billing_ctx,
                        &ConnectorResponse::NonStreaming(Default::default()),
                        "error",
                        Some(e.to_string()),
                    ).await {
                        Ok(transaction) => {
                            if let Err(e) = billing_store.insert_transaction(transaction).await {
                                tracing::error!("Failed to insert billing transaction: {}", e);
                            }
                        }
                        Err(e) => {
                            tracing::error!("Failed to process billing for error case: {}", e);
                        }
                    }
                });
            }
        }

        result
    }
}
