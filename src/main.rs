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
mod sse;

use axum::{routing::post, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing();

    // Load configuration
    let cfg_path = std::env::var("XJP_CONFIG").unwrap_or_else(|_| "config/xjp.toml".into());
    let registry = registry::load_from_toml(&cfg_path).await?;

    // Initialize database connection pool
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/xjp_gateway".to_string());

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

    let app_state = routing::AppState::new(registry, key_store).await?;

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
