use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use agentbeam::cli::commands::Cli;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("agentbeam=info,iroh=warn")),
        )
        .init();
    
    let cli = Cli::parse();
    cli.execute().await?;
    
    Ok(())
}
