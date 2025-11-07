use std::path::PathBuf;
use std::collections::HashMap;
use tokio::sync::mpsc;
use chrono::Utc;
use crate::{
    Result,
    error_types::{Error, ErrorCategory, ErrorSeverity, SystemError},
};
use super::{FileState, ConflictResolution, Conflict};
use super::conflict::ConflictResolver;

pub struct BatchProcessor {
    resolver: ConflictResolver,
    error_tx: mpsc::Sender<SystemError>,
    remote_states: HashMap<PathBuf, FileState>,
}

impl BatchProcessor {
    pub fn new(
        default_resolution: ConflictResolution,
        error_tx: mpsc::Sender<SystemError>,
    ) -> Self {
        Self {
            resolver: ConflictResolver::new(default_resolution),
            error_tx,
            remote_states: HashMap::new(),
        }
    }

    pub async fn process_batch(&mut self, batch: &mut [(PathBuf, FileState)]) -> Result<Vec<(PathBuf, FileState)>> {
        let mut processed = Vec::new();
        let mut conflicts = Vec::new();

        // First pass: Identify conflicts and simple syncs
        for (path, local_state) in batch.iter() {
            if let Some(remote_state) = self.remote_states.get(path) {
                if self.is_conflict(local_state, remote_state) {
                    conflicts.push(Conflict {
                        path: path.clone(),
                        local_state: local_state.clone(),
                        remote_state: remote_state.clone(),
                        detected_at: Utc::now(),
                        resolution: None,
                    });
                } else {
                    // No conflict, can be synced directly
                    processed.push((path.clone(), local_state.clone()));
                }
            } else {
                // New file, can be synced directly
                processed.push((path.clone(), local_state.clone()));
            }
        }

        // Second pass: Resolve conflicts
        for mut conflict in conflicts {
            match self.resolver.resolve(&mut conflict).await {
                Ok(resolved_state) => {
                    processed.push((conflict.path, resolved_state));
                }
                Err(e) => {
                    // Report error but continue processing
                    let system_error = SystemError {
                        code: 1,
                        message: format!("Failed to resolve conflict for {:?}: {}", conflict.path, e),
                        source: "sync::batch".to_string(),
                        timestamp: chrono::Utc::now(),
                        path: Some(conflict.path.clone()),
                    };
                    let _ = self.error_tx.send(system_error).await;
                }
            }
        }

        Ok(processed)
    }

    fn is_conflict(&self, local: &FileState, remote: &FileState) -> bool {
        // Different checksums indicate content conflict
        if local.checksum != remote.checksum {
            // If modified times are different, we have a real conflict
            if local.modified != remote.modified {
                return true;
            }
        }
        false
    }

    pub async fn update_remote_state(&mut self, path: PathBuf, state: FileState) {
        self.remote_states.insert(path, state);
    }

    pub async fn process_deletions(&mut self, local_states: &HashMap<PathBuf, FileState>) -> Result<Vec<PathBuf>> {
        let mut deletions = Vec::new();

        // Find files that exist remotely but not locally
        for (path, _) in self.remote_states.iter() {
            if !local_states.contains_key(path) {
                deletions.push(path.clone());
            }
        }

        Ok(deletions)
    }
}
