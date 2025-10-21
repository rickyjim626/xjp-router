use axum::response::IntoResponse;
use lazy_static::lazy_static;
use prometheus::{
    register_histogram_vec, register_int_counter_vec, register_int_gauge, Encoder, HistogramVec,
    IntCounterVec, IntGauge, TextEncoder,
};

lazy_static! {
    /// Total number of requests processed
    pub static ref REQUESTS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xjp_requests_total",
        "Total number of requests processed",
        &["tenant_id", "logical_model", "provider", "status"]
    )
    .unwrap();

    /// Request duration in seconds
    pub static ref REQUEST_DURATION: HistogramVec = register_histogram_vec!(
        "xjp_request_duration_seconds",
        "Request duration in seconds",
        &["tenant_id", "logical_model", "provider", "stream"],
        vec![0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]
    )
    .unwrap();

    /// Total number of tokens processed
    pub static ref TOKENS_TOTAL: IntCounterVec = register_int_counter_vec!(
        "xjp_tokens_total",
        "Total number of tokens processed",
        &["tenant_id", "logical_model", "provider", "type"]
    )
    .unwrap();

    /// Number of active connections
    pub static ref ACTIVE_CONNECTIONS: IntGauge =
        register_int_gauge!("xjp_active_connections", "Number of active connections").unwrap();

    /// Rate limit hits
    pub static ref RATE_LIMIT_HITS: IntCounterVec = register_int_counter_vec!(
        "xjp_rate_limit_hits_total",
        "Total number of rate limit hits",
        &["tenant_id"]
    )
    .unwrap();

    /// Authentication errors
    pub static ref AUTH_ERRORS: IntCounterVec = register_int_counter_vec!(
        "xjp_auth_errors_total",
        "Total number of authentication errors",
        &["type"]
    )
    .unwrap();
}

/// Export metrics in Prometheus text format
pub fn export_metrics() -> Result<String, Box<dyn std::error::Error>> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer)?;
    Ok(String::from_utf8(buffer)?)
}

/// Metrics handler for /metrics endpoint
pub async fn metrics_handler() -> axum::response::Response {
    match export_metrics() {
        Ok(metrics) => (
            axum::http::StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; version=0.0.4",
            )],
            metrics,
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to export metrics: {}", e),
        )
            .into_response(),
    }
}
