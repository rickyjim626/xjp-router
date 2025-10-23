use super::{SecretError, Result};
use async_trait::async_trait;
use secret_store_sdk::{Auth, BatchKeys, ClientBuilder, ExportFormat};
use std::collections::HashMap;

/// Trait for secret providers - supports multiple implementations
#[async_trait]
pub trait SecretProvider: Send + Sync {
    /// Get a single secret by key
    async fn get_secret(&self, key: &str) -> Result<String>;

    /// Get multiple secrets in a batch (more efficient)
    async fn get_secrets(&self, keys: &[&str]) -> Result<HashMap<String, String>>;

    /// Refresh/clear cache (optional)
    async fn refresh(&self) -> Result<()>;
}

/// SDK-based secret provider using xjp-secret-store-sdk
pub struct SdkSecretProvider {
    client: secret_store_sdk::Client,
    namespace: String,
}

impl SdkSecretProvider {
    /// Create a new SDK-based secret provider
    pub fn new(base_url: &str, api_key: &str, namespace: &str) -> Result<Self> {
        Self::with_options(base_url, api_key, namespace, 300, 3, 10000)
    }

    /// Create with custom options
    pub fn with_options(
        base_url: &str,
        api_key: &str,
        namespace: &str,
        cache_ttl_secs: u64,
        retries: u32,
        timeout_ms: u64,
    ) -> Result<Self> {
        let client = ClientBuilder::new(base_url)
            .auth(Auth::api_key(api_key))
            .enable_cache(true)
            .cache_ttl_secs(cache_ttl_secs)
            .retries(retries)
            .timeout_ms(timeout_ms)
            .build()
            .map_err(|e| SecretError::Config(format!("Failed to build SDK client: {}", e)))?;

        Ok(Self {
            client,
            namespace: namespace.to_string(),
        })
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> &secret_store_sdk::CacheStats {
        self.client.cache_stats()
    }
}

#[async_trait]
impl SecretProvider for SdkSecretProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        use secrecy::ExposeSecret;

        let secret = self
            .client
            .get_secret(&self.namespace, key, Default::default())
            .await
            .map_err(|e| {
                tracing::warn!("Failed to get secret '{}' from SDK: {}", key, e);
                SecretError::Sdk(e)
            })?;

        Ok(secret.value.expose_secret().to_string())
    }

    async fn get_secrets(&self, keys: &[&str]) -> Result<HashMap<String, String>> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let batch_keys = BatchKeys::Keys(keys.iter().map(|k| k.to_string()).collect());

        let result = self
            .client
            .batch_get(&self.namespace, batch_keys, ExportFormat::Json)
            .await
            .map_err(|e| {
                tracing::warn!("Failed to batch get secrets from SDK: {}", e);
                SecretError::Sdk(e)
            })?;

        match result {
            secret_store_sdk::BatchGetResult::Json(json_result) => {
                tracing::info!(
                    "Batch fetched {} secrets from secret-store",
                    json_result.secrets.len()
                );
                Ok(json_result.secrets)
            }
            _ => Err(SecretError::UnexpectedFormat),
        }
    }

    async fn refresh(&self) -> Result<()> {
        self.client.clear_cache();
        tracing::debug!("Secret store cache cleared");
        Ok(())
    }
}

/// Environment variable-based secret provider (fallback)
pub struct EnvSecretProvider;

impl EnvSecretProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SecretProvider for EnvSecretProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        std::env::var(key).map_err(|_| {
            tracing::debug!("Secret '{}' not found in environment variables", key);
            SecretError::NotFound(key.to_string())
        })
    }

    async fn get_secrets(&self, keys: &[&str]) -> Result<HashMap<String, String>> {
        let mut result = HashMap::new();
        for key in keys {
            if let Ok(value) = std::env::var(key) {
                result.insert(key.to_string(), value);
            }
        }
        tracing::info!(
            "Fetched {} secrets from environment variables",
            result.len()
        );
        Ok(result)
    }

    async fn refresh(&self) -> Result<()> {
        Ok(()) // No-op for environment variables
    }
}

/// Hybrid secret provider: tries SDK first, falls back to environment variables
pub struct HybridSecretProvider {
    sdk: Option<SdkSecretProvider>,
    env: EnvSecretProvider,
}

impl HybridSecretProvider {
    pub fn new(sdk: Option<SdkSecretProvider>) -> Self {
        Self {
            sdk,
            env: EnvSecretProvider::new(),
        }
    }

    pub fn with_sdk(sdk: SdkSecretProvider) -> Self {
        Self {
            sdk: Some(sdk),
            env: EnvSecretProvider::new(),
        }
    }

    pub fn env_only() -> Self {
        Self {
            sdk: None,
            env: EnvSecretProvider::new(),
        }
    }
}

#[async_trait]
impl SecretProvider for HybridSecretProvider {
    async fn get_secret(&self, key: &str) -> Result<String> {
        // Try SDK first
        if let Some(sdk) = &self.sdk {
            match sdk.get_secret(key).await {
                Ok(value) => return Ok(value),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get secret '{}' from SDK: {}, falling back to env",
                        key,
                        e
                    );
                }
            }
        }

        // Fallback to environment variables
        self.env.get_secret(key).await
    }

    async fn get_secrets(&self, keys: &[&str]) -> Result<HashMap<String, String>> {
        // Try SDK first
        if let Some(sdk) = &self.sdk {
            match sdk.get_secrets(keys).await {
                Ok(secrets) => {
                    // Check for missing keys and fetch from env
                    let missing: Vec<_> = keys
                        .iter()
                        .filter(|k| !secrets.contains_key(**k))
                        .copied()
                        .collect();

                    if missing.is_empty() {
                        return Ok(secrets);
                    }

                    tracing::info!(
                        "Fetched {} secrets from SDK, {} missing, checking env",
                        secrets.len(),
                        missing.len()
                    );

                    // Fetch missing keys from environment
                    let env_secrets = self.env.get_secrets(&missing).await?;
                    let mut result = secrets;
                    result.extend(env_secrets);

                    return Ok(result);
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to batch get from SDK: {}, falling back to env",
                        e
                    );
                }
            }
        }

        // Fallback to environment variables only
        self.env.get_secrets(keys).await
    }

    async fn refresh(&self) -> Result<()> {
        if let Some(sdk) = &self.sdk {
            sdk.refresh().await?;
        }
        Ok(())
    }
}

/// Preload secrets at application startup
pub async fn preload_secrets(
    provider: &dyn SecretProvider,
    keys: &[String],
) -> HashMap<String, String> {
    if keys.is_empty() {
        tracing::debug!("No secrets to preload");
        return HashMap::new();
    }

    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();

    match provider.get_secrets(&key_refs).await {
        Ok(secrets) => {
            tracing::info!("Successfully preloaded {} secrets", secrets.len());
            secrets
        }
        Err(e) => {
            tracing::error!("Failed to preload secrets: {}", e);
            HashMap::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_provider() {
        std::env::set_var("TEST_SECRET", "test_value");

        let provider = EnvSecretProvider::new();
        let result = provider.get_secret("TEST_SECRET").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_value");

        std::env::remove_var("TEST_SECRET");
    }

    #[tokio::test]
    async fn test_env_provider_batch() {
        std::env::set_var("SECRET_1", "value1");
        std::env::set_var("SECRET_2", "value2");

        let provider = EnvSecretProvider::new();
        let keys = vec!["SECRET_1", "SECRET_2", "SECRET_3"];
        let result = provider.get_secrets(&keys).await.unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result.get("SECRET_1"), Some(&"value1".to_string()));
        assert_eq!(result.get("SECRET_2"), Some(&"value2".to_string()));

        std::env::remove_var("SECRET_1");
        std::env::remove_var("SECRET_2");
    }

    #[tokio::test]
    async fn test_hybrid_env_only() {
        std::env::set_var("HYBRID_TEST", "hybrid_value");

        let provider = HybridSecretProvider::env_only();
        let result = provider.get_secret("HYBRID_TEST").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hybrid_value");

        std::env::remove_var("HYBRID_TEST");
    }
}
