//! Encrypted Manifest Module
//!
//! Provides E2E encrypted manifest storage that protects file metadata:
//! - File names, paths, and directory structure
//! - File sizes and timestamps
//! - Hash values and compression status
//!
//! Only users with the correct encryption key can browse backup contents.
//! The storage provider sees only encrypted blobs.
//!
//! Architecture:
//! - `manifest.json.enc` - Encrypted full manifest (AES-256-GCM)
//! - `manifest_header.json` - Public header for backup listing (backup_id, timestamp only)

use std::path::{Path, PathBuf};
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use crate::error::{Result, SkylockError};
use crate::encryption::EncryptionManager;
use crate::direct_upload::{BackupManifest, FileEntry};

/// Public manifest header - visible without encryption key
/// Contains minimal info needed for backup listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestHeader {
    /// Unique backup ID
    pub backup_id: String,
    /// When backup was created
    pub timestamp: DateTime<Utc>,
    /// Total number of files (count only, not names)
    pub file_count: usize,
    /// Total backup size in bytes
    pub total_size: u64,
    /// Encryption format version
    pub encryption_version: String,
    /// Whether manifest is encrypted (v3+ always true)
    pub manifest_encrypted: bool,
    /// SHA-256 hash of encrypted manifest for integrity
    pub encrypted_manifest_hash: String,
    /// Version of manifest format
    pub manifest_format_version: u32,
}

impl ManifestHeader {
    /// Create header from full manifest
    pub fn from_manifest(manifest: &BackupManifest, encrypted_hash: &str) -> Self {
        Self {
            backup_id: manifest.backup_id.clone(),
            timestamp: manifest.timestamp,
            file_count: manifest.file_count,
            total_size: manifest.total_size,
            encryption_version: manifest.encryption_version.clone(),
            manifest_encrypted: true,
            encrypted_manifest_hash: encrypted_hash.to_string(),
            manifest_format_version: 3, // v3 = encrypted manifests
        }
    }
}

/// Encrypted manifest container
#[derive(Debug)]
pub struct EncryptedManifest {
    /// Public header (always readable)
    pub header: ManifestHeader,
    /// Encrypted manifest data (requires key to decrypt)
    pub encrypted_data: Vec<u8>,
}

/// File tree node for hierarchical browsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTreeNode {
    /// Node name (file or directory name)
    pub name: String,
    /// Full path
    pub path: PathBuf,
    /// Whether this is a directory
    pub is_directory: bool,
    /// File size (0 for directories)
    pub size: u64,
    /// File hash (empty for directories)
    pub hash: String,
    /// Whether compressed
    pub compressed: bool,
    /// Last modified timestamp
    pub timestamp: DateTime<Utc>,
    /// Children (for directories)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<FileTreeNode>,
}

impl FileTreeNode {
    /// Create a file node
    pub fn file(entry: &FileEntry) -> Self {
        Self {
            name: entry.local_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            path: entry.local_path.clone(),
            is_directory: false,
            size: entry.size,
            hash: entry.hash.clone(),
            compressed: entry.compressed,
            timestamp: entry.timestamp,
            children: Vec::new(),
        }
    }
    
    /// Create a directory node
    pub fn directory(name: &str, path: PathBuf) -> Self {
        Self {
            name: name.to_string(),
            path,
            is_directory: true,
            size: 0,
            hash: String::new(),
            compressed: false,
            timestamp: Utc::now(),
            children: Vec::new(),
        }
    }
    
    /// Add child node
    pub fn add_child(&mut self, child: FileTreeNode) {
        if self.is_directory {
            self.children.push(child);
        }
    }
    
    /// Get total size of directory (recursive)
    pub fn total_size(&self) -> u64 {
        if self.is_directory {
            self.children.iter().map(|c| c.total_size()).sum()
        } else {
            self.size
        }
    }
    
    /// Get total file count (recursive)
    pub fn file_count(&self) -> usize {
        if self.is_directory {
            self.children.iter().map(|c| c.file_count()).sum()
        } else {
            1
        }
    }
}

/// Build file tree from manifest entries
pub fn build_file_tree(files: &[FileEntry]) -> Vec<FileTreeNode> {
    use std::collections::BTreeMap;
    
    // Group files by parent directory
    let mut dir_map: BTreeMap<PathBuf, Vec<&FileEntry>> = BTreeMap::new();
    
    for entry in files {
        let parent = entry.local_path
            .parent()
            .unwrap_or(Path::new("/"))
            .to_path_buf();
        dir_map.entry(parent).or_default().push(entry);
    }
    
    // Build tree structure
    let mut root_nodes: Vec<FileTreeNode> = Vec::new();
    
    for (dir_path, entries) in dir_map {
        // Create directory node
        let dir_name = dir_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string();
        
        let mut dir_node = FileTreeNode::directory(&dir_name, dir_path.clone());
        
        // Add file children
        for entry in entries {
            dir_node.add_child(FileTreeNode::file(entry));
        }
        
        // Sort children by name
        dir_node.children.sort_by(|a, b| a.name.cmp(&b.name));
        
        root_nodes.push(dir_node);
    }
    
    // Sort directories by path
    root_nodes.sort_by(|a, b| a.path.cmp(&b.path));
    
    root_nodes
}

/// Manifest encryption handler
pub struct ManifestEncryption<'a> {
    encryption: &'a EncryptionManager,
}

impl<'a> ManifestEncryption<'a> {
    /// Create new manifest encryption handler
    pub fn new(encryption: &'a EncryptionManager) -> Self {
        Self { encryption }
    }
    
    /// Encrypt a backup manifest
    /// 
    /// Returns the encrypted data and public header
    pub fn encrypt_manifest(&self, manifest: &BackupManifest) -> Result<EncryptedManifest> {
        // Serialize manifest to JSON
        let manifest_json = serde_json::to_vec_pretty(manifest)
            .map_err(|e| SkylockError::Encryption(
                format!("Failed to serialize manifest: {}", e)
            ))?;
        
        // Encrypt with AAD binding to backup_id
        let encrypted_data = self.encryption.encrypt_with_aad(
            &manifest_json,
            &manifest.backup_id,
            "manifest.json"
        )?;
        
        // Calculate hash of encrypted data for integrity
        let mut hasher = Sha256::new();
        hasher.update(&encrypted_data);
        let hash = format!("{:x}", hasher.finalize());
        
        // Create public header
        let header = ManifestHeader::from_manifest(manifest, &hash);
        
        Ok(EncryptedManifest {
            header,
            encrypted_data,
        })
    }
    
    /// Decrypt a backup manifest
    /// 
    /// Requires the correct encryption key
    pub fn decrypt_manifest(
        &self,
        encrypted_data: &[u8],
        backup_id: &str,
    ) -> Result<BackupManifest> {
        // Decrypt with AAD verification
        let decrypted = self.encryption.decrypt_with_aad(
            encrypted_data,
            backup_id,
            "manifest.json"
        )?;
        
        // Deserialize JSON
        let manifest: BackupManifest = serde_json::from_slice(&decrypted)
            .map_err(|e| SkylockError::Encryption(
                format!("Failed to deserialize manifest: {}", e)
            ))?;
        
        Ok(manifest)
    }
    
    /// Verify encrypted manifest integrity
    pub fn verify_integrity(&self, encrypted_data: &[u8], expected_hash: &str) -> bool {
        let mut hasher = Sha256::new();
        hasher.update(encrypted_data);
        let actual_hash = format!("{:x}", hasher.finalize());
        
        // Constant-time comparison
        use subtle::ConstantTimeEq;
        actual_hash.as_bytes().ct_eq(expected_hash.as_bytes()).into()
    }
}

/// Browseable backup - decrypted view of a backup for authorized users
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseableBackup {
    /// Backup ID
    pub backup_id: String,
    /// Backup timestamp
    pub timestamp: DateTime<Utc>,
    /// File tree structure
    pub file_tree: Vec<FileTreeNode>,
    /// Total file count
    pub file_count: usize,
    /// Total size in bytes
    pub total_size: u64,
    /// Source paths that were backed up
    pub source_paths: Vec<PathBuf>,
    /// Encryption version used
    pub encryption_version: String,
}

impl BrowseableBackup {
    /// Create browseable backup from decrypted manifest
    pub fn from_manifest(manifest: &BackupManifest) -> Self {
        let file_tree = build_file_tree(&manifest.files);
        
        Self {
            backup_id: manifest.backup_id.clone(),
            timestamp: manifest.timestamp,
            file_tree,
            file_count: manifest.file_count,
            total_size: manifest.total_size,
            source_paths: manifest.source_paths.clone(),
            encryption_version: manifest.encryption_version.clone(),
        }
    }
    
    /// Find a file by path
    pub fn find_file(&self, path: &str) -> Option<&FileTreeNode> {
        for dir in &self.file_tree {
            for file in &dir.children {
                if file.path.to_str() == Some(path) {
                    return Some(file);
                }
            }
        }
        None
    }
    
    /// List all files matching a pattern
    pub fn find_files_matching(&self, pattern: &str) -> Vec<&FileTreeNode> {
        let mut matches = Vec::new();
        let pattern_lower = pattern.to_lowercase();
        
        for dir in &self.file_tree {
            for file in &dir.children {
                if file.name.to_lowercase().contains(&pattern_lower)
                    || file.path.to_string_lossy().to_lowercase().contains(&pattern_lower)
                {
                    matches.push(file);
                }
            }
        }
        
        matches
    }
    
    /// Get files in a specific directory
    pub fn files_in_directory(&self, dir_path: &str) -> Vec<&FileTreeNode> {
        for dir in &self.file_tree {
            if dir.path.to_str() == Some(dir_path) {
                return dir.children.iter().collect();
            }
        }
        Vec::new()
    }
    
    /// Get all directories
    pub fn directories(&self) -> Vec<&FileTreeNode> {
        self.file_tree.iter().collect()
    }
    
    /// Get summary statistics
    pub fn summary(&self) -> BackupSummary {
        let mut compressed_count = 0;
        let mut compressed_size = 0u64;
        let mut uncompressed_size = 0u64;
        
        for dir in &self.file_tree {
            for file in &dir.children {
                if file.compressed {
                    compressed_count += 1;
                    compressed_size += file.size;
                } else {
                    uncompressed_size += file.size;
                }
            }
        }
        
        BackupSummary {
            backup_id: self.backup_id.clone(),
            timestamp: self.timestamp,
            total_files: self.file_count,
            total_directories: self.file_tree.len(),
            total_size: self.total_size,
            compressed_files: compressed_count,
            compressed_size,
            uncompressed_size,
        }
    }
}

/// Summary statistics for a backup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSummary {
    pub backup_id: String,
    pub timestamp: DateTime<Utc>,
    pub total_files: usize,
    pub total_directories: usize,
    pub total_size: u64,
    pub compressed_files: usize,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
}

impl BackupSummary {
    /// Format size in human-readable format
    pub fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        
        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
    
    /// Get compression ratio
    pub fn compression_ratio(&self) -> f64 {
        if self.compressed_size == 0 {
            return 1.0;
        }
        // Note: We don't store original size before compression
        // This would need to be tracked separately
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn create_test_entry(path: &str, size: u64, compressed: bool) -> FileEntry {
        FileEntry {
            local_path: PathBuf::from(path),
            remote_path: format!("/backup/{}.enc", path),
            size,
            hash: "abc123".to_string(),
            compressed,
            encrypted: true,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_file_tree_building() {
        let files = vec![
            create_test_entry("/home/user/doc.txt", 100, false),
            create_test_entry("/home/user/image.png", 5000, true),
            create_test_entry("/home/user/projects/code.rs", 2000, false),
        ];
        
        let tree = build_file_tree(&files);
        
        // Should have 2 directories: /home/user and /home/user/projects
        assert_eq!(tree.len(), 2);
        
        // First directory should have 2 files
        let user_dir = tree.iter().find(|d| d.path.ends_with("user")).unwrap();
        assert_eq!(user_dir.children.len(), 2);
    }

    #[test]
    fn test_file_tree_node_total_size() {
        let mut dir = FileTreeNode::directory("test", PathBuf::from("/test"));
        dir.add_child(FileTreeNode {
            name: "file1.txt".to_string(),
            path: PathBuf::from("/test/file1.txt"),
            is_directory: false,
            size: 100,
            hash: String::new(),
            compressed: false,
            timestamp: Utc::now(),
            children: vec![],
        });
        dir.add_child(FileTreeNode {
            name: "file2.txt".to_string(),
            path: PathBuf::from("/test/file2.txt"),
            is_directory: false,
            size: 200,
            hash: String::new(),
            compressed: false,
            timestamp: Utc::now(),
            children: vec![],
        });
        
        assert_eq!(dir.total_size(), 300);
        assert_eq!(dir.file_count(), 2);
    }

    #[test]
    fn test_manifest_header_creation() {
        let manifest = BackupManifest {
            backup_id: "test_backup".to_string(),
            timestamp: Utc::now(),
            files: vec![],
            total_size: 1000,
            file_count: 5,
            source_paths: vec![],
            base_backup_id: None,
            encryption_version: "v2".to_string(),
            kdf_params: None,
            signature: None,
            backup_chain_version: 0,
            encrypted_path_map: None,
        };
        
        let header = ManifestHeader::from_manifest(&manifest, "abc123hash");
        
        assert_eq!(header.backup_id, "test_backup");
        assert_eq!(header.file_count, 5);
        assert_eq!(header.total_size, 1000);
        assert!(header.manifest_encrypted);
        assert_eq!(header.manifest_format_version, 3);
    }

    #[test]
    fn test_manifest_encryption_roundtrip() {
        let encryption = EncryptionManager::new("test_password").unwrap();
        let handler = ManifestEncryption::new(&encryption);
        
        let manifest = BackupManifest {
            backup_id: "test_backup".to_string(),
            timestamp: Utc::now(),
            files: vec![create_test_entry("/test/file.txt", 100, false)],
            total_size: 100,
            file_count: 1,
            source_paths: vec![PathBuf::from("/test")],
            base_backup_id: None,
            encryption_version: "v2".to_string(),
            kdf_params: None,
            signature: None,
            backup_chain_version: 0,
            encrypted_path_map: None,
        };
        
        // Encrypt
        let encrypted = handler.encrypt_manifest(&manifest).unwrap();
        
        // Verify header
        assert_eq!(encrypted.header.backup_id, "test_backup");
        assert!(encrypted.header.manifest_encrypted);
        
        // Verify integrity
        assert!(handler.verify_integrity(
            &encrypted.encrypted_data,
            &encrypted.header.encrypted_manifest_hash
        ));
        
        // Decrypt
        let decrypted = handler.decrypt_manifest(
            &encrypted.encrypted_data,
            "test_backup"
        ).unwrap();
        
        assert_eq!(decrypted.backup_id, manifest.backup_id);
        assert_eq!(decrypted.file_count, manifest.file_count);
        assert_eq!(decrypted.files.len(), 1);
        assert_eq!(decrypted.files[0].local_path, PathBuf::from("/test/file.txt"));
    }

    #[test]
    fn test_wrong_key_fails() {
        let encryption1 = EncryptionManager::new("password1").unwrap();
        let encryption2 = EncryptionManager::new("password2").unwrap();
        
        let handler1 = ManifestEncryption::new(&encryption1);
        let handler2 = ManifestEncryption::new(&encryption2);
        
        let manifest = BackupManifest {
            backup_id: "test".to_string(),
            timestamp: Utc::now(),
            files: vec![],
            total_size: 0,
            file_count: 0,
            source_paths: vec![],
            base_backup_id: None,
            encryption_version: "v2".to_string(),
            kdf_params: None,
            signature: None,
            backup_chain_version: 0,
            encrypted_path_map: None,
        };
        
        let encrypted = handler1.encrypt_manifest(&manifest).unwrap();
        
        // Wrong key should fail to decrypt
        let result = handler2.decrypt_manifest(&encrypted.encrypted_data, "test");
        assert!(result.is_err());
    }

    #[test]
    fn test_browseable_backup() {
        let manifest = BackupManifest {
            backup_id: "browse_test".to_string(),
            timestamp: Utc::now(),
            files: vec![
                create_test_entry("/home/user/doc.txt", 100, false),
                create_test_entry("/home/user/image.png", 5000, true),
            ],
            total_size: 5100,
            file_count: 2,
            source_paths: vec![PathBuf::from("/home/user")],
            base_backup_id: None,
            encryption_version: "v2".to_string(),
            kdf_params: None,
            signature: None,
            backup_chain_version: 0,
            encrypted_path_map: None,
        };
        
        let browseable = BrowseableBackup::from_manifest(&manifest);
        
        assert_eq!(browseable.file_count, 2);
        assert_eq!(browseable.total_size, 5100);
        
        // Test find_file
        let found = browseable.find_file("/home/user/doc.txt");
        assert!(found.is_some());
        assert_eq!(found.unwrap().size, 100);
        
        // Test find_files_matching
        let matches = browseable.find_files_matching(".txt");
        assert_eq!(matches.len(), 1);
        
        // Test summary
        let summary = browseable.summary();
        assert_eq!(summary.compressed_files, 1);
    }
}
