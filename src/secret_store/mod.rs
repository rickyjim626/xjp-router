//! Secret Store Integration Module
//!
//! This module provides integration with xjp-secret-store service for centralized
//! secret management. It supports multiple implementations:
//!
//! - `SdkSecretProvider`: Uses the official xjp-secret-store-sdk
//! - `EnvSecretProvider`: Falls back to environment variables
//! - `HybridSecretProvider`: Tries SDK first, falls back to env vars
//!
//! # Example
//!
//! ```no_run
//! use xjp_gateway::secret_store::{SecretProvider, HybridSecretProvider, SdkSecretProvider};
//!
//! async fn example() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create SDK provider
//!     let sdk = SdkSecretProvider::new(
//!         "https://secret-store.example.com",
//!         "api-key",
//!         "namespace",
//!     )?;
//!
//!     // Create hybrid provider with fallback
//!     let provider = HybridSecretProvider::with_sdk(sdk);
//!
//!     // Get a single secret
//!     let api_key = provider.get_secret("providers/openrouter/api-key").await?;
//!
//!     // Batch get multiple secrets (more efficient)
//!     let secrets = provider.get_secrets(&[
//!         "providers/openrouter/api-key",
//!         "providers/vertex/api-key",
//!     ]).await?;
//!
//!     Ok(())
//! }
//! ```

mod config;
mod error;
mod health;
mod provider;

pub use config::SecretStoreConfig;
pub use error::{Result, SecretError};
pub use health::{check_secret_store_health, get_cache_stats, CacheStats, HealthCheckResult};
pub use provider::{
    preload_secrets, EnvSecretProvider, HybridSecretProvider, SdkSecretProvider, SecretProvider,
};
