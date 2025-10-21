use tracing_subscriber::EnvFilter;

pub fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "info,axum=info,reqwest=warn".into());
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new(filter))
        .with_target(false)
        .compact()
        .init();
}
