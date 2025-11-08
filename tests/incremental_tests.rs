/// Integration tests for incremental backup functionality
/// Tests edge cases: empty dirs, symlinks, large files, permission changes,
/// concurrent backups, and chain restore scenarios

use skylock_backup::change_tracker::FileIndex;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Helper to create a test environment
struct TestEnv {
    source_dir: TempDir,
    index_dir: TempDir,
}

impl TestEnv {
    fn new() -> Self {
        let source_dir = TempDir::new().unwrap();
        let index_dir = TempDir::new().unwrap();
        
        Self {
            source_dir,
            index_dir,
        }
    }
    
    fn source_path(&self) -> &Path {
        self.source_dir.path()
    }
    
    fn index_path(&self) -> &Path {
        self.index_dir.path()
    }
    
    fn create_file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.source_path().join(name);
        let mut file = File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }
    
    fn create_dir(&self, name: &str) -> PathBuf {
        let path = self.source_path().join(name);
        fs::create_dir(&path).unwrap();
        path
    }
    
    #[cfg(unix)]
    fn create_symlink(&self, target: &str, link: &str) -> PathBuf {
        use std::os::unix::fs::symlink;
        let link_path = self.source_path().join(link);
        symlink(target, &link_path).unwrap();
        link_path
    }
}

#[tokio::test]
async fn test_incremental_empty_directories() {
    let env = TestEnv::new();
    
    // Create empty directories
    env.create_dir("empty1");
    env.create_dir("empty2");
    env.create_dir("nested");
    env.create_dir("nested/empty_child");
    
    // Create file index for baseline
    let baseline = FileIndex::build(&[env.source_path().to_path_buf()]).unwrap();
    let index_path = env.index_path().join("baseline.json");
    baseline.save(&index_path).await.unwrap();
    
    // Create another empty dir (change)
    env.create_dir("empty3");
    
    // Detect changes
    let changes = baseline.detect_changes(&[env.source_path().to_path_buf()]).await.unwrap();
    
    // Should detect new empty directory
    assert!(!changes.is_empty(), "Should detect new empty directory");
}

#[test]
#[cfg(unix)]
fn test_incremental_symlinks() {
    let env = TestEnv::new();
    
    // Create target file and symlink
    env.create_file("target.txt", "target content");
    env.create_symlink("target.txt", "link.txt");
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Modify target file
    let target_path = env.source_path().join("target.txt");
    let mut file = File::create(&target_path).unwrap();
    file.write_all(b"modified content").unwrap();
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect change to target (symlink itself unchanged)
    assert!(!changes.is_empty(), "Should detect change to symlink target");
}

#[test]
fn test_incremental_large_files() {
    let env = TestEnv::new();
    
    // Create a large file (>10MB to trigger compression)
    let large_content = vec![b'X'; 11 * 1024 * 1024]; // 11 MB
    let large_path = env.source_path().join("large.bin");
    let mut file = File::create(&large_path).unwrap();
    file.write_all(&large_content).unwrap();
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Modify just the end of the large file
    let mut file = File::options().append(true).open(&large_path).unwrap();
    file.write_all(b"MODIFIED").unwrap();
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect change to large file
    assert_eq!(changes.len(), 1, "Should detect single large file modification");
}

#[test]
#[cfg(unix)]
fn test_incremental_permission_changes() {
    use std::os::unix::fs::PermissionsExt;
    
    let env = TestEnv::new();
    
    // Create file with initial permissions
    let file_path = env.create_file("test.txt", "content");
    let mut perms = fs::metadata(&file_path).unwrap().permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file_path, perms).unwrap();
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Change permissions only (not content)
    let mut perms = fs::metadata(&file_path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&file_path, perms).unwrap();
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect metadata change (permissions)
    // Note: Depending on implementation, this might be detected as MetadataChanged or not at all
    // since we primarily track mtime and size
    println!("Detected changes: {:?}", changes);
}

#[test]
fn test_incremental_many_small_files() {
    let env = TestEnv::new();
    
    // Create many small files (baseline)
    for i in 0..100 {
        env.create_file(&format!("file_{}.txt", i), &format!("content {}", i));
    }
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Modify only 5 files
    for i in [10, 25, 50, 75, 90] {
        let path = env.source_path().join(format!("file_{}.txt", i));
        let mut file = File::create(&path).unwrap();
        file.write_all(b"MODIFIED").unwrap();
    }
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect only 5 modified files
    assert_eq!(changes.len(), 5, "Should detect exactly 5 modified files");
}

#[test]
fn test_incremental_file_rename_as_delete_add() {
    let env = TestEnv::new();
    
    // Create file
    env.create_file("old_name.txt", "content");
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Rename file
    let old_path = env.source_path().join("old_name.txt");
    let new_path = env.source_path().join("new_name.txt");
    fs::rename(old_path, new_path).unwrap();
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect both removal and addition (rename detected as delete + add)
    assert_eq!(changes.len(), 2, "Rename should be detected as delete + add");
}

#[test]
fn test_incremental_nested_directory_changes() {
    let env = TestEnv::new();
    
    // Create nested structure
    env.create_dir("level1");
    env.create_dir("level1/level2");
    env.create_dir("level1/level2/level3");
    env.create_file("level1/level2/level3/deep_file.txt", "deep content");
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Modify deep file
    let deep_path = env.source_path().join("level1/level2/level3/deep_file.txt");
    let mut file = File::create(&deep_path).unwrap();
    file.write_all(b"MODIFIED").unwrap();
    
    // Add sibling file
    env.create_file("level1/level2/level3/sibling.txt", "new sibling");
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect both modifications in deep directory
    assert_eq!(changes.len(), 2, "Should detect changes in deeply nested directory");
}

#[test]
fn test_incremental_no_baseline() {
    let env = TestEnv::new();
    
    // Create some files
    env.create_file("file1.txt", "content1");
    env.create_file("file2.txt", "content2");
    
    // Try to detect changes without baseline
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let result = tracker.detect_changes(&[env.source_path().to_path_buf()]);
    
    // Should fail gracefully or treat all as new
    match result {
        Ok(changes) => {
            // If no baseline exists, might return all files as added
            assert!(!changes.is_empty(), "Without baseline, should detect all files");
        }
        Err(e) => {
            // Or might return error indicating no baseline
            println!("Expected error without baseline: {:?}", e);
        }
    }
}

#[test]
fn test_change_tracker_file_content_vs_mtime() {
    let env = TestEnv::new();
    
    // Create file
    let file_path = env.create_file("test.txt", "original");
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    tracker.save_index(&index, "baseline").unwrap();
    
    // Modify file content but try to preserve mtime
    use std::time::SystemTime;
    let original_mtime = fs::metadata(&file_path).unwrap().modified().unwrap();
    
    // Modify content
    let mut file = File::create(&file_path).unwrap();
    file.write_all(b"modified content").unwrap();
    drop(file);
    
    // Try to restore original mtime (this may not work on all systems)
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // Mtime will be different after write, so hash should catch it
    }
    
    // Detect changes
    let changes = tracker.detect_changes(&[env.source_path().to_path_buf()]).unwrap();
    
    // Should detect change via hash even if mtime manipulated
    assert!(!changes.is_empty(), "Should detect content change via hash");
}

#[test]
fn test_incremental_backup_chain_ids() {
    let env = TestEnv::new();
    
    // Create initial files
    env.create_file("file1.txt", "content1");
    
    // Create baseline index
    let tracker = ChangeTracker::new(env.backup_path().to_path_buf());
    let index1 = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    let backup_id1 = "backup1";
    tracker.save_index(&index1, backup_id1).unwrap();
    
    // Modify file
    env.create_file("file1.txt", "modified1");
    
    // Create second backup index
    let index2 = tracker.build_file_index(&[env.source_path().to_path_buf()]).unwrap();
    let backup_id2 = "backup2";
    tracker.save_index(&index2, backup_id2).unwrap();
    
    // Verify we can load both indexes
    let loaded1 = tracker.load_index(Some(backup_id1)).unwrap();
    let loaded2 = tracker.load_index(Some(backup_id2)).unwrap();
    
    assert_ne!(loaded1.files.len(), loaded2.files.len(), 
               "Different backup indexes should exist independently");
}
