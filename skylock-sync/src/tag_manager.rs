use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use tokio::fs;
use skylock_core::Result;
use std::collections::{HashMap, HashSet};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub created: DateTime<Utc>,
    pub modified: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRule {
    pub name: String,
    pub conditions: Vec<TagCondition>,
    pub tags: Vec<String>,
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagCondition {
    Filename(String),
    Path(String),
    Extension(Vec<String>),
    Content(String),
    Size(u64, u64),
    Age(i64, i64),
    Metadata(String, String),
    Custom(String),
}

pub struct TagManager {
    tags: HashMap<String, Tag>,
    rules: Vec<TagRule>,
    tag_index: HashMap<PathBuf, HashSet<String>>,
    base_path: PathBuf,
}

impl TagManager {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            tags: HashMap::new(),
            rules: Vec::new(),
            tag_index: HashMap::new(),
            base_path,
        }
    }

    pub fn add_tag(&mut self, name: String, color: Option<String>, description: Option<String>) -> Result<()> {
        let tag = Tag {
            name: name.clone(),
            color,
            description,
            created: Utc::now(),
            modified: Utc::now(),
            metadata: HashMap::new(),
        };

        self.tags.insert(name, tag);
        Ok(())
    }

    pub fn add_rule(&mut self, rule: TagRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| -r.priority); // Sort by priority (highest first)
    }

    pub async fn tag_file(&mut self, file_info: &FileInfo) -> Result<HashSet<String>> {
        let mut applied_tags = HashSet::new();

        for rule in &self.rules {
            if self.matches_rule(file_info, rule).await? {
                for tag_name in &rule.tags {
                    if self.tags.contains_key(tag_name) {
                        applied_tags.insert(tag_name.clone());
                    }
                }
            }
        }

        if !applied_tags.is_empty() {
            self.tag_index.insert(file_info.path.clone(), applied_tags.clone());
            self.save_tags(file_info, &applied_tags).await?;
        }

        Ok(applied_tags)
    }

    async fn matches_rule(&self, file_info: &FileInfo, rule: &TagRule) -> Result<bool> {
        for condition in &rule.conditions {
            match condition {
                TagCondition::Filename(pattern) => {
                    if !regex::Regex::new(pattern)?.is_match(&file_info.name) {
                        return Ok(false);
                    }
                },
                TagCondition::Path(pattern) => {
                    if !regex::Regex::new(pattern)?.is_match(&file_info.path.to_string_lossy()) {
                        return Ok(false);
                    }
                },
                TagCondition::Extension(extensions) => {
                    if let Some(ext) = &file_info.extension {
                        if !extensions.contains(&ext.to_string()) {
                            return Ok(false);
                        }
                    } else {
                        return Ok(false);
                    }
                },
                TagCondition::Content(pattern) => {
                    let content = fs::read_to_string(&file_info.path).await?;
                    if !regex::Regex::new(pattern)?.is_match(&content) {
                        return Ok(false);
                    }
                },
                TagCondition::Size(min, max) => {
                    let size = file_info.size;
                    if size < *min || size > *max {
                        return Ok(false);
                    }
                },
                TagCondition::Age(min_days, max_days) => {
                    let age = Utc::now() - file_info.modified;
                    let age_days = age.num_days();
                    if age_days < *min_days || age_days > *max_days {
                        return Ok(false);
                    }
                },
                TagCondition::Metadata(key, value) => {
                    if file_info.metadata.attributes.get(key) != Some(value) {
                        return Ok(false);
                    }
                },
                TagCondition::Custom(_) => {
                    // Custom conditions handled separately
                },
            }
        }

        Ok(true)
    }

    async fn save_tags(&self, file_info: &FileInfo, tags: &HashSet<String>) -> Result<()> {
        // Save tags to extended attributes or alternate data stream
        #[cfg(windows)]
        {
            use windows_acl::acl::ACL;
            let mut acl = ACL::from_file_path(&file_info.path)?;
            let tags_str = serde_json::to_string(tags)?;
            acl.set_security_info(&tags_str, "SKYLOCK_TAGS")?;
        }

        #[cfg(unix)]
        {
            use xattr::FileExt;
            let file = std::fs::File::open(&file_info.path)?;
            let tags_str = serde_json::to_string(tags)?;
            file.set_xattr("user.skylock.tags", tags_str.as_bytes())?;
        }

        Ok(())
    }

    pub async fn get_file_tags(&self, path: &PathBuf) -> Result<HashSet<String>> {
        if let Some(tags) = self.tag_index.get(path) {
            return Ok(tags.clone());
        }

        // Try to load tags from file metadata
        #[cfg(windows)]
        {
            use windows_acl::acl::ACL;
            if let Ok(acl) = ACL::from_file_path(path) {
                if let Ok(tags_str) = acl.get_security_info("SKYLOCK_TAGS") {
                    if let Ok(tags) = serde_json::from_str::<HashSet<String>>(&tags_str) {
                        return Ok(tags);
                    }
                }
            }
        }

        #[cfg(unix)]
        {
            use xattr::FileExt;
            if let Ok(file) = std::fs::File::open(path) {
                if let Ok(Some(tags_bytes)) = file.get_xattr("user.skylock.tags") {
                    if let Ok(tags_str) = String::from_utf8(tags_bytes) {
                        if let Ok(tags) = serde_json::from_str::<HashSet<String>>(&tags_str) {
                            return Ok(tags);
                        }
                    }
                }
            }
        }

        Ok(HashSet::new())
    }

    pub fn get_files_with_tag(&self, tag: &str) -> Vec<PathBuf> {
        self.tag_index
            .iter()
            .filter_map(|(path, tags)| {
                if tags.contains(tag) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_all_tags(&self) -> Vec<Tag> {
        self.tags.values().cloned().collect()
    }

    pub async fn remove_tag(&mut self, path: &PathBuf, tag: &str) -> Result<()> {
        if let Some(tags) = self.tag_index.get_mut(path) {
            tags.remove(tag);
            self.save_tags(&FileInfo::from_path(path).await?, tags).await?;
        }
        Ok(())
    }

    pub async fn add_tag_to_file(&mut self, path: &PathBuf, tag: &str) -> Result<()> {
        if !self.tags.contains_key(tag) {
            return Ok(());
        }

        let mut tags = self.get_file_tags(path).await?;
        tags.insert(tag.to_string());
        self.tag_index.insert(path.clone(), tags.clone());
        self.save_tags(&FileInfo::from_path(path).await?, &tags).await?;

        Ok(())
    }

    pub async fn rebuild_index(&mut self) -> Result<()> {
        self.tag_index.clear();

        let mut stack = vec![self.base_path.clone()];
        while let Some(dir) = stack.pop() {
            let mut entries = fs::read_dir(&dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    if let Ok(tags) = self.get_file_tags(&path).await {
                        if !tags.is_empty() {
                            self.tag_index.insert(path, tags);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
