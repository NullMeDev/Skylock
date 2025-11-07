//! Backup diff/comparison module
//!
//! Provides functionality to compare two backups and identify differences.

use crate::direct_upload::BackupManifest;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Represents the difference between two backups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDiff {
    /// ID of the older backup (base)
    pub backup_id_old: String,
    /// ID of the newer backup
    pub backup_id_new: String,
    /// Timestamp of old backup
    pub timestamp_old: DateTime<Utc>,
    /// Timestamp of new backup
    pub timestamp_new: DateTime<Utc>,
    /// Files that were added in the new backup
    pub files_added: Vec<FileDiff>,
    /// Files that were removed in the new backup
    pub files_removed: Vec<FileDiff>,
    /// Files that were modified between backups
    pub files_modified: Vec<FileModification>,
    /// Files that were moved/renamed (same hash, different path)
    pub files_moved: Vec<FileMove>,
    /// Summary statistics
    pub summary: DiffSummary,
}

/// Information about a single file in a diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    /// Relative path of the file
    pub path: String,
    /// File size in bytes
    pub size: u64,
    /// SHA-256 hash
    pub hash: String,
    /// Whether file was compressed
    pub compressed: bool,
}

/// Information about a modified file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileModification {
    /// Relative path of the file
    pub path: String,
    /// Old file size
    pub size_old: u64,
    /// New file size
    pub size_new: u64,
    /// Size difference (new - old, can be negative)
    pub size_delta: i64,
    /// Old hash
    pub hash_old: String,
    /// New hash
    pub hash_new: String,
    /// Compression status changed
    pub compression_changed: bool,
}

/// Information about a moved/renamed file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMove {
    /// Old path
    pub path_old: String,
    /// New path
    pub path_new: String,
    /// File size (unchanged)
    pub size: u64,
    /// SHA-256 hash (unchanged)
    pub hash: String,
}

/// Summary statistics for a diff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    /// Total number of files added
    pub files_added_count: usize,
    /// Total number of files removed
    pub files_removed_count: usize,
    /// Total number of files modified
    pub files_modified_count: usize,
    /// Total number of files moved/renamed
    pub files_moved_count: usize,
    /// Total number of files unchanged
    pub files_unchanged_count: usize,
    /// Total size added (bytes)
    pub size_added: u64,
    /// Total size removed (bytes)
    pub size_removed: u64,
    /// Net size change (bytes, can be negative)
    pub size_delta: i64,
}

impl BackupDiff {
    /// Compare two backup manifests and generate a diff
    ///
    /// # Arguments
    /// * `manifest_old` - The older backup manifest (base)
    /// * `manifest_new` - The newer backup manifest
    ///
    /// # Returns
    /// * `BackupDiff` containing all differences
    pub fn compare(manifest_old: &BackupManifest, manifest_new: &BackupManifest) -> Self {
        // Convert FileEntry to FileMetadata
        use crate::direct_upload::FileMetadata;
        let old_metadata: Vec<FileMetadata> = manifest_old.files.iter().map(|f| f.into()).collect();
        let new_metadata: Vec<FileMetadata> = manifest_new.files.iter().map(|f| f.into()).collect();
        
        // Build hash maps for efficient lookup
        let old_files: HashMap<String, _> = old_metadata
            .iter()
            .map(|f| (f.relative_path.clone(), f))
            .collect();

        let new_files: HashMap<String, _> = new_metadata
            .iter()
            .map(|f| (f.relative_path.clone(), f))
            .collect();

        // Build hash-to-path maps for move detection
        let old_hash_to_paths: HashMap<String, Vec<String>> = {
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for file in &old_metadata {
                map.entry(file.hash.clone())
                    .or_insert_with(Vec::new)
                    .push(file.relative_path.clone());
            }
            map
        };

        let new_hash_to_paths: HashMap<String, Vec<String>> = {
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for file in &new_metadata {
                map.entry(file.hash.clone())
                    .or_insert_with(Vec::new)
                    .push(file.relative_path.clone());
            }
            map
        };

        let mut files_added = Vec::new();
        let mut files_removed = Vec::new();
        let mut files_modified = Vec::new();
        let mut files_moved = Vec::new();
        let mut files_unchanged_count = 0;

        let mut processed_paths = HashSet::new();
        let mut size_added = 0u64;
        let mut size_removed = 0u64;

        // Find added and modified files
        for (path, new_file) in &new_files {
            if let Some(old_file) = old_files.get(path) {
                // File exists in both backups
                if old_file.hash == new_file.hash {
                    // File unchanged
                    files_unchanged_count += 1;
                } else {
                    // File modified
                    let size_delta = new_file.size as i64 - old_file.size as i64;
                    if size_delta > 0 {
                        size_added += size_delta as u64;
                    } else {
                        size_removed += (-size_delta) as u64;
                    }

                    files_modified.push(FileModification {
                        path: path.clone(),
                        size_old: old_file.size,
                        size_new: new_file.size,
                        size_delta,
                        hash_old: old_file.hash.clone(),
                        hash_new: new_file.hash.clone(),
                        compression_changed: old_file.compressed != new_file.compressed,
                    });
                }
                processed_paths.insert(path.clone());
            } else {
                // Check if this is a moved file (same hash, different path)
                if let Some(old_paths) = old_hash_to_paths.get(&new_file.hash) {
                    // Find an old path that hasn't been processed
                    let moved = old_paths.iter().find(|old_path| {
                        !processed_paths.contains(*old_path) && !new_files.contains_key(*old_path)
                    });

                    if let Some(old_path) = moved {
                        // This is a move/rename
                        files_moved.push(FileMove {
                            path_old: old_path.clone(),
                            path_new: path.clone(),
                            size: new_file.size,
                            hash: new_file.hash.clone(),
                        });
                        processed_paths.insert(old_path.clone());
                        processed_paths.insert(path.clone());
                        continue;
                    }
                }

                // File added (not a move)
                size_added += new_file.size;
                files_added.push(FileDiff {
                    path: path.clone(),
                    size: new_file.size,
                    hash: new_file.hash.clone(),
                    compressed: new_file.compressed,
                });
                processed_paths.insert(path.clone());
            }
        }

        // Find removed files
        for (path, old_file) in &old_files {
            if processed_paths.contains(path) {
                continue; // Already processed (move or modify)
            }

            if !new_files.contains_key(path) {
                // File removed
                size_removed += old_file.size;
                files_removed.push(FileDiff {
                    path: path.clone(),
                    size: old_file.size,
                    hash: old_file.hash.clone(),
                    compressed: old_file.compressed,
                });
            }
        }

        let size_delta = size_added as i64 - size_removed as i64;

        // Sort results by path for consistent output
        files_added.sort_by(|a, b| a.path.cmp(&b.path));
        files_removed.sort_by(|a, b| a.path.cmp(&b.path));
        files_modified.sort_by(|a, b| a.path.cmp(&b.path));
        files_moved.sort_by(|a, b| a.path_old.cmp(&b.path_old));

        // Calculate counts before moving vectors
        let files_added_count = files_added.len();
        let files_removed_count = files_removed.len();
        let files_modified_count = files_modified.len();
        let files_moved_count = files_moved.len();

        BackupDiff {
            backup_id_old: manifest_old.backup_id.clone(),
            backup_id_new: manifest_new.backup_id.clone(),
            timestamp_old: manifest_old.timestamp,
            timestamp_new: manifest_new.timestamp,
            files_added,
            files_removed,
            files_modified,
            files_moved,
            summary: DiffSummary {
                files_added_count,
                files_removed_count,
                files_modified_count,
                files_moved_count,
                files_unchanged_count,
                size_added,
                size_removed,
                size_delta,
            },
        }
    }

    /// Check if there are any differences
    pub fn has_changes(&self) -> bool {
        !self.files_added.is_empty()
            || !self.files_removed.is_empty()
            || !self.files_modified.is_empty()
            || !self.files_moved.is_empty()
    }

    /// Get total number of changes
    pub fn total_changes(&self) -> usize {
        self.files_added.len()
            + self.files_removed.len()
            + self.files_modified.len()
            + self.files_moved.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::direct_upload::FileEntry;
    use std::path::PathBuf;

    fn create_test_manifest(backup_id: &str, files: Vec<FileEntry>) -> BackupManifest {
        BackupManifest {
            backup_id: backup_id.to_string(),
            timestamp: Utc::now(),
            file_count: files.len(),
            total_size: files.iter().map(|f| f.size).sum(),
            files,
            source_paths: vec![],
        }
    }

    fn create_file_entry(path: &str, size: u64, hash: &str, compressed: bool) -> FileEntry {
        FileEntry {
            local_path: PathBuf::from(path),
            remote_path: format!("/backups/{}", path),
            size,
            hash: hash.to_string(),
            compressed,
            encrypted: true,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_diff_no_changes() {
        let files = vec![
            create_file_entry("file1.txt", 100, "hash1", false),
            create_file_entry("file2.txt", 200, "hash2", false),
        ];

        let manifest1 = create_test_manifest("backup1", files.clone());
        let manifest2 = create_test_manifest("backup2", files);

        let diff = BackupDiff::compare(&manifest1, &manifest2);

        assert!(!diff.has_changes());
        assert_eq!(diff.total_changes(), 0);
        assert_eq!(diff.summary.files_unchanged_count, 2);
    }

    #[test]
    fn test_diff_file_added() {
        let files_old = vec![create_file_entry("file1.txt", 100, "hash1", false)];

        let files_new = vec![
            create_file_entry("file1.txt", 100, "hash1", false),
            create_file_entry("file2.txt", 200, "hash2", false),
        ];

        let manifest_old = create_test_manifest("backup1", files_old);
        let manifest_new = create_test_manifest("backup2", files_new);

        let diff = BackupDiff::compare(&manifest_old, &manifest_new);

        assert!(diff.has_changes());
        assert_eq!(diff.files_added.len(), 1);
        assert_eq!(diff.files_added[0].path, "file2.txt");
        assert_eq!(diff.files_added[0].size, 200);
        assert_eq!(diff.summary.size_added, 200);
    }

    #[test]
    fn test_diff_file_removed() {
        let files_old = vec![
            create_file_entry("file1.txt", 100, "hash1", false),
            create_file_entry("file2.txt", 200, "hash2", false),
        ];

        let files_new = vec![create_file_entry("file1.txt", 100, "hash1", false)];

        let manifest_old = create_test_manifest("backup1", files_old);
        let manifest_new = create_test_manifest("backup2", files_new);

        let diff = BackupDiff::compare(&manifest_old, &manifest_new);

        assert!(diff.has_changes());
        assert_eq!(diff.files_removed.len(), 1);
        assert_eq!(diff.files_removed[0].path, "file2.txt");
        assert_eq!(diff.files_removed[0].size, 200);
        assert_eq!(diff.summary.size_removed, 200);
    }

    #[test]
    fn test_diff_file_modified() {
        let files_old = vec![create_file_entry("file1.txt", 100, "hash1", false)];

        let files_new = vec![create_file_entry("file1.txt", 150, "hash2", false)];

        let manifest_old = create_test_manifest("backup1", files_old);
        let manifest_new = create_test_manifest("backup2", files_new);

        let diff = BackupDiff::compare(&manifest_old, &manifest_new);

        assert!(diff.has_changes());
        assert_eq!(diff.files_modified.len(), 1);
        assert_eq!(diff.files_modified[0].path, "file1.txt");
        assert_eq!(diff.files_modified[0].size_old, 100);
        assert_eq!(diff.files_modified[0].size_new, 150);
        assert_eq!(diff.files_modified[0].size_delta, 50);
        assert_eq!(diff.summary.size_added, 50);
    }

    #[test]
    fn test_diff_file_moved() {
        let files_old = vec![create_file_entry("old/file1.txt", 100, "hash1", false)];

        let files_new = vec![create_file_entry("new/file1.txt", 100, "hash1", false)];

        let manifest_old = create_test_manifest("backup1", files_old);
        let manifest_new = create_test_manifest("backup2", files_new);

        let diff = BackupDiff::compare(&manifest_old, &manifest_new);

        assert!(diff.has_changes());
        assert_eq!(diff.files_moved.len(), 1);
        assert_eq!(diff.files_moved[0].path_old, "old/file1.txt");
        assert_eq!(diff.files_moved[0].path_new, "new/file1.txt");
        assert_eq!(diff.files_moved[0].hash, "hash1");
        assert_eq!(diff.summary.size_added, 0);
        assert_eq!(diff.summary.size_removed, 0);
    }

    #[test]
    fn test_diff_complex() {
        let files_old = vec![
            create_file_entry("file1.txt", 100, "hash1", false),
            create_file_entry("file2.txt", 200, "hash2", false),
            create_file_entry("file3.txt", 300, "hash3", false),
            create_file_entry("old_name.txt", 400, "hash4", false),
        ];

        let files_new = vec![
            create_file_entry("file1.txt", 100, "hash1", false), // Unchanged
            create_file_entry("file2.txt", 250, "hash2_new", false), // Modified
            create_file_entry("file4.txt", 500, "hash5", false),  // Added
            create_file_entry("new_name.txt", 400, "hash4", false), // Moved
        ];

        let manifest_old = create_test_manifest("backup1", files_old);
        let manifest_new = create_test_manifest("backup2", files_new);

        let diff = BackupDiff::compare(&manifest_old, &manifest_new);

        assert!(diff.has_changes());
        assert_eq!(diff.summary.files_unchanged_count, 1); // file1.txt
        assert_eq!(diff.files_modified.len(), 1); // file2.txt
        assert_eq!(diff.files_removed.len(), 1); // file3.txt
        assert_eq!(diff.files_added.len(), 1); // file4.txt
        assert_eq!(diff.files_moved.len(), 1); // old_name.txt -> new_name.txt
        assert_eq!(diff.total_changes(), 4);
    }
}
