use anyhow::Result;
use std::fs::{self, create_dir_all};
use std::path::{Path, PathBuf};

pub const SAMPLE_GITIGNORE: &str = r#"# Build artifacts
/target
/dist
/build

# Dependencies
node_modules/

# Environment files
.env
.env.*

# IDE
.vscode/
.idea/

# OS
.DS_Store
Thumbs.db

# Temporary
*.tmp
*.log
.agentbeam-*
"#;

pub const SAMPLE_BEAMIGNORE: &str = r#"# Additional ignores for beaming
*.secret
*.key
credentials.json
"#;

pub const SAMPLE_MAIN_RS: &str = r#"use std::collections::HashMap;

fn main() {
    println!("Hello from AgentBeam test workspace!");
    
    let mut data = HashMap::new();
    data.insert("version", "0.1.0");
    data.insert("status", "testing");
    
    for (key, value) in &data {
        println!("{}: {}", key, value);
    }
}
"#;

pub const SAMPLE_LIB_RS: &str = r#"pub mod utils;

pub fn process_data(input: &str) -> String {
    format!("Processed: {}", input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_data() {
        assert_eq!(process_data("test"), "Processed: test");
    }
}
"#;

pub const SAMPLE_UTILS_RS: &str = r#"pub fn helper_function(x: i32, y: i32) -> i32 {
    x + y
}

pub fn format_output(msg: &str) -> String {
    format!("[{}] {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg)
}
"#;

pub const SAMPLE_CARGO_TOML: &str = r#"[package]
name = "test-workspace"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
"#;

pub const SAMPLE_README: &str = r#"# Test Workspace

This is a dummy workspace created for testing AgentBeam P2P transfers.

## Features
- Sample Rust project structure
- Various file types and sizes
- Gitignore patterns for testing

## Testing
This workspace is used to validate:
- File transfer integrity
- Ignore pattern handling
- Collection-based bundling
- Memory-safe streaming
"#;

pub const SAMPLE_CLAUDE_MD: &str = r#"# Project Context

This is a dummy Claude Code session for testing AgentBeam.

## Current Tasks
- Implement P2P file transfer
- Test ignore patterns
- Validate collection handling

## Important Notes
- Using Iroh for P2P networking
- Collections instead of TAR archives
- Memory-safe streaming operations

## Session Information
- Session ID: test-session-12345
- Created: 2024-01-01
- Last Updated: 2024-01-02
"#;

pub const SAMPLE_CONVERSATION: &str = r#"{
  "messages": [
    {
      "role": "user",
      "content": "Help me set up a P2P file transfer system"
    },
    {
      "role": "assistant",
      "content": "I'll help you build a P2P file transfer system using Iroh..."
    }
  ],
  "session_id": "test-session-12345",
  "created_at": "2024-01-01T10:00:00Z"
}"#;

pub const SAMPLE_WORKSPACE_STATE: &str = r#"{
  "workspace_path": "/test/workspace",
  "files_modified": 42,
  "last_sync": "2024-01-02T15:30:00Z",
  "session_active": true
}"#;

pub struct DummyWorkspace {
    pub root: PathBuf,
    pub workspace_dir: PathBuf,
    pub session_dir: PathBuf,
}

impl DummyWorkspace {
    pub fn create(base_path: Option<&Path>) -> Result<Self> {
        let root = match base_path {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir()?.join(".agentbeam-test"),
        };
        
        let workspace_dir = root.join("dummy-workspace");
        let session_dir = root.join("dummy-session");
        
        let dummy = Self {
            root,
            workspace_dir,
            session_dir,
        };
        
        dummy.setup_workspace()?;
        dummy.setup_session()?;
        
        Ok(dummy)
    }
    
    fn setup_workspace(&self) -> Result<()> {
        create_dir_all(&self.workspace_dir.join("src/utils"))?;
        create_dir_all(&self.workspace_dir.join("tests"))?;
        create_dir_all(&self.workspace_dir.join(".git"))?;
        create_dir_all(&self.workspace_dir.join("target/debug/build"))?;
        
        fs::write(self.workspace_dir.join("src/main.rs"), SAMPLE_MAIN_RS)?;
        fs::write(self.workspace_dir.join("src/lib.rs"), SAMPLE_LIB_RS)?;
        fs::write(self.workspace_dir.join("src/utils/helpers.rs"), SAMPLE_UTILS_RS)?;
        fs::write(self.workspace_dir.join("Cargo.toml"), SAMPLE_CARGO_TOML)?;
        fs::write(self.workspace_dir.join("README.md"), SAMPLE_README)?;
        fs::write(self.workspace_dir.join(".gitignore"), SAMPLE_GITIGNORE)?;
        fs::write(self.workspace_dir.join(".beamignore"), SAMPLE_BEAMIGNORE)?;
        
        fs::write(self.workspace_dir.join(".env"), "SECRET_KEY=should_be_ignored")?;
        fs::write(self.workspace_dir.join(".env.local"), "API_KEY=also_ignored")?;
        
        fs::write(self.workspace_dir.join(".git/config"), "[core]\nrepositoryformatversion = 0")?;
        
        self.create_dummy_node_modules(100)?;
        
        self.create_dummy_build_artifacts()?;
        
        Ok(())
    }
    
    fn setup_session(&self) -> Result<()> {
        create_dir_all(&self.session_dir)?;
        
        fs::write(self.session_dir.join("CLAUDE.md"), SAMPLE_CLAUDE_MD)?;
        fs::write(self.session_dir.join("conversation.json"), SAMPLE_CONVERSATION)?;
        fs::write(self.session_dir.join("workspace-state.json"), SAMPLE_WORKSPACE_STATE)?;
        
        Ok(())
    }
    
    fn create_dummy_node_modules(&self, file_count: usize) -> Result<()> {
        let node_modules = self.workspace_dir.join("node_modules");
        create_dir_all(&node_modules.join("package1/lib"))?;
        create_dir_all(&node_modules.join("package2/dist"))?;
        create_dir_all(&node_modules.join("@scope/package3/src"))?;
        
        for i in 0..file_count {
            let content = format!("// Dummy file {} for size testing\nmodule.exports = {{}};", i);
            let path = if i % 3 == 0 {
                node_modules.join(format!("package1/lib/file{}.js", i))
            } else if i % 3 == 1 {
                node_modules.join(format!("package2/dist/bundle{}.js", i))
            } else {
                node_modules.join(format!("@scope/package3/src/component{}.js", i))
            };
            fs::write(path, content)?;
        }
        
        Ok(())
    }
    
    fn create_dummy_build_artifacts(&self) -> Result<()> {
        let target = self.workspace_dir.join("target/debug");
        
        let binary_content = vec![0u8; 1024 * 100]; 
        fs::write(target.join("test-workspace"), binary_content.clone())?;
        fs::write(target.join("test-workspace.d"), "src/main.rs: src/lib.rs")?;
        
        for i in 0..5 {
            fs::write(
                target.join(format!("build/lib{}.rlib", i)),
                binary_content.clone()
            )?;
        }
        
        Ok(())
    }
    
    pub fn create_large_files(&self, total_size_mb: usize) -> Result<()> {
        let large_dir = self.workspace_dir.join("large_files");
        create_dir_all(&large_dir)?;
        
        let file_size = 10 * 1024 * 1024; 
        let file_count = total_size_mb / 10;
        
        for i in 0..file_count {
            let content = vec![b'X'; file_size];
            fs::write(large_dir.join(format!("large_{}.dat", i)), content)?;
        }
        
        Ok(())
    }
    
    pub fn cleanup(&self) -> Result<()> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
        }
        Ok(())
    }
}

impl Drop for DummyWorkspace {
    fn drop(&mut self) {
        if std::env::var("KEEP_TEST_FILES").is_err() {
            let _ = self.cleanup();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_dummy_workspace_creation() {
        let temp_dir = TempDir::new().unwrap();
        let dummy = DummyWorkspace::create(Some(temp_dir.path())).unwrap();
        
        assert!(dummy.workspace_dir.join("src/main.rs").exists());
        assert!(dummy.workspace_dir.join(".gitignore").exists());
        assert!(dummy.workspace_dir.join("node_modules").exists());
        assert!(dummy.session_dir.join("CLAUDE.md").exists());
    }
    
    #[test]
    fn test_large_files_generation() {
        let temp_dir = TempDir::new().unwrap();
        let dummy = DummyWorkspace::create(Some(temp_dir.path())).unwrap();
        
        dummy.create_large_files(50).unwrap();
        
        let large_files_dir = dummy.workspace_dir.join("large_files");
        assert!(large_files_dir.exists());
        
        let entries: Vec<_> = fs::read_dir(large_files_dir).unwrap().collect();
        assert_eq!(entries.len(), 5); 
    }
}