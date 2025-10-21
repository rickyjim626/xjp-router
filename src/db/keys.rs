use base64::Engine as _;
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Information about an API key
#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub id: Uuid,
    pub tenant_id: String,
    pub description: Option<String>,
    pub rate_limit_rpm: i32,
    pub rate_limit_rpd: i32,
    pub is_active: bool,
}

/// Trait for key storage and validation
#[async_trait::async_trait]
pub trait KeyStore: Send + Sync {
    /// Verify a raw API key and return key info if valid
    async fn verify_key(&self, raw_key: &str) -> Result<KeyInfo, KeyStoreError>;

    /// Update last_used_at timestamp for a key
    async fn touch_key(&self, key_id: Uuid) -> Result<(), KeyStoreError>;

    /// Create a new API key
    async fn create_key(
        &self,
        tenant_id: String,
        description: Option<String>,
        rate_limit_rpm: Option<i32>,
        rate_limit_rpd: Option<i32>,
    ) -> Result<(Uuid, String), KeyStoreError>;

    /// Deactivate an API key
    async fn deactivate_key(&self, key_id: Uuid) -> Result<(), KeyStoreError>;
}

/// Errors that can occur during key operations
#[derive(Debug, thiserror::Error)]
pub enum KeyStoreError {
    #[error("Invalid key format")]
    InvalidFormat,

    #[error("Key not found")]
    NotFound,

    #[error("Key is inactive")]
    Inactive,

    #[error("Key has expired")]
    Expired,

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// PostgreSQL implementation of KeyStore
pub struct PgKeyStore {
    pool: PgPool,
}

impl PgKeyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Hash a raw key for storage/comparison
    fn hash_key(raw_key: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(raw_key.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Generate a new API key with XJP prefix
    fn generate_key() -> String {
        use rand::Rng;
        let random_bytes: Vec<u8> = (0..32).map(|_| rand::thread_rng().gen()).collect();
        let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&random_bytes);
        format!("XJP_{}", encoded)
    }
}

#[async_trait::async_trait]
impl KeyStore for PgKeyStore {
    async fn verify_key(&self, raw_key: &str) -> Result<KeyInfo, KeyStoreError> {
        // Check format
        if !raw_key.starts_with("XJP_") {
            return Err(KeyStoreError::InvalidFormat);
        }

        let key_hash = Self::hash_key(raw_key);

        // Query database
        let row = sqlx::query(
            r#"
            SELECT id, tenant_id, description, rate_limit_rpm, rate_limit_rpd,
                   is_active, expires_at
            FROM api_keys
            WHERE key_hash = $1
            "#,
        )
        .bind(&key_hash)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(KeyStoreError::NotFound)?;

        // Check if active
        let is_active: bool = row.get("is_active");
        if !is_active {
            return Err(KeyStoreError::Inactive);
        }

        // Check expiration
        if let Some(expires_at) = row.try_get::<Option<time::OffsetDateTime>, _>("expires_at")? {
            if expires_at < time::OffsetDateTime::now_utc() {
                return Err(KeyStoreError::Expired);
            }
        }

        Ok(KeyInfo {
            id: row.get("id"),
            tenant_id: row.get("tenant_id"),
            description: row.get("description"),
            rate_limit_rpm: row.get("rate_limit_rpm"),
            rate_limit_rpd: row.get("rate_limit_rpd"),
            is_active,
        })
    }

    async fn touch_key(&self, key_id: Uuid) -> Result<(), KeyStoreError> {
        sqlx::query(
            r#"
            UPDATE api_keys
            SET last_used_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(key_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn create_key(
        &self,
        tenant_id: String,
        description: Option<String>,
        rate_limit_rpm: Option<i32>,
        rate_limit_rpd: Option<i32>,
    ) -> Result<(Uuid, String), KeyStoreError> {
        let raw_key = Self::generate_key();
        let key_hash = Self::hash_key(&raw_key);

        let row = sqlx::query(
            r#"
            INSERT INTO api_keys (key_hash, tenant_id, description, rate_limit_rpm, rate_limit_rpd)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
        )
        .bind(&key_hash)
        .bind(&tenant_id)
        .bind(&description)
        .bind(rate_limit_rpm.unwrap_or(60))
        .bind(rate_limit_rpd.unwrap_or(1000))
        .fetch_one(&self.pool)
        .await?;

        let id: Uuid = row.get("id");
        Ok((id, raw_key))
    }

    async fn deactivate_key(&self, key_id: Uuid) -> Result<(), KeyStoreError> {
        sqlx::query(
            r#"
            UPDATE api_keys
            SET is_active = false
            WHERE id = $1
            "#,
        )
        .bind(key_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
