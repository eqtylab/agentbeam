use serde::{Deserialize, Serialize};
use url::Url;
use iroh_blobs::Hash;

pub const MAX_BEAM_SIZE: u64 = 5_000_000_000;
pub const WARN_THRESHOLD: u64 = 1_000_000_000;
pub const STREAM_BUFFER_SIZE: usize = 8192;
pub const TEMP_DIR_PREFIX: &str = ".agentbeam-";

pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".git/objects/",
    "node_modules/",
    "target/",
    "dist/",
    "build/",
    "*.log",
    ".env*",
    ".agentbeam-*",
];

#[derive(Debug, Clone)]
pub enum ConnectionMode {
    Direct,
    DefaultRelay,
    CustomRelay(Url),
}

impl Default for ConnectionMode {
    fn default() -> Self {
        ConnectionMode::DefaultRelay
    }
}

#[derive(Debug, Clone)]
pub struct BeamConfig {
    pub connection_mode: ConnectionMode,
    pub max_size: u64,
    pub warn_threshold: u64,
    pub force: bool,
    pub test_mode: bool,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            connection_mode: ConnectionMode::default(),
            max_size: MAX_BEAM_SIZE,
            warn_threshold: WARN_THRESHOLD,
            force: false,
            test_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamContent {
    pub metadata_hash: Hash,
    pub collection_hash: Hash,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeamMetadata {
    pub session_id: String,
    pub workspace_name: String,
    pub created_at: u64,
    pub beam_version: String,
    pub total_size: u64,
    pub file_count: usize,
}