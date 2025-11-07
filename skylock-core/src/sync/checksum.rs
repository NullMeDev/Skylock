use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use sha2::{Sha256, Digest};
use xxhash_rust::xxh3::xxh3_64;
use crate::Result;
use super::ChecksumAlgorithm;

pub struct Checksummer;

impl Checksummer {
    pub async fn calculate(&self, path: &PathBuf, algorithm: &ChecksumAlgorithm) -> Result<String> {
        let mut file = File::open(path).await?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).await?;

        Ok(match algorithm {
            ChecksumAlgorithm::MD5 => self.calculate_md5(&buffer),
            ChecksumAlgorithm::SHA256 => self.calculate_sha256(&buffer),
            ChecksumAlgorithm::XXHash => self.calculate_xxhash(&buffer),
        })
    }

    fn calculate_md5(&self, data: &[u8]) -> String {
        let digest = md5::compute(data);
        format!("{:x}", digest)
    }

    fn calculate_sha256(&self, data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        format!("{:x}", hasher.finalize())
    }

    fn calculate_xxhash(&self, data: &[u8]) -> String {
        format!("{:x}", xxh3_64(data))
    }
}
