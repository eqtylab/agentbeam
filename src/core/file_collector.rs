use anyhow::{Context, Result};
use futures::StreamExt;
use ignore::WalkBuilder;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use iroh_blobs::{
    format::collection::Collection,
    api::{blobs::{AddPathOptions, ImportMode, ExportMode, ExportOptions}, Store, TempTag},
    BlobsProtocol, BlobFormat,
};
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

use crate::core::config::{BeamMetadata, WARN_THRESHOLD};

pub struct FileCollector {
    root_path: PathBuf,
}

impl FileCollector {
    pub fn new(root_path: PathBuf) -> Self {
        Self { root_path }
    }

    pub fn collect_files(&self) -> Result<Vec<(String, PathBuf)>> {
        let walker = WalkBuilder::new(&self.root_path)
            .add_custom_ignore_filename(".beamignore")
            .git_ignore(true)
            .git_global(false)
            .git_exclude(false)
            .hidden(false)
            .build();

        let mut files = Vec::new();
        for entry in walker {
            let entry = entry?;
            if entry.file_type().map_or(false, |ft| ft.is_file()) {
                let path = entry.path();
                let relative = path
                    .strip_prefix(&self.root_path)
                    .context("Failed to strip prefix")?;
                let relative_str = relative
                    .to_str()
                    .context("Path contains invalid UTF-8")?
                    .replace('\\', "/");

                // Always exclude .agentbeam-* directories and their contents
                // These are our temporary directories and should never be transferred
                if relative_str.starts_with(".agentbeam-") || relative_str.contains("/.agentbeam-") {
                    trace!("Excluding agentbeam temp file: {}", relative_str);
                    continue;
                }

                files.push((relative_str, path.to_owned()));
            }
        }

        files.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(files)
    }

    pub async fn create_collection(
        &self,
        blobs: &BlobsProtocol,
        files: Vec<(String, PathBuf)>,
        metadata: BeamMetadata,
        mp: Option<&MultiProgress>,
    ) -> Result<(TempTag, u64, Collection)> {
        let file_count = files.len();
        let mut total_size = 0u64;

        let pb = mp.map(|mp| {
            let pb = mp.add(ProgressBar::new(file_count as u64));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
            );
            pb.set_message("Importing files...");
            pb
        });

        let mut collection_items = Vec::new();
        
        for (i, (relative_path, file_path)) in files.into_iter().enumerate() {
            if let Some(ref pb) = pb {
                pb.set_position(i as u64);
                pb.set_message(format!("Importing {}", relative_path));
            }

            let file_size = std::fs::metadata(&file_path)?.len();
            total_size += file_size;

            debug!("Adding file: {} ({}bytes)", relative_path, file_size);

            // Ensure absolute path for add_path_with_opts
            let abs_path = if file_path.is_absolute() {
                file_path
            } else {
                std::env::current_dir()?.join(&file_path)
            };

            // For now, use TryReference for all files since we've excluded
            // the problematic .agentbeam-* directories
            let import_mode = ImportMode::TryReference;

            let add_options = AddPathOptions {
                path: abs_path,
                mode: import_mode,
                format: BlobFormat::Raw,
            };

            let mut stream = blobs.store().add_path_with_opts(add_options).stream().await;
            let tag = loop {
                match stream.next().await {
                    Some(progress) => {
                        use iroh_blobs::api::blobs::AddProgressItem::*;
                        match progress {
                            Done(tag) => break tag,
                            Error(e) => return Err(e.into()),
                            _ => {}
                        }
                    }
                    None => anyhow::bail!("Import stream ended without tag"),
                }
            };

            collection_items.push((relative_path, *tag.hash()));
        }

        if total_size > WARN_THRESHOLD && mp.is_some() {
            println!("⚠️  Large workspace: {:.2}GB", total_size as f64 / 1_000_000_000.0);
        }

        // Add metadata to the collection
        let metadata_json = serde_json::to_vec(&metadata)?;
        let metadata_tag = blobs.add_slice(&metadata_json).await?;
        collection_items.push((".agentbeam-metadata.json".to_string(), metadata_tag.hash));

        let collection = Collection::from_iter(collection_items);
        let collection_tag = collection.clone().store(blobs.store()).await?;

        if let Some(pb) = pb {
            pb.finish_with_message(format!("✓ Imported {} files", file_count));
        }

        Ok((collection_tag, total_size, collection))
    }

    pub async fn export_collection(
        blobs: &BlobsProtocol,
        collection: Collection,
        target_dir: &Path,
        mp: Option<&MultiProgress>,
    ) -> Result<()> {
        std::fs::create_dir_all(target_dir)?;

        let pb = mp.map(|mp| {
            let pb = mp.add(ProgressBar::new(collection.len() as u64));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("█▉▊▋▌▍▎▏  "),
            );
            pb.set_message("Exporting files...");
            pb
        });

        for (i, (name, hash)) in collection.iter().enumerate() {
            // Don't skip metadata - we need it for restoration
            // if name == ".agentbeam-metadata.json" {
            //     continue;
            // }

            if let Some(ref pb) = pb {
                pb.set_position(i as u64);
                pb.set_message(format!("Exporting {}", name));
            }

            let target_path = if target_dir.is_absolute() {
                target_dir.join(name)
            } else {
                std::env::current_dir()?.join(target_dir).join(name)
            };
            
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut stream = blobs.store()
                .export_with_opts(ExportOptions {
                    hash: *hash,
                    target: target_path.clone(),
                    mode: ExportMode::TryReference,
                })
                .stream()
                .await;

            while let Some(progress) = stream.next().await {
                use iroh_blobs::api::blobs::ExportProgressItem::*;
                match progress {
                    Done => {
                        trace!("Exported {} to {}", name, target_path.display());
                        break;
                    }
                    Error(e) => return Err(e.into()),
                    _ => {}
                }
            }
        }

        if let Some(pb) = pb {
            pb.finish_with_message(format!("✓ Exported {} files", collection.len()));
        }

        Ok(())
    }
}