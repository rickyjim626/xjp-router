/// Export secrets from xjp-secret-store to various formats
///
/// Usage:
///   cargo run --bin export_secrets -- --namespace router --output .env
///   cargo run --bin export_secrets -- --namespace router --format json --output secrets.json
///   cargo run --bin export_secrets -- --namespace router --format shell > export.sh

use clap::{Parser, ValueEnum};
use secret_store_sdk::{Auth, ClientBuilder, ExportFormat};

#[derive(Parser, Debug)]
#[command(name = "export_secrets")]
#[command(about = "Export secrets from xjp-secret-store", long_about = None)]
struct Args {
    /// Namespace to export from
    #[arg(short, long, default_value = "router")]
    namespace: String,

    /// Export format
    #[arg(short, long, default_value = "dotenv")]
    format: Format,

    /// Output file (stdout if not specified)
    #[arg(short, long)]
    output: Option<String>,

    /// Secret store base URL
    #[arg(long, env = "SECRET_STORE_BASE_URL", default_value = "https://kskxndnvmqwr.sg-members-1.clawcloudrun.com")]
    base_url: String,

    /// API key
    #[arg(long, env = "SECRET_STORE_API_KEY")]
    api_key: String,
}

#[derive(Debug, Clone, ValueEnum)]
enum Format {
    Json,
    Dotenv,
    Shell,
    DockerCompose,
}

impl From<Format> for ExportFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Json => ExportFormat::Json,
            Format::Dotenv => ExportFormat::Dotenv,
            Format::Shell => ExportFormat::Shell,
            Format::DockerCompose => ExportFormat::DockerCompose,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Create client
    let client = ClientBuilder::new(&args.base_url)
        .auth(Auth::api_key(&args.api_key))
        .build()?;

    // Export environment
    let result = client
        .export_env(
            &args.namespace,
            secret_store_sdk::ExportEnvOpts {
                format: args.format.clone().into(),
                use_cache: false,
                if_none_match: None,
            },
        )
        .await?;

    // Get output string
    let output = match result {
        secret_store_sdk::EnvExport::Json(json) => {
            // Manually format JSON since EnvJsonExport might not implement Serialize
            format!(
                "{{\n  \"namespace\": \"{}\",\n  \"environment\": {{\n{}\n  }},\n  \"total\": {}\n}}",
                json.namespace,
                json.environment
                    .iter()
                    .map(|(k, v)| format!("    \"{}\": \"{}\"", k, v.replace("\"", "\\\"")))
                    .collect::<Vec<_>>()
                    .join(",\n"),
                json.total
            )
        }
        secret_store_sdk::EnvExport::Text(text) => text,
    };

    // Write to file or stdout
    if let Some(output_path) = args.output {
        std::fs::write(&output_path, &output)?;
        eprintln!("✓ Exported {} secrets to {}", args.namespace, output_path);
    } else {
        println!("{}", output);
    }

    Ok(())
}
