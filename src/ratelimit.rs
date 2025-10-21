use axum::{
    body::Body,
    extract::Request,
    http::{HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use dashmap::DashMap;
use governor::{
    clock::{Clock, DefaultClock},
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter as GovernorRateLimiter,
};
use std::{num::NonZeroU32, sync::Arc};
use uuid::Uuid;

/// Per-tenant rate limiter
pub struct RateLimiter {
    limiters: Arc<DashMap<Uuid, Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>>>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            limiters: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a rate limiter for a specific API key
    fn get_or_create_limiter(
        &self,
        key_id: Uuid,
        rpm: u32,
    ) -> Arc<GovernorRateLimiter<NotKeyed, InMemoryState, DefaultClock>> {
        self.limiters
            .entry(key_id)
            .or_insert_with(|| {
                let quota =
                    Quota::per_minute(NonZeroU32::new(rpm).unwrap_or(NonZeroU32::new(60).unwrap()));
                Arc::new(GovernorRateLimiter::direct(quota))
            })
            .clone()
    }

    /// Check if a request is allowed for a specific key
    pub fn check(&self, key_id: Uuid, rpm: u32) -> Result<(), RateLimitError> {
        let limiter = self.get_or_create_limiter(key_id, rpm);
        match limiter.check() {
            Ok(_) => Ok(()),
            Err(not_until) => {
                let wait_time = not_until
                    .wait_time_from(DefaultClock::default().now())
                    .as_secs();
                Err(RateLimitError::Exceeded {
                    retry_after: wait_time,
                })
            }
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limit errors
#[derive(Debug, thiserror::Error)]
pub enum RateLimitError {
    #[error("rate limit exceeded, retry after {retry_after} seconds")]
    Exceeded { retry_after: u64 },
}

impl IntoResponse for RateLimitError {
    fn into_response(self) -> Response {
        match self {
            RateLimitError::Exceeded { retry_after } => {
                let mut response = (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": {
                            "message": format!("Rate limit exceeded. Retry after {} seconds", retry_after),
                            "type": "rate_limit_error",
                            "code": "rate_limit_exceeded"
                        }
                    })),
                )
                    .into_response();

                response.headers_mut().insert(
                    "Retry-After",
                    HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                );
                response.headers_mut().insert(
                    "X-RateLimit-Reset",
                    HeaderValue::from_str(&retry_after.to_string()).unwrap(),
                );

                response
            }
        }
    }
}

/// Middleware to apply rate limiting based on authenticated key
pub async fn rate_limit_middleware(
    request: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    // Extract key_info from request extensions
    // The auth middleware should have already set this
    let key_info = request.extensions().get::<crate::db::KeyInfo>().cloned();

    if let Some(key_info) = key_info {
        // Get rate limiter from request extensions
        let rate_limiter = request.extensions().get::<Arc<RateLimiter>>().cloned();

        if let Some(rate_limiter) = rate_limiter {
            // Check rate limit
            if let Err(e) = rate_limiter.check(key_info.id, key_info.rate_limit_rpm as u32) {
                return Err(e.into_response());
            }
        }
    }

    Ok(next.run(request).await)
}
