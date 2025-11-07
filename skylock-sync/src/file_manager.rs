use std::path::PathBuf;
use tokio::sync::mpsc;
use crate::organizer::{FileOrganizer, OrganizationRule, OrganizeProgress};
use crate::tag_manager::{TagManager, TagRule};
use crate::file_info::FileInfo;
use skylock_core::Result;

pub struct FileManager {
    organizer: FileOrganizer,
    tag_manager: TagManager,
    base_path: PathBuf,
    progress_tx: mpsc::Sender<Progress>,
}

#[derive(Debug, Clone)]
pub enum Progress {
    Organizing(OrganizeProgress),
    Tagging {
        file: PathBuf,
        tags: Vec<String>,
    },
    Scanning {
        files_processed: u64,
        total_files: u64,
        current_file: PathBuf,
    },
}

impl FileManager {
    pub fn new(base_path: PathBuf, progress_tx: mpsc::Sender<Progress>) -> Self {
        let (org_tx, mut org_rx) = mpsc::channel(100);

        // Forward organization progress to main progress channel
        let progress_tx_clone = progress_tx.clone();
        tokio::spawn(async move {
            while let Some(org_progress) = org_rx.recv().await {
                let _ = progress_tx_clone.send(Progress::Organizing(org_progress)).await;
            }
        });

        Self {
            organizer: FileOrganizer::new(base_path.clone(), org_tx),
            tag_manager: TagManager::new(base_path.clone()),
            base_path,
            progress_tx,
        }
    }

    pub fn add_organization_rule(&mut self, rule: OrganizationRule) {
        self.organizer.add_rule(rule);
    }

    pub fn add_tag_rule(&mut self, rule: TagRule) {
        self.tag_manager.add_rule(rule);
    }

    pub async fn process_workspace(&mut self) -> Result<()> {
        let mut stack = vec![self.base_path.clone()];
        let mut total_files = 0;
        let mut processed = 0;

        // First pass to count files
        {
            let mut count_stack = vec![self.base_path.clone()];
            while let Some(dir) = count_stack.pop() {
                let mut entries = tokio::fs::read_dir(&dir).await?;
                while let Some(entry) = entries.next_entry().await? {
                    let path = entry.path();
                    if path.is_dir() {
                        count_stack.push(path);
                    } else {
                        total_files += 1;
                    }
                }
            }
        }

        // Process files
        while let Some(dir) = stack.pop() {
            let mut entries = tokio::fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();

                if path.is_dir() {
                    stack.push(path);
                    continue;
                }

                processed += 1;
                self.progress_tx.send(Progress::Scanning {
                    files_processed: processed,
                    total_files,
                    current_file: path.clone(),
                }).await?;

                let file_info = FileInfo::from_path(&path).await?;

                // Apply organization rules
                let _ = self.organizer.organize_file(&file_info).await?;

                // Apply tagging rules
                if let Ok(tags) = self.tag_manager.tag_file(&file_info).await {
                    if !tags.is_empty() {
                        self.progress_tx.send(Progress::Tagging {
                            file: path,
                            tags: tags.into_iter().collect(),
                        }).await?;
                    }
                }
            }
        }

        // Final cleanup
        self.organizer.cleanup().await?;
        self.tag_manager.rebuild_index().await?;

        Ok(())
    }

    pub async fn get_file_tags(&self, path: &PathBuf) -> Result<Vec<String>> {
        Ok(self.tag_manager.get_file_tags(path).await?
            .into_iter()
            .collect())
    }

    pub fn get_files_with_tag(&self, tag: &str) -> Vec<PathBuf> {
        self.tag_manager.get_files_with_tag(tag)
    }

    pub async fn add_tag_to_file(&mut self, path: &PathBuf, tag: &str) -> Result<()> {
        self.tag_manager.add_tag_to_file(path, tag).await
    }

    pub async fn remove_tag_from_file(&mut self, path: &PathBuf, tag: &str) -> Result<()> {
        self.tag_manager.remove_tag(path, tag).await
    }
}
