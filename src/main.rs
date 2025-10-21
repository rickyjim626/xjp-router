mod api;
mod auth;
mod registry;
mod connectors;
mod sse;
mod core;
mod routing;
mod observability;

use axum::{routing::post, Router};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    observability::init_tracing();

    let cfg_path = std::env::var("XJP_CONFIG").unwrap_or_else(|_| "config/xjp.toml".into());
    let registry = registry::load_from_toml(&cfg_path).await?;

    let app_state = routing::AppState::new(registry).await?;

    let app = Router::new()
        .route("/v1/chat/completions", post(api::openai::chat_completions))
        .route("/v1/messages", post(api::anthropic::messages))
        .route("/healthz", axum::routing::get(|| async { "ok" }))
        .with_state(app_state)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let addr = SocketAddr::from(([0,0,0,0], port));
    tracing::info!("XiaojinPro Gateway listening on {addr}");
    axum::Server::bind(&addr).serve(app.into_make_service()).await?;
    Ok(())
}
