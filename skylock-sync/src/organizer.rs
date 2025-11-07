use std::path::{Path, PathBuf};
use tokio::fs;
use serde::{Serialize, Deserialize};
use skylock_core::Result;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, HashSet};
use regex::Regex;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationRule {
    pub name: String,
    pub patterns: Vec<String>,
    pub destination: PathBuf,
    pub structure: FolderStructure,
    pub conditions: Vec<Condition>,
    pub actions: Vec<Action>,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FolderStructure {
    Flat,
    DateBased(DateFormat),
    CategoryBased,
    HierarchicalType,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DateFormat {
    YearMonth,      // YYYY/MM
    YearMonthDay,   // YYYY/MM/DD
    Year,           // YYYY
    Custom(String), // Custom strftime format
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Condition {
    FileSize(SizeCondition),
    FileAge(AgeCondition),
    FileType(Vec<String>),
    ContentType(Vec<ContentType>),
    Pattern(String),
    Metadata(String, String),
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SizeCondition {
    pub min: Option<u64>,
    pub max: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeCondition {
    pub min_days: Option<i64>,
    pub max_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    Move,
    Copy,
    Link,
    Tag(Vec<String>),
    Rename(String),
    Compress,
    Custom(String),
}

pub struct FileOrganizer {
    rules: Vec<OrganizationRule>,
    base_path: PathBuf,
    processed_files: HashSet<PathBuf>,
    progress_tx: mpsc::Sender<OrganizeProgress>,
}

#[derive(Debug, Clone)]
pub struct OrganizeProgress {
    pub files_processed: u64,
    pub total_files: u64,
    pub current_file: PathBuf,
    pub action: String,
}

impl FileOrganizer {
    pub fn new(base_path: PathBuf, progress_tx: mpsc::Sender<OrganizeProgress>) -> Self {
        Self {
            rules: Vec::new(),
            base_path,
            processed_files: HashSet::new(),
            progress_tx,
        }
    }

    pub fn add_rule(&mut self, rule: OrganizationRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| -r.priority); // Sort by priority (highest first)
    }

    pub async fn organize_file(&mut self, file_info: &FileInfo) -> Result<Vec<Action>> {
        let mut applied_actions = Vec::new();

        for rule in &self.rules {
            if self.matches_rule(file_info, rule) {
                let actions = self.apply_rule(file_info, rule).await?;
                applied_actions.extend(actions);

                // Update progress
                self.progress_tx.send(OrganizeProgress {
                    files_processed: self.processed_files.len() as u64,
                    total_files: 0, // Updated by caller
                    current_file: file_info.path.clone(),
                    action: format!("Applied rule: {}", rule.name),
                }).await?;
            }
        }

        self.processed_files.insert(file_info.path.clone());
        Ok(applied_actions)
    }

    fn matches_rule(&self, file_info: &FileInfo, rule: &OrganizationRule) -> bool {
        // Check if file matches any pattern
        let matches_pattern = rule.patterns.iter().any(|pattern| {
            if let Ok(regex) = Regex::new(pattern) {
                regex.is_match(&file_info.path.to_string_lossy())
            } else {
                false
            }
        });

        if !matches_pattern {
            return false;
        }

        // Check all conditions
        rule.conditions.iter().all(|condition| {
            match condition {
                Condition::FileSize(size_cond) => {
                    let size = file_info.size;
                    size_cond.min.map_or(true, |min| size >= min) &&
                    size_cond.max.map_or(true, |max| size <= max)
                },
                Condition::FileAge(age_cond) => {
                    let age = Utc::now() - file_info.modified;
                    let age_days = age.num_days();
                    age_cond.min_days.map_or(true, |min| age_days >= min) &&
                    age_cond.max_days.map_or(true, |max| age_days <= max)
                },
                Condition::FileType(extensions) => {
                    file_info.extension.as_ref()
                        .map_or(false, |ext| extensions.contains(&ext.to_string()))
                },
                Condition::ContentType(types) => {
                    types.contains(&file_info.metadata.content_type)
                },
                Condition::Pattern(pattern) => {
                    Regex::new(pattern)
                        .map_or(false, |re| re.is_match(&file_info.name))
                },
                Condition::Metadata(key, value) => {
                    file_info.metadata.attributes.get(key)
                        .map_or(false, |v| v == value)
                },
                Condition::Custom(_) => true, // Custom conditions handled separately
            }
        })
    }

    async fn apply_rule(&self, file_info: &FileInfo, rule: &OrganizationRule) -> Result<Vec<Action>> {
        let mut applied_actions = Vec::new();

        for action in &rule.actions {
            match action {
                Action::Move => {
                    let dest = self.get_destination_path(file_info, rule)?;
                    fs::create_dir_all(dest.parent().unwrap()).await?;
                    fs::rename(&file_info.path, &dest).await?;
                    applied_actions.push(action.clone());
                },
                Action::Copy => {
                    let dest = self.get_destination_path(file_info, rule)?;
                    fs::create_dir_all(dest.parent().unwrap()).await?;
                    fs::copy(&file_info.path, &dest).await?;
                    applied_actions.push(action.clone());
                },
                Action::Link => {
                    let dest = self.get_destination_path(file_info, rule)?;
                    fs::create_dir_all(dest.parent().unwrap()).await?;
                    #[cfg(windows)]
                    std::os::windows::fs::symlink_file(&file_info.path, &dest)?;
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&file_info.path, &dest)?;
                    applied_actions.push(action.clone());
                },
                Action::Tag(tags) => {
                    // Tags are handled by the tagging system
                    applied_actions.push(action.clone());
                },
                Action::Rename(pattern) => {
                    let new_name = self.apply_rename_pattern(file_info, pattern)?;
                    let new_path = file_info.path.with_file_name(new_name);
                    fs::rename(&file_info.path, &new_path).await?;
                    applied_actions.push(action.clone());
                },
                Action::Compress => {
                    let dest = self.get_destination_path(file_info, rule)?;
                    self.compress_file(file_info, &dest).await?;
                    applied_actions.push(action.clone());
                },
                Action::Custom(_) => {
                    // Custom actions handled separately
                },
            }
        }

        Ok(applied_actions)
    }

    fn get_destination_path(&self, file_info: &FileInfo, rule: &OrganizationRule) -> Result<PathBuf> {
        let mut dest = rule.destination.clone();

        match &rule.structure {
            FolderStructure::Flat => {
                dest.push(&file_info.name);
            },
            FolderStructure::DateBased(format) => {
                let date_path = match format {
                    DateFormat::YearMonth => {
                        file_info.modified.format("%Y/%m").to_string()
                    },
                    DateFormat::YearMonthDay => {
                        file_info.modified.format("%Y/%m/%d").to_string()
                    },
                    DateFormat::Year => {
                        file_info.modified.format("%Y").to_string()
                    },
                    DateFormat::Custom(fmt) => {
                        file_info.modified.format(fmt).to_string()
                    },
                };
                dest.push(date_path);
                dest.push(&file_info.name);
            },
            FolderStructure::CategoryBased => {
                let category = format!("{:?}", file_info.metadata.content_type);
                dest.push(category);
                dest.push(&file_info.name);
            },
            FolderStructure::HierarchicalType => {
                if let Some(ext) = &file_info.extension {
                    dest.push(ext);
                }
                dest.push(&file_info.name);
            },
            FolderStructure::Custom(pattern) => {
                let path = self.apply_pattern(file_info, pattern)?;
                dest.push(path);
            },
        }

        Ok(dest)
    }

    fn apply_rename_pattern(&self, file_info: &FileInfo, pattern: &str) -> Result<String> {
        let mut result = pattern.to_string();

        // Replace placeholders
        result = result.replace("{name}", &file_info.name);
        result = result.replace("{ext}", file_info.extension.as_deref().unwrap_or(""));
        result = result.replace("{date}", &file_info.modified.format("%Y%m%d").to_string());
        result = result.replace("{type}", &format!("{:?}", file_info.metadata.content_type));

        Ok(result)
    }

    fn apply_pattern(&self, file_info: &FileInfo, pattern: &str) -> Result<PathBuf> {
        let mut result = pattern.to_string();

        // Replace placeholders
        result = result.replace("{year}", &file_info.modified.format("%Y").to_string());
        result = result.replace("{month}", &file_info.modified.format("%m").to_string());
        result = result.replace("{day}", &file_info.modified.format("%d").to_string());
        result = result.replace("{type}", &format!("{:?}", file_info.metadata.content_type));
        result = result.replace("{ext}", file_info.extension.as_deref().unwrap_or(""));

        Ok(PathBuf::from(result))
    }

    async fn compress_file(&self, file_info: &FileInfo, dest: &Path) -> Result<()> {
        use async_compression::tokio::write::GzipEncoder;
        use tokio::io::AsyncWriteExt;

        let source = fs::File::open(&file_info.path).await?;
        let compressed = fs::File::create(dest).await?;
        let mut encoder = GzipEncoder::new(compressed);

        tokio::io::copy(&mut source.into_std().await, &mut encoder).await?;
        encoder.shutdown().await?;

        Ok(())
    }

    pub async fn cleanup(&mut self) -> Result<()> {
        // Remove empty directories
        let mut dirs_to_check = Vec::new();
        for rule in &self.rules {
            dirs_to_check.push(rule.destination.clone());
        }

        for dir in dirs_to_check {
            self.cleanup_empty_dirs(&dir).await?;
        }

        Ok(())
    }

    async fn cleanup_empty_dirs(&self, dir: &Path) -> Result<bool> {
        if !dir.is_dir() {
            return Ok(false);
        }

        let mut is_empty = true;
        let mut entries = fs::read_dir(dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                if !self.cleanup_empty_dirs(&path).await? {
                    is_empty = false;
                }
            } else {
                is_empty = false;
            }
        }

        if is_empty {
            fs::remove_dir(dir).await?;
        }

        Ok(is_empty)
    }
}
