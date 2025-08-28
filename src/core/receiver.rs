use anyhow::{Context, Result};
use colored::Colorize;
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use iroh::{Endpoint, NodeAddr, Watcher};
use iroh::endpoint::ConnectionType;
use iroh_blobs::{
    format::collection::Collection,
    get::request::get_hash_seq_and_sizes,
    api::{remote::GetProgressItem, Store},
    ticket::BlobTicket,
    BlobsProtocol, HashAndFormat,
};
use std::path::Path;
use tokio::sync::mpsc;
use tracing::{debug, info, trace};

use crate::core::file_collector::FileCollector;

pub struct Receiver<'a> {
    endpoint: &'a Endpoint,
    blobs: &'a BlobsProtocol,
    mp: Option<&'a MultiProgress>,
}

impl<'a> Receiver<'a> {
    pub fn new(endpoint: &'a Endpoint, blobs: &'a BlobsProtocol, mp: Option<&'a MultiProgress>) -> Self {
        Self {
            endpoint,
            blobs,
            mp,
        }
    }

    pub async fn receive_from_ticket(
        &self,
        ticket: &BlobTicket,
        target_dir: &Path,
    ) -> Result<()> {
        println!("Connecting to peer...");
        
        // Log that we're attempting to connect
        tracing::info!(
            event = "connecting",
            role = "receiver"
        );
        
        let hash = ticket.hash();
        let node_addr = ticket.node_addr().clone();
        let format = ticket.format();
        let hash_and_format = HashAndFormat::new(hash, format);


        // Resume Support Implementation Note:
        // The specification references `iroh_blobs::get::Options { resume: true }` but this API
        // was removed in iroh-blobs when it moved to its own repository (v0.28.0+).
        // Resume functionality is now built into the blob store layer:
        // - local.is_complete() checks if we already have the complete blob
        // - local.missing() returns only the parts we still need to download
        // - execute_get() automatically downloads only the missing parts
        // This provides automatic resume without needing explicit configuration.
        let local = self.blobs.remote().local(hash_and_format).await?;
        
        if !local.is_complete() {
            let stats = self.download_blob(&node_addr, hash_and_format).await?;
            
            info!("Download complete: {:?}", stats);
            
            // Ensure the blob is fully written before loading the collection
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        } else {
            println!("Collection already available locally");
        }
        
        let collection = Collection::load(hash, self.blobs.store()).await?;
        println!(
            "{} {} files in collection",
            "✓".green(),
            collection.len()
        );
        
        FileCollector::export_collection(self.blobs, collection, target_dir, self.mp).await?;
        
        println!(
            "{} Workspace restored to {}",
            "✓".green(),
            target_dir.display()
        );
        
        Ok(())
    }

    async fn download_blob(
        &self,
        node_addr: &NodeAddr,
        hash_and_format: HashAndFormat,
    ) -> Result<iroh_blobs::get::Stats> {
        let connection = self
            .endpoint
            .connect(node_addr.clone(), iroh_blobs::protocol::ALPN)
            .await
            .context("Failed to connect to peer")?;
        
        // Try to determine actual connection path
        // Check connection type from endpoint
        let path = if let Some(mut conn_type_watcher) = self.endpoint.conn_type(node_addr.node_id) {
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
            node_id = %node_addr.node_id,
            path = path,
            role = "receiver"
        );
        
        let (_hash_seq, sizes) = get_hash_seq_and_sizes(
            &connection,
            &hash_and_format.hash,
            1024 * 1024 * 32,
            None,
        )
        .await
        .context("Failed to get blob info")?;
        
        let total_size = sizes.iter().copied().sum::<u64>();
        let file_count = sizes.len().saturating_sub(1);
        
        println!(
            "Downloading {} files ({} bytes)",
            file_count,
            total_size
        );
        
        let pb = self.mp.map(|mp| {
            let pb = mp.add(ProgressBar::new(total_size));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {percent}% | {bytes}/{total_bytes} | {bytes_per_sec}")
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
            );
            pb.set_message("Downloading");
            pb
        });
        
        let local = self.blobs.remote().local(hash_and_format).await?;
        let local_size = local.local_bytes();
        
        let get = self.blobs.remote().execute_get(connection, local.missing());
        
        let (tx, mut rx) = mpsc::channel::<u64>(32);
        
        let progress_task = if let Some(ref pb) = pb {
            let pb = pb.clone();
            Some(tokio::spawn(async move {
                let mut current = local_size;
                while let Some(offset) = rx.recv().await {
                    current = local_size + offset;
                    pb.set_position(current);
                }
                pb.finish_with_message("✓ Download complete");
            }))
        } else {
            None
        };
        
        let mut stats = iroh_blobs::get::Stats::default();
        let mut stream = get.stream();
        
        while let Some(item) = stream.next().await {
            trace!("Download progress: {:?}", item);
            match item {
                GetProgressItem::Progress(offset) => {
                    tx.send(offset).await.ok();
                }
                GetProgressItem::Done(value) => {
                    stats = value;
                    break;
                }
                GetProgressItem::Error(cause) => {
                    anyhow::bail!("Download error: {:?}", cause);
                }
            }
        }
        
        drop(tx);
        if let Some(task) = progress_task {
            task.await.ok();
        }
        
        Ok(stats)
    }
}