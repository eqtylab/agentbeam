use anyhow::Result;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use iroh::{Endpoint, endpoint::ConnectionType, Watcher};
use iroh_blobs::provider::Event;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tracing::{debug, trace};

pub struct ProviderMonitor<'a> {
    receiver: mpsc::Receiver<Event>,
    mp: Option<&'a MultiProgress>,
    endpoint: &'a Endpoint,
}

impl<'a> ProviderMonitor<'a> {
    pub fn new(receiver: mpsc::Receiver<Event>, mp: Option<&'a MultiProgress>, endpoint: &'a Endpoint) -> Self {
        Self { receiver, mp, endpoint }
    }

    pub async fn monitor_until_complete(&mut self) -> Result<()> {
        let mut active_transfers: HashSet<u64> = HashSet::new();
        let mut transfer_bars: HashMap<u64, ProgressBar> = HashMap::new();
        let mut connected = false;

        while let Some(event) = self.receiver.recv().await {
            trace!("Provider event: {:?}", event);
            
            match event {
                Event::ClientConnected {
                    connection_id: _,
                    node_id,
                    permitted,
                } => {
                    println!("{} Peer {} connected", "✓".green(), node_id);
                    
                    // Get actual connection type from endpoint
                    let path = if let Some(mut conn_type_watcher) = self.endpoint.conn_type(node_id) {
                        match conn_type_watcher.get() {
                            ConnectionType::Direct(_) => "direct",
                            ConnectionType::Relay(_) => "relay",
                            ConnectionType::Mixed(_, _) => "mixed",
                            ConnectionType::None => "unknown",
                        }
                    } else {
                        "unknown"
                    };
                    
                    tracing::info!(
                        event = "connection_established",
                        node_id = %node_id,
                        path = path,
                        role = "sender"
                    );
                    
                    permitted.send(true).await.ok();
                    connected = true;
                }
                
                Event::GetRequestReceived { 
                    request_id,
                    hash,
                    ..
                } => {
                    debug!("Get request {} for hash {}", request_id, hash);
                }
                
                Event::TransferStarted {
                    request_id,
                    size,
                    hash,
                    ..
                } => {
                    println!("{} Uploading {} ({} bytes)", 
                        "⬆".blue(), 
                        hash.to_hex().chars().take(8).collect::<String>(),
                        size
                    );
                    
                    active_transfers.insert(request_id);
                    
                    if let Some(ref mp) = self.mp {
                        let pb = mp.add(ProgressBar::new(size));
                        pb.set_style(
                            ProgressStyle::default_bar()
                                .template("[{elapsed_precise}] {bar:40.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec}")
                                .unwrap()
                                .progress_chars("█▉▊▋▌▍▎▏  "),
                        );
                        pb.set_message(format!("Transfer {}", request_id));
                        transfer_bars.insert(request_id, pb);
                    }
                }
                
                Event::TransferProgress {
                    request_id,
                    end_offset,
                    ..
                } => {
                    if let Some(pb) = transfer_bars.get(&request_id) {
                        pb.set_position(end_offset);
                    }
                }
                
                Event::TransferCompleted {
                    request_id,
                    ..
                } => {
                    active_transfers.remove(&request_id);
                    
                    if let Some(pb) = transfer_bars.remove(&request_id) {
                        pb.finish_with_message(format!("✓ Transfer {} complete", request_id));
                    }
                    
                    debug!("Transfer {} completed", request_id);
                    // Don't exit here - wait for ConnectionClosed event
                }
                
                Event::TransferAborted {
                    request_id,
                    ..
                } => {
                    active_transfers.remove(&request_id);
                    
                    if let Some(pb) = transfer_bars.remove(&request_id) {
                        pb.finish_with_message(format!("⚠ Transfer {} aborted", request_id));
                    }
                    
                    println!("{} Transfer {} aborted", "⚠".yellow(), request_id);
                }
                
                Event::ConnectionClosed { .. } => {
                    if connected {
                        println!("{} Connection closed by receiver.", "✓".green());
                        // Give a brief moment for cleanup
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        return Ok(());
                    }
                }
                
                _ => {}
            }
        }
        
        Ok(())
    }
}