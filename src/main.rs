mod api;
mod auth;
mod connectors;
mod core;
mod db;
mod metrics;
mod observability;
mod ratelimit;
mod registry;
mod routing;
mod secret_store;
mod sse;

use axum::{routing::post, Router};
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use secret_store::{
    preload_secrets, EnvSecretProvider, HybridSecretProvider, SdkSecretProvider, SecretProvider,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing();

    // Load configuration
    let cfg_path = std::env::var("XJP_CONFIG").unwrap_or_else(|_| "config/xjp.toml".into());
    let registry = registry::load_from_toml(&cfg_path).await?;

    // Initialize SecretProvider
    tracing::info!("Initializing secret provider...");
    let secret_provider: Arc<dyn SecretProvider> =
        if registry.secret_store_config.enabled {
            // Get API key from environment
            let api_key = std::env::var("SECRET_STORE_API_KEY")
                .expect("SECRET_STORE_API_KEY must be set when secret_store.enabled=true");

            tracing::info!(
                "Secret store enabled, connecting to: {}",
                registry.secret_store_config.base_url
            );

            // Create SDK provider
            match SdkSecretProvider::with_options(
                &registry.secret_store_config.base_url,
                &api_key,
                &registry.secret_store_config.namespace,
                registry.secret_store_config.cache_ttl_secs,
                registry.secret_store_config.retries,
                registry.secret_store_config.timeout_ms,
            ) {
                Ok(sdk_provider) => {
                    tracing::info!("SDK provider initialized successfully");
                    Arc::new(HybridSecretProvider::with_sdk(sdk_provider))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize SDK provider: {}, falling back to env-only", e);
                    Arc::new(HybridSecretProvider::env_only())
                }
            }
        } else {
            tracing::info!("Secret store disabled, using environment variables only");
            Arc::new(EnvSecretProvider::new())
        };

    // Preload secrets if configured
    let preloaded_secrets: HashMap<String, String> = if registry.secret_store_config.preload {
        tracing::info!(
            "Preloading {} secrets...",
            registry.secret_store_config.preload_keys.len()
        );
        preload_secrets(
            secret_provider.as_ref(),
            &registry.secret_store_config.preload_keys,
        )
        .await
    } else {
        HashMap::new()
    };

    // Get database URL from preloaded secrets or secret provider or environment
    let database_url = if let Some(url) = preloaded_secrets.get("infrastructure/database-url") {
        tracing::info!("Using database URL from preloaded secrets");
        url.clone()
    } else {
        match secret_provider
            .get_secret("infrastructure/database-url")
            .await
        {
            Ok(url) => {
                tracing::info!("Using database URL from secret provider");
                url
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to get database URL from secret provider: {}, falling back to env",
                    e
                );
                std::env::var("DATABASE_URL").unwrap_or_else(|_| {
                    "postgres://postgres:postgres@localhost:5432/xjp_gateway".to_string()
                })
            }
        }
    };

    tracing::info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await?;

    // Run migrations
    tracing::info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Create KeyStore instance
    let key_store: Arc<dyn db::KeyStore> = Arc::new(db::PgKeyStore::new(pool.clone()));

    let app_state = routing::AppState::new(
        registry,
        key_store,
        secret_provider,
        preloaded_secrets,
    )
    .await?;

    let app = Router::new()
        .route("/v1/chat/completions", post(api::openai::chat_completions))
        .route("/v1/messages", post(api::anthropic::messages))
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .route("/metrics", axum::routing::get(metrics::metrics_handler))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("XiaojinPro Gateway listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
