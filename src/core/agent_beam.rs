use anyhow::{Context, Result};
use iroh::{Endpoint, NodeAddr, Watcher};
use iroh_blobs::{provider::Event, store::fs::FsStore, BlobsProtocol};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;

use crate::core::cleanup::TempDirGuard;
use crate::core::config::{BeamConfig, ConnectionMode, TEMP_DIR_PREFIX};

pub struct AgentBeam {
    pub endpoint: Endpoint,
    pub blobs: BlobsProtocol,
    pub config: BeamConfig,
    store: FsStore,
    temp_dir_guard: Option<TempDirGuard>,
}

impl AgentBeam {
    pub async fn new(config: BeamConfig) -> Result<Self> {
        let temp_dir_name = format!("{}{}", TEMP_DIR_PREFIX, hex::encode(rand::random::<[u8; 8]>()));
        let temp_dir = PathBuf::from(temp_dir_name);
        
        let store = FsStore::load(&temp_dir)
            .await
            .context("Failed to create FsStore")?;
        
        let temp_dir_guard = Some(TempDirGuard::new(temp_dir));
        
        let endpoint_builder = match &config.connection_mode {
            ConnectionMode::Direct => {
                Endpoint::builder().relay_mode(iroh::RelayMode::Disabled)
            }
            ConnectionMode::DefaultRelay => {
                Endpoint::builder()
            }
            ConnectionMode::CustomRelay(url) => {
                let relay_url = iroh::RelayUrl::from(url.clone());
                Endpoint::builder().relay_mode(iroh::RelayMode::Custom(relay_url.into()))
            }
        };
        
        let endpoint = endpoint_builder
            .bind()
            .await
            .context("Failed to bind endpoint")?;
        
        info!("Endpoint created with NodeID: {}", endpoint.node_id());
        
        // Create blobs protocol without progress tracking initially
        // It can be recreated with progress tracking when needed
        let blobs = BlobsProtocol::new(&store, endpoint.clone(), None);
        
        Ok(Self {
            endpoint,
            blobs,
            config,
            store,
            temp_dir_guard,
        })
    }
    
    pub fn node_id(&self) -> iroh::NodeId {
        self.endpoint.node_id()
    }
    
    pub async fn node_addr(&self) -> NodeAddr {
        self.endpoint.node_addr().initialized().await
    }
    
    pub fn blobs_with_progress(&self, progress_tx: mpsc::Sender<Event>) -> BlobsProtocol {
        BlobsProtocol::new(&self.store, self.endpoint.clone(), Some(progress_tx))
    }
    
    pub async fn shutdown(mut self) -> Result<()> {
        info!("Shutting down AgentBeam...");
        
        self.endpoint.close().await;
        
        if let Some(guard) = self.temp_dir_guard.take() {
            drop(guard);
        }
        
        Ok(())
    }
    
    pub fn keep_temp_dir(&mut self) {
        if let Some(ref guard) = self.temp_dir_guard {
            guard.cancel_cleanup();
        }
    }
}