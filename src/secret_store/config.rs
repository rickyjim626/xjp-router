use serde::{Deserialize, Serialize};

/// Configuration for secret store integration
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SecretStoreConfig {
    /// Whether secret store is enabled
    #[serde(default)]
    pub enabled: bool,

    /// Secret store base URL
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Namespace to use (e.g., "router")
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl_secs")]
    pub cache_ttl_secs: u64,

    /// Number of retries for failed requests
    #[serde(default = "default_retries")]
    pub retries: u32,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Whether to preload secrets at startup
    #[serde(default = "default_preload")]
    pub preload: bool,

    /// List of secret keys to preload
    #[serde(default)]
    pub preload_keys: Vec<String>,
}

fn default_base_url() -> String {
    "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com".to_string()
}

fn default_namespace() -> String {
    "router".to_string()
}

fn default_cache_ttl_secs() -> u64 {
    300 // 5 minutes
}

fn default_retries() -> u32 {
    3
}

fn default_timeout_ms() -> u64 {
    10000 // 10 seconds
}

fn default_preload() -> bool {
    true
}

impl Default for SecretStoreConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_base_url(),
            namespace: default_namespace(),
            cache_ttl_secs: default_cache_ttl_secs(),
            retries: default_retries(),
            timeout_ms: default_timeout_ms(),
            preload: default_preload(),
            preload_keys: vec![],
        }
    }
}
