use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
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
    claude_session::{ClaudeContext, ClaudeSessionInfo, GitContext},
    config::{BeamConfig, BeamMetadata, ConnectionMode, MAX_BEAM_SIZE},
    file_collector::FileCollector,
    provider_monitor::ProviderMonitor,
    receiver::Receiver,
};
use crate::test_utils::dummy::DummyWorkspace;

#[derive(ValueEnum, Clone, Debug, Default)]
pub enum LogFormat {
    #[default]
    Human,
    Json,
}

#[derive(Parser, Debug)]
#[command(name = "agentbeam")]
#[command(about = "P2P workspace and session sharing for Claude Code", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    #[arg(long, value_enum, default_value_t = LogFormat::Human, global = true)]
    pub log_format: LogFormat,
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
        
        #[arg(short = 'y', long, help = "Skip confirmation prompts")]
        yes: bool,
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
                yes,
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
                
                // Log the configured mode for test validation
                let mode_str = match &config.connection_mode {
                    ConnectionMode::Direct => "direct",
                    ConnectionMode::DefaultRelay => "default_relay", 
                    ConnectionMode::CustomRelay(_) => "custom_relay",
                };
                tracing::info!(event = "config_mode", mode = mode_str, role = "sender");
                
                beam_session(config, workspace, yes).await
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
                
                // Log the configured mode for test validation
                let mode_str = match &config.connection_mode {
                    ConnectionMode::Direct => "direct",
                    ConnectionMode::DefaultRelay => "default_relay",
                    ConnectionMode::CustomRelay(_) => "custom_relay",
                };
                tracing::info!(event = "config_mode", mode = mode_str, role = "receiver");
                
                receive_session(ticket, target, config).await
            }
            
            Commands::CleanupTest => {
                cleanup_test_data().await
            }
        }
    }
}

async fn beam_session(config: BeamConfig, workspace_path: Option<PathBuf>, skip_confirm: bool) -> Result<()> {
    let (workspace_dir, session_dir, _guard) = if config.test_mode {
        println!("{} TEST MODE: Using dummy data", "⚠️".yellow());
        let dummy = DummyWorkspace::create(None)?;
        println!("✓ Generated test workspace with {} files",
            std::fs::read_dir(&dummy.workspace_dir)?.count());

        let workspace = dummy.workspace_dir.clone();
        let session = dummy.session_dir.clone();

        (workspace, session, Some(dummy))
    } else {
        let workspace = workspace_path
            .unwrap_or_else(|| PathBuf::from("."))
            .canonicalize()?;
        let session = PathBuf::from(".claude-code-session");
        (workspace, session, None)
    };

    // Ensure .agentbeam-* is in .gitignore
    ensure_gitignore_has_agentbeam_pattern(&workspace_dir)?;

    if !config.test_mode && !skip_confirm {
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
    
    // Detect Claude session and git context
    let claude_context = if config.test_mode {
        // In test mode, create a mock context
        ClaudeContext {
            session: None,
            git_branch: "main".to_string(),
            git_has_changes: false,
            git_remote_url: None,
        }
    } else {
        println!("Detecting Claude session...");
        ClaudeContext::detect(&workspace_dir).await?
    };
    
    // Show Claude session info if found
    if let Some(ref session) = claude_context.session {
        println!("📎 Found Claude Code session ({} entries)", session.entry_count);
        println!("   Branch: {}", claude_context.git_branch);
        if claude_context.git_has_changes {
            println!("   ⚠️  Uncommitted changes present");
        }
    }
    
    // Get user consent if Claude session exists
    if !config.test_mode && !skip_confirm && claude_context.session.is_some() {
        println!();
        println!("{} This will also share your Claude Code conversation history", "📎".cyan());
        print!("Continue with session sharing? (y/N) ");
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
    let mut files = collector.collect_files()?;
    
    // Add Claude session to files if present
    claude_context.add_to_collection(&mut files);
    
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
        claude_session: claude_context.session.as_ref().map(|s| ClaudeSessionInfo {
            original_session_id: s.session_id.clone(),
            project_slug: s.project_slug.clone(),
            entry_count: s.entry_count,
        }),
        git_context: Some(GitContext {
            branch: claude_context.git_branch.clone(),
            has_uncommitted_changes: claude_context.git_has_changes,
            remote_url: claude_context.git_remote_url.clone(),
        }),
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
        iroh_blobs::BlobFormat::HashSeq,
    );
    
    // Log ticket ready for test validation
    tracing::info!(
        event = "ticket_ready",
        ticket = %ticket.to_string(),
        role = "sender"
    );
    
    println!();
    println!("Share this ticket:");
    println!("{}", ticket.to_string().bright_cyan());
    println!();
    
    let mut monitor = ProviderMonitor::new(progress_rx, Some(&mp), &agent_beam.endpoint);
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
    
    // Check for metadata and restore Claude session if present
    let metadata_path = target_dir.join(".agentbeam-metadata.json");
    if metadata_path.exists() {
        let metadata_content = std::fs::read_to_string(&metadata_path)?;
        let metadata: BeamMetadata = serde_json::from_str(&metadata_content)?;
        
        // Display git context if available
        if let Some(git) = &metadata.git_context {
            println!();
            println!("📦 Git context from sender:");
            println!("   Branch: {}", git.branch);
            if git.has_uncommitted_changes {
                println!("   ⚠️  Sender had uncommitted changes");
            }
            if let Some(remote) = &git.remote_url {
                println!("   Remote: {}", remote);
            }
        }
        
        // Restore Claude session if present
        if let Some(claude_info) = &metadata.claude_session {
            println!();
            println!("📎 Restoring Claude Code session...");
            
            let session_source = target_dir.join(".agentbeam/claude-session.jsonl");
            if session_source.exists() {
                ClaudeContext::restore(&target_dir, claude_info, &session_source).await?;
                println!("✓ Claude session restored ({} entries)", claude_info.entry_count);
            } else {
                println!("⚠️  Session file not found in package");
            }
        }
        
        // Initialize git if needed and set branch
        if let Some(git) = &metadata.git_context {
            if !target_dir.join(".git").exists() {
                println!();
                println!("Initializing git repository...");
                
                std::process::Command::new("git")
                    .args(&["init"])
                    .current_dir(&target_dir)
                    .output()?;
                
                // Create matching branch if not main/master
                if git.branch != "main" && git.branch != "master" {
                    std::process::Command::new("git")
                        .args(&["checkout", "-b", &git.branch])
                        .current_dir(&target_dir)
                        .output()?;
                }
                
                println!("✓ Git initialized on branch: {}", git.branch);
            }
        }
    }
    
    agent_beam.shutdown().await?;
    
    Ok(())
}

fn ensure_gitignore_has_agentbeam_pattern(workspace_dir: &PathBuf) -> Result<()> {
    let gitignore_path = workspace_dir.join(".gitignore");
    let pattern = ".agentbeam-*";

    // Read existing gitignore or create empty content
    let mut content = if gitignore_path.exists() {
        std::fs::read_to_string(&gitignore_path)?
    } else {
        String::new()
    };

    // Check if pattern already exists
    let has_pattern = content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == pattern || trimmed == ".agentbeam-*/"
    });

    if !has_pattern {
        // Add the pattern
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }

        // Add a comment if this is the first entry or after existing content
        if content.is_empty() {
            content.push_str("# AgentBeam temporary directories\n");
        } else {
            content.push_str("\n# AgentBeam temporary directories\n");
        }
        content.push_str(pattern);
        content.push('\n');

        // Write back to file
        std::fs::write(&gitignore_path, content)?;
        println!("✓ Added {} to .gitignore", pattern);
    }

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