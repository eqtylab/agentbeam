use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::MultiProgress;
use iroh::Watcher;
use iroh_blobs::{ticket::BlobTicket, BlobsProtocol};
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;
use tokio::sync::mpsc;
use url::Url;

use crate::core::{
    agent_beam::AgentBeam,
    config::{BeamConfig, BeamMetadata, ConnectionMode, MAX_BEAM_SIZE},
    file_collector::FileCollector,
    provider_monitor::ProviderMonitor,
    receiver::Receiver,
};
use crate::test_utils::dummy::DummyWorkspace;

#[derive(Parser, Debug)]
#[command(name = "agentbeam")]
#[command(about = "P2P workspace and session sharing for Claude Code", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    #[command(about = "Share your current workspace and Claude Code session")]
    BeamSession {
        #[arg(long, help = "Run in test mode with dummy data")]
        test_mode: bool,
        
        #[arg(long, help = "Force sharing even if size exceeds limits")]
        force: bool,
        
        #[arg(long, help = "Disable relay, use direct P2P only")]
        no_relay: bool,
        
        #[arg(long, help = "Use a custom relay URL")]
        relay_url: Option<Url>,
        
        #[arg(long, help = "Path to workspace (defaults to current directory)")]
        workspace: Option<PathBuf>,
    },
    
    #[command(about = "Receive a shared workspace from a ticket")]
    Receive {
        #[arg(help = "The sharing ticket from the sender")]
        ticket: String,
        
        #[arg(long, help = "Target directory for extraction", default_value = "./beamed-workspace")]
        target: PathBuf,
        
        #[arg(long, help = "Disable relay, use direct P2P only")]
        no_relay: bool,
        
        #[arg(long, help = "Use a custom relay URL")]
        relay_url: Option<Url>,
    },
    
    #[command(about = "Clean up test data")]
    CleanupTest,
}

impl Cli {
    pub async fn execute(self) -> Result<()> {
        match self.command {
            Commands::BeamSession {
                test_mode,
                force,
                no_relay,
                relay_url,
                workspace,
            } => {
                let config = BeamConfig {
                    connection_mode: if no_relay {
                        ConnectionMode::Direct
                    } else if let Some(url) = relay_url {
                        ConnectionMode::CustomRelay(url)
                    } else {
                        ConnectionMode::DefaultRelay
                    },
                    max_size: MAX_BEAM_SIZE,
                    warn_threshold: crate::core::config::WARN_THRESHOLD,
                    force,
                    test_mode,
                };
                
                beam_session(config, workspace).await
            }
            
            Commands::Receive {
                ticket,
                target,
                no_relay,
                relay_url,
            } => {
                let config = BeamConfig {
                    connection_mode: if no_relay {
                        ConnectionMode::Direct
                    } else if let Some(url) = relay_url {
                        ConnectionMode::CustomRelay(url)
                    } else {
                        ConnectionMode::DefaultRelay
                    },
                    ..Default::default()
                };
                
                receive_session(ticket, target, config).await
            }
            
            Commands::CleanupTest => {
                cleanup_test_data().await
            }
        }
    }
}

async fn beam_session(config: BeamConfig, workspace_path: Option<PathBuf>) -> Result<()> {
    let (workspace_dir, session_dir, _guard) = if config.test_mode {
        println!("{} TEST MODE: Using dummy data", "⚠️".yellow());
        let dummy = DummyWorkspace::create(None)?;
        println!("✓ Generated test workspace with {} files", 
            std::fs::read_dir(&dummy.workspace_dir)?.count());
        
        let workspace = dummy.workspace_dir.clone();
        let session = dummy.session_dir.clone();
        
        (workspace, session, Some(dummy))
    } else {
        let workspace = workspace_path.unwrap_or_else(|| PathBuf::from("."));
        let session = PathBuf::from(".claude-code-session");
        (workspace, session, None)
    };
    
    if !config.test_mode {
        println!("{} This will share:", "⚠️".yellow());
        println!("  - Your entire workspace (respecting .gitignore/.beamignore)");
        println!("  - Claude Code conversation history");
        println!("  - Your IP address with the recipient");
        println!();
        print!("Continue? (y/N) ");
        use std::io::{self, Write};
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }
    
    let agent_beam = AgentBeam::new(config.clone()).await?;
    
    let mp = MultiProgress::new();
    
    let collector = FileCollector::new(workspace_dir.clone());
    let files = collector.collect_files()?;
    println!("Packaging workspace ({} files)...", files.len());
    
    let metadata = BeamMetadata {
        session_id: format!("session-{}", hex::encode(rand::random::<[u8; 8]>())),
        workspace_name: workspace_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("workspace")
            .to_string(),
        created_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs(),
        beam_version: env!("CARGO_PKG_VERSION").to_string(),
        total_size: 0,
        file_count: files.len(),
    };
    
    let (collection_tag, total_size, _collection) = collector
        .create_collection(&agent_beam.blobs, files, metadata, Some(&mp))
        .await?;
    
    if total_size > config.max_size && !config.force {
        anyhow::bail!(
            "Workspace too large: {:.2}GB (limit: {:.2}GB)\nUse --force to override",
            total_size as f64 / 1_000_000_000.0,
            config.max_size as f64 / 1_000_000_000.0
        );
    }
    
    let (progress_tx, progress_rx) = mpsc::channel(32);
    let blobs_with_progress = agent_beam.blobs_with_progress(progress_tx);
    
    // Set up router to accept connections
    let router = iroh::protocol::Router::builder(agent_beam.endpoint.clone())
        .accept(iroh_blobs::ALPN, blobs_with_progress)
        .spawn();
    
    // Wait for endpoint to initialize
    let _ = router.endpoint().home_relay().initialized().await;
    
    let node_addr = agent_beam.node_addr().await;
    let ticket = BlobTicket::new(
        node_addr,
        *collection_tag.hash(),
        iroh_blobs::BlobFormat::Raw,
    );
    
    println!();
    println!("Share this ticket:");
    println!("{}", ticket.to_string().bright_cyan());
    println!();
    
    let mut monitor = ProviderMonitor::new(progress_rx, Some(&mp));
    monitor.monitor_until_complete().await?;
    
    agent_beam.shutdown().await?;
    
    Ok(())
}

async fn receive_session(ticket_str: String, target_dir: PathBuf, config: BeamConfig) -> Result<()> {
    let ticket = BlobTicket::from_str(&ticket_str)
        .context("Invalid ticket format")?;
    
    let agent_beam = AgentBeam::new(config).await?;
    
    let mp = MultiProgress::new();
    
    let receiver = Receiver::new(&agent_beam.endpoint, &agent_beam.blobs, Some(&mp));
    receiver.receive_from_ticket(&ticket, &target_dir).await?;
    
    let file_count = std::fs::read_dir(&target_dir)?.count();
    println!("{} {} files extracted", "✓".green(), file_count);
    
    agent_beam.shutdown().await?;
    
    Ok(())
}

async fn cleanup_test_data() -> Result<()> {
    let test_dir = PathBuf::from(".agentbeam-test");
    if test_dir.exists() {
        std::fs::remove_dir_all(&test_dir)?;
        println!("{} Removed .agentbeam-test directory", "✓".green());
    } else {
        println!("No test directory found");
    }
    Ok(())
}