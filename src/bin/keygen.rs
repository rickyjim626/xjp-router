use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <tenant_id> [description] [rate_limit_rpm] [rate_limit_rpd]",
            args[0]
        );
        eprintln!(
            "\nExample: {} my-tenant \"Production API Key\" 120 5000",
            args[0]
        );
        std::process::exit(1);
    }

    let tenant_id = args[1].clone();
    let description = args.get(2).cloned();
    let rate_limit_rpm = args.get(3).and_then(|s| s.parse().ok());
    let rate_limit_rpd = args.get(4).and_then(|s| s.parse().ok());

    // Connect to database
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/xjp_gateway".to_string());

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Run migrations
    println!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Create KeyStore instance
    use xjp_gateway::db::{KeyStore, PgKeyStore};
    let key_store: Arc<dyn KeyStore> = Arc::new(PgKeyStore::new(pool));

    // Generate new API key
    println!("\nGenerating new API key...");
    let (key_id, raw_key) = key_store
        .create_key(
            tenant_id.clone(),
            description.clone(),
            rate_limit_rpm,
            rate_limit_rpd,
        )
        .await?;

    // Display results
    println!("\n✅ API Key created successfully!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Key ID:       {}", key_id);
    println!("Tenant ID:    {}", tenant_id);
    if let Some(desc) = description {
        println!("Description:  {}", desc);
    }
    println!(
        "Rate Limits:  {} RPM / {} RPD",
        rate_limit_rpm.unwrap_or(60),
        rate_limit_rpd.unwrap_or(1000)
    );
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("\n🔑 API Key (save this, it will not be shown again):");
    println!("{}", raw_key);
    println!("\n💡 Test with:");
    println!(
        "curl -H \"Authorization: Bearer {}\" http://localhost:8080/healthz",
        raw_key
    );

    Ok(())
}
