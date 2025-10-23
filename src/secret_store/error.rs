/// Errors related to secret store operations
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    /// Secret not found
    #[error("Secret not found: {0}")]
    NotFound(String),

    /// SDK error
    #[error("SDK error: {0}")]
    Sdk(#[from] secret_store_sdk::Error),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Unexpected format error
    #[error("Unexpected response format")]
    UnexpectedFormat,

    /// Network error
    #[error("Network error: {0}")]
    Network(String),

    /// Timeout error
    #[error("Request timeout")]
    Timeout,

    /// Other errors
    #[error("Other error: {0}")]
    Other(String),
}

impl From<std::env::VarError> for SecretError {
    fn from(e: std::env::VarError) -> Self {
        Self::Config(format!("Environment variable error: {}", e))
    }
}

pub type Result<T> = std::result::Result<T, SecretError>;
