use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;
use tracing::{debug, info, trace};

#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub session_file: PathBuf,
    pub session_id: String,
    pub project_slug: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone)]
pub struct ClaudeContext {
    pub session: Option<ClaudeSession>,
    pub git_branch: String,
    pub git_has_changes: bool,
    pub git_remote_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeSessionInfo {
    pub original_session_id: String,
    pub project_slug: String,
    pub entry_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitContext {
    pub branch: String,
    pub has_uncommitted_changes: bool,
    pub remote_url: Option<String>,
}

impl ClaudeContext {
    /// Detect Claude session and git context for a workspace
    pub async fn detect(workspace: &Path) -> Result<Self> {
        debug!("Detecting Claude context for: {}", workspace.display());
        
        // Get git context
        let (git_branch, git_has_changes, git_remote_url) = Self::get_git_state(workspace)?;
        
        // Detect Claude session
        let session = Self::detect_session(workspace)?;
        
        if let Some(ref s) = session {
            info!("Found Claude session: {} ({} entries)", s.session_id, s.entry_count);
        } else {
            debug!("No Claude session found for workspace");
        }
        
        Ok(Self {
            session,
            git_branch,
            git_has_changes,
            git_remote_url,
        })
    }
    
    /// Add Claude session file to the collection files list
    pub fn add_to_collection(&self, files: &mut Vec<(String, PathBuf)>) {
        if let Some(ref session) = self.session {
            files.push((
                ".agentbeam/claude-session.jsonl".to_string(),
                session.session_file.clone(),
            ));
        }
    }
    
    /// Restore Claude session on the receiver side
    pub async fn restore(
        target_dir: &Path,
        claude_info: &ClaudeSessionInfo,
        session_source: &Path,
    ) -> Result<()> {
        info!("Restoring Claude session for receiver");
        
        // Generate project slug for receiver's absolute path
        let abs_target = if target_dir.is_absolute() {
            target_dir.to_path_buf()
        } else {
            std::env::current_dir()?.join(target_dir)
        };
        let receiver_slug = Self::path_to_slug(&abs_target);
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let claude_project_dir = home.join(".claude/projects").join(&receiver_slug);
        
        // Create directory if needed
        fs::create_dir_all(&claude_project_dir)?;
        
        // Check if the original session ID already exists
        let original_session_file = claude_project_dir.join(format!("{}.jsonl", claude_info.original_session_id));
        
        let (new_session_id, session_dest) = if original_session_file.exists() {
            // Collision detected - append -agentbeam to avoid overwriting
            println!("⚠️  Session ID {} already exists locally", claude_info.original_session_id);
            println!("   Creating separate copy with -agentbeam suffix");
            
            let new_id = format!("{}-agentbeam", claude_info.original_session_id);
            let dest = claude_project_dir.join(format!("{}.jsonl", new_id));
            (new_id, dest)
        } else {
            // Safe to use a new UUID
            let new_id = uuid::Uuid::new_v4().to_string();
            let dest = claude_project_dir.join(format!("{}.jsonl", new_id));
            (new_id, dest)
        };
        
        // Copy session with updated IDs
        Self::copy_session_with_new_id(session_source, &session_dest, &new_session_id).await?;
        
        info!(
            "Claude session restored to: ~/.claude/projects/{}/{}.jsonl",
            receiver_slug, new_session_id
        );
        
        println!("   Session path: ~/.claude/projects/{}/{}.jsonl", 
            receiver_slug, new_session_id);
        
        Ok(())
    }
    
    /// Detect Claude session for a workspace
    fn detect_session(workspace: &Path) -> Result<Option<ClaudeSession>> {
        let slug = Self::path_to_slug(workspace);
        let home = dirs::home_dir().context("Failed to get home directory")?;
        let claude_dir = home.join(".claude/projects").join(&slug);
        
        if !claude_dir.exists() {
            trace!("Claude project directory does not exist: {}", claude_dir.display());
            return Ok(None);
        }
        
        let session_file = Self::find_latest_session(&claude_dir)?;
        
        match session_file {
            Some(path) => {
                let session_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                
                // Count entries in the session file
                let content = fs::read_to_string(&path)?;
                let entry_count = content.lines().count();
                
                Ok(Some(ClaudeSession {
                    session_file: path,
                    session_id,
                    project_slug: slug,
                    entry_count,
                }))
            }
            None => {
                trace!("No session files found in Claude project directory");
                Ok(None)
            }
        }
    }
    
    /// Convert a file path to Claude's project slug format
    pub fn path_to_slug(path: &Path) -> String {
        path.to_string_lossy()
            .chars()
            .map(|c| if c == '/' { '-' } else { c })
            .collect()
    }
    
    /// Find the most recently modified session file in a directory
    fn find_latest_session(claude_dir: &Path) -> Result<Option<PathBuf>> {
        let mut sessions: Vec<_> = fs::read_dir(claude_dir)?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path()
                    .extension()
                    .map(|ext| ext == "jsonl")
                    .unwrap_or(false)
            })
            .collect();
        
        if sessions.is_empty() {
            return Ok(None);
        }
        
        // Sort by modification time
        sessions.sort_by_key(|entry| {
            entry
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        });
        
        // Return the most recent
        Ok(sessions.into_iter().last().map(|e| e.path()))
    }
    
    /// Get git state for a workspace
    fn get_git_state(workspace: &Path) -> Result<(String, bool, Option<String>)> {
        // Check if it's a git repository
        if !workspace.join(".git").exists() {
            debug!("Workspace is not a git repository");
            return Ok(("main".to_string(), false, None));
        }
        
        // Get current branch
        let branch_output = Command::new("git")
            .args(&["branch", "--show-current"])
            .current_dir(workspace)
            .output()?;
        
        let branch = if branch_output.status.success() {
            String::from_utf8_lossy(&branch_output.stdout)
                .trim()
                .to_string()
        } else {
            "main".to_string()
        };
        
        // Check for uncommitted changes
        let status_output = Command::new("git")
            .args(&["status", "--porcelain"])
            .current_dir(workspace)
            .output()?;
        
        let has_changes = !status_output.stdout.is_empty();
        
        // Get remote URL (optional)
        let remote_output = Command::new("git")
            .args(&["remote", "get-url", "origin"])
            .current_dir(workspace)
            .output()
            .ok();
        
        let remote_url = remote_output
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string());
        
        Ok((branch, has_changes, remote_url))
    }
    
    /// Copy a session file with updated session IDs
    async fn copy_session_with_new_id(
        source: &Path,
        dest: &Path,
        new_session_id: &str,
    ) -> Result<()> {
        let content = fs::read_to_string(source)?;
        let mut output = Vec::new();
        
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            
            let mut entry: Value = serde_json::from_str(line)
                .with_context(|| format!("Failed to parse session line: {}", line))?;
            
            // Update sessionId field if present
            if let Some(obj) = entry.as_object_mut() {
                if obj.contains_key("sessionId") {
                    obj.insert(
                        "sessionId".to_string(),
                        serde_json::json!(new_session_id),
                    );
                }
            }
            
            output.push(serde_json::to_string(&entry)?);
        }
        
        fs::write(dest, output.join("\n"))?;
        Ok(())
    }
}