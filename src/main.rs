use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use agentbeam::cli::commands::{Cli, LogFormat};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let log_format = cli.log_format.clone();
    
    // Configure tracing based on log format
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("agentbeam=info,iroh=warn"));
    
    match log_format {
        LogFormat::Human => {
            tracing_subscriber::registry()
                .with(fmt::layer())
                .with(filter)
                .init();
        }
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(fmt::layer().json())
                .with(filter)
                .init();
        }
    }
    
    cli.execute().await?;
    
    Ok(())
}
