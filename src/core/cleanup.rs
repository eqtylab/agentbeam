use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::warn;

pub struct TempDirGuard {
    path: PathBuf,
    should_cleanup: Arc<AtomicBool>,
}

impl TempDirGuard {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            should_cleanup: Arc::new(AtomicBool::new(true)),
        }
    }
    
    pub fn cancel_cleanup(&self) {
        self.should_cleanup.store(false, Ordering::SeqCst);
    }
    
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if !self.should_cleanup.load(Ordering::SeqCst) {
            return;
        }
        
        if std::thread::panicking() {
            warn!("Cleanup may be incomplete due to panic");
        }
        
        if let Err(e) = std::fs::remove_dir_all(&self.path) {
            if self.path.exists() {
                warn!("Failed to cleanup temporary directory {}: {}", self.path.display(), e);
            }
        }
    }
}