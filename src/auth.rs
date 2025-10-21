use axum::http::StatusCode;
use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
    Json,
};

use crate::db::keys::KeyStoreError;
use crate::db::{KeyInfo, KeyStore};

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("missing XJP key")]
    Missing,
    #[error("invalid XJP key")]
    Invalid,
    #[error("key not found")]
    NotFound,
    #[error("key is inactive")]
    Inactive,
    #[error("key has expired")]
    Expired,
    #[error("database error: {0}")]
    Database(String),
}

impl From<KeyStoreError> for AuthError {
    fn from(err: KeyStoreError) -> Self {
        match err {
            KeyStoreError::InvalidFormat => AuthError::Invalid,
            KeyStoreError::NotFound => AuthError::NotFound,
            KeyStoreError::Inactive => AuthError::Inactive,
            KeyStoreError::Expired => AuthError::Expired,
            KeyStoreError::Database(e) => AuthError::Database(e.to_string()),
            KeyStoreError::Internal(e) => AuthError::Database(e),
        }
    }
}

pub fn extract_xjpkey(headers: &HeaderMap) -> Result<String, AuthError> {
    if let Some(bearer) = headers.get(axum::http::header::AUTHORIZATION) {
        if let Ok(s) = bearer.to_str() {
            let pfx = "Bearer ";
            if s.starts_with(pfx) && s[pfx.len()..].starts_with("XJP") {
                return Ok(s[pfx.len()..].into());
            }
        }
    }
    if let Some(key) = headers.get("x-api-key") {
        if let Ok(s) = key.to_str() {
            if s.starts_with("XJP") {
                return Ok(s.into());
            }
        }
    }
    Err(AuthError::Missing)
}

/// Verify an API key using the KeyStore and return KeyInfo
pub async fn verify_key(key_store: &dyn KeyStore, raw_key: &str) -> Result<KeyInfo, AuthError> {
    let key_info = key_store.verify_key(raw_key).await?;

    // Update last_used_at asynchronously (fire and forget)
    let key_id = key_info.id;
    tokio::spawn(async move {
        // Note: we need to clone the key_store or pass it differently
        // For now, this is a placeholder - we'll fix this in the middleware
        tracing::debug!("Key {} used", key_id);
    });

    Ok(key_info)
}

/// Authentication middleware
pub async fn auth_middleware(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::response::Response> {
    use axum::extract::State;

    // Extract headers
    let headers = request.headers().clone();

    // Extract the raw key
    let raw_key = match extract_xjpkey(&headers) {
        Ok(key) => key,
        Err(e) => return Err(e.into_response()),
    };

    // Get KeyStore from extensions (set by the state)
    let key_store = request
        .extensions()
        .get::<std::sync::Arc<dyn KeyStore>>()
        .cloned();

    if let Some(key_store) = key_store {
        // Verify the key
        let key_info = match verify_key(&*key_store, &raw_key).await {
            Ok(info) => info,
            Err(e) => return Err(e.into_response()),
        };

        // Store KeyInfo in request extensions for downstream middleware/handlers
        request.extensions_mut().insert(key_info);
    } else {
        return Err(
            AuthError::Database("KeyStore not found in extensions".to_string()).into_response(),
        );
    }

    Ok(next.run(request).await)
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let code = match self {
            AuthError::Missing => StatusCode::UNAUTHORIZED,
            AuthError::Invalid => StatusCode::UNAUTHORIZED,
            AuthError::NotFound => StatusCode::UNAUTHORIZED,
            AuthError::Inactive => StatusCode::FORBIDDEN,
            AuthError::Expired => StatusCode::UNAUTHORIZED,
            AuthError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = serde_json::json!({
            "error": { "message": self.to_string(), "type": "auth_error" }
        });
        (code, Json(body)).into_response()
    }
}
