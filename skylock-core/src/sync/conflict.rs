use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use crate::Result;
use super::{FileState, ConflictResolution};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub path: PathBuf,
    pub local_state: FileState,
    pub remote_state: FileState,
    pub detected_at: DateTime<Utc>,
    pub resolution: Option<ConflictResolution>,
}

#[derive(Debug)]
pub struct ConflictResolver {
    default_strategy: ConflictResolution,
}

impl ConflictResolver {
    pub fn new(default_strategy: ConflictResolution) -> Self {
        Self {
            default_strategy,
        }
    }

    pub async fn resolve(&self, conflict: &mut Conflict) -> Result<FileState> {
        let resolution = conflict.resolution.clone().unwrap_or_else(|| self.default_strategy.clone());

        match resolution {
            ConflictResolution::KeepNewest => {
                if conflict.local_state.modified >= conflict.remote_state.modified {
                    Ok(conflict.local_state.clone())
                } else {
                    Ok(conflict.remote_state.clone())
                }
            },
            ConflictResolution::KeepOldest => {
                if conflict.local_state.modified <= conflict.remote_state.modified {
                    Ok(conflict.local_state.clone())
                } else {
                    Ok(conflict.remote_state.clone())
                }
            },
            ConflictResolution::KeepBoth => {
                // Create a new path for the conflicted file
                let new_path = self.generate_conflict_path(&conflict.path);
                let mut new_state = conflict.local_state.clone();
                new_state.path = new_path;
                Ok(new_state)
            },
            ConflictResolution::Manual => {
                // Return the local state and let the caller handle manual resolution
                Ok(conflict.local_state.clone())
            },
        }
    }

    fn generate_conflict_path(&self, original_path: &PathBuf) -> PathBuf {
        let extension = original_path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");

        let stem = original_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("");

        let parent = original_path.parent().unwrap_or(std::path::Path::new(""));

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let new_name = format!("{}_conflict_{}.{}", stem, timestamp, extension);

        parent.join(new_name)
    }
}
