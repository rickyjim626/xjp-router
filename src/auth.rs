use axum::{http::HeaderMap, response::{IntoResponse, Response}, Json};
use axum::http::StatusCode;

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("missing XJP key")]
    Missing,
    #[error("invalid XJP key")]
    Invalid,
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

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let code = match self {
            AuthError::Missing => StatusCode::UNAUTHORIZED,
            AuthError::Invalid => StatusCode::UNAUTHORIZED,
        };
        let body = serde_json::json!({
            "error": { "message": self.to_string(), "type": "auth_error" }
        });
        (code, Json(body)).into_response()
    }
}
