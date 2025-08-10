use anyhow::Result;
use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use iroh_blobs::provider::Event;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tracing::{debug, error, trace};

pub struct ProviderMonitor<'a> {
    receiver: mpsc::Receiver<Event>,
    mp: Option<&'a MultiProgress>,
}

impl<'a> ProviderMonitor<'a> {
    pub fn new(receiver: mpsc::Receiver<Event>, mp: Option<&'a MultiProgress>) -> Self {
        Self { receiver, mp }
    }

    pub async fn monitor_until_complete(&mut self) -> Result<()> {
        let mut active_transfers: HashSet<u64> = HashSet::new();
        let mut transfer_bars: HashMap<u64, ProgressBar> = HashMap::new();
        let mut connected = false;

        while let Some(event) = self.receiver.recv().await {
            trace!("Provider event: {:?}", event);
            
            match event {
                Event::ClientConnected {
                    node_id,
                    permitted,
                    ..
                } => {
                    println!("{} Peer {} connected", "✓".green(), node_id);
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
                    
                    if active_transfers.is_empty() && connected {
                        println!(
                            "{} All transfers complete. Safe to close.",
                            "✓".green()
                        );
                        return Ok(());
                    }
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
                    if active_transfers.is_empty() {
                        println!("{} Connection closed. Transfer complete.", "✓".green());
                        return Ok(());
                    }
                }
                
                _ => {}
            }
        }
        
        Ok(())
    }
}