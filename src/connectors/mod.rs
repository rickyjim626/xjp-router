use futures_util::stream::BoxStream;
use thiserror::Error;

pub mod openrouter;
pub mod vertex;
pub mod clewdr;

use crate::registry::EgressRoute;
use crate::core::entities::{UnifiedChunk, UnifiedRequest};

#[derive(Clone, Debug)]
pub struct ConnectorCapabilities {
    pub text: bool,
    pub vision: bool,
    pub video: bool,
    pub tools: bool,
    pub stream: bool,
}

#[async_trait::async_trait]
pub trait Connector: Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> ConnectorCapabilities;
    async fn invoke(&self, route: &EgressRoute, req: UnifiedRequest) -> Result<ConnectorResponse, ConnectorError>;
}

pub enum ConnectorResponse {
    Streaming(BoxStream<'static, Result<UnifiedChunk, ConnectorError>>),
    NonStreaming(UnifiedChunk),
}

#[derive(Error, Debug)]
pub enum ConnectorError {
    #[error("auth error: {0}")]
    Auth(String),
    #[error("rate_limited")]
    RateLimited,
    #[error("upstream_timeout")]
    Timeout,
    #[error("upstream_error: {0}")]
    Upstream(String),
    #[error("invalid_request: {0}")]
    Invalid(String),
    #[error("internal: {0}")]
    Internal(String),
}

impl From<reqwest::Error> for ConnectorError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_timeout() { ConnectorError::Timeout } else { ConnectorError::Upstream(e.to_string()) }
    }
}

impl From<anyhow::Error> for ConnectorError {
    fn from(e: anyhow::Error) -> Self { ConnectorError::Internal(e.to_string()) }
}

impl axum::response::IntoResponse for ConnectorError {
    fn into_response(self) -> axum::response::Response {
        use axum::{http::StatusCode, Json};
        let (code, msg) = match &self {
            ConnectorError::Auth(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            ConnectorError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            ConnectorError::Timeout => (StatusCode::GATEWAY_TIMEOUT, self.to_string()),
            ConnectorError::Upstream(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            ConnectorError::Invalid(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ConnectorError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };
        let body = serde_json::json!({
            "error": {
                "message": msg,
                "type": "xjp_error"
            }
        });
        (code, Json(body)).into_response()
    }
}
