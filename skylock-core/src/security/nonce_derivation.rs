//! Deterministic nonce derivation using HKDF
//! 
//! This module provides a secure, deterministic nonce generation mechanism
//! that guarantees uniqueness without requiring persistent state.
//! 
//! **Nonce Strategy**:
//! - AES-256-GCM: 96-bit (12-byte) nonces derived via HKDF
//! - XChaCha20-Poly1305: 192-bit (24-byte) nonces derived via HKDF
//! 
//! **Derivation**: `nonce = HKDF(key=block_key, salt=block_hash, info=chunk_index||"skylock-nonce")`

use hkdf::Hkdf;
use sha2::Sha256;
use crate::{Result, SkylockError};

/// Derive a 96-bit (12-byte) nonce for AES-256-GCM
pub fn derive_aes_gcm_nonce(
    block_key: &[u8],
    block_hash: &[u8],
    chunk_index: u64,
) -> Result<[u8; 12]> {
    let hkdf = Hkdf::<Sha256>::new(Some(block_hash), block_key);
    
    // Info combines chunk index and context string
    let info = format!("{}||skylock-nonce-gcm", chunk_index);
    
    let mut nonce = [0u8; 12];
    hkdf.expand(info.as_bytes(), &mut nonce)
        .map_err(|e| SkylockError::Encryption(format!("Nonce derivation failed: {}", e)))?;
    
    Ok(nonce)
}

/// Derive a 192-bit (24-byte) nonce for XChaCha20-Poly1305
pub fn derive_xchacha_nonce(
    block_key: &[u8],
    block_hash: &[u8],
    chunk_index: u64,
) -> Result<[u8; 24]> {
    let hkdf = Hkdf::<Sha256>::new(Some(block_hash), block_key);
    
    // Info combines chunk index and context string
    let info = format!("{}||skylock-nonce-xchacha", chunk_index);
    
    let mut nonce = [0u8; 24];
    hkdf.expand(info.as_bytes(), &mut nonce)
        .map_err(|e| SkylockError::Encryption(format!("Nonce derivation failed: {}", e)))?;
    
    Ok(nonce)
}

/// Derive a generic nonce of arbitrary length (for future algorithms)
pub fn derive_nonce(
    block_key: &[u8],
    block_hash: &[u8],
    chunk_index: u64,
    nonce_size: usize,
    algorithm: &str,
) -> Result<Vec<u8>> {
    if nonce_size == 0 || nonce_size > 255 {
        return Err(SkylockError::Encryption(
            format!("Invalid nonce size: {} (must be 1-255 bytes)", nonce_size)
        ));
    }
    
    let hkdf = Hkdf::<Sha256>::new(Some(block_hash), block_key);
    
    // Info combines chunk index, algorithm, and context
    let info = format!("{}||skylock-nonce-{}", chunk_index, algorithm);
    
    let mut nonce = vec![0u8; nonce_size];
    hkdf.expand(info.as_bytes(), &mut nonce)
        .map_err(|e| SkylockError::Encryption(format!("Nonce derivation failed: {}", e)))?;
    
    Ok(nonce)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_aes_gcm_nonce_deterministic() {
        let block_key = b"test_block_key_32_bytes_long!!!";
        let block_hash = b"test_block_hash_value";
        let chunk_idx = 42;
        
        let nonce1 = derive_aes_gcm_nonce(block_key, block_hash, chunk_idx).unwrap();
        let nonce2 = derive_aes_gcm_nonce(block_key, block_hash, chunk_idx).unwrap();
        
        // Same inputs should produce same nonce (deterministic)
        assert_eq!(nonce1, nonce2);
        assert_eq!(nonce1.len(), 12);
    }
    
    #[test]
    fn test_nonce_uniqueness() {
        let block_key = b"test_block_key_32_bytes_long!!!";
        let block_hash = b"test_block_hash_value";
        
        // Different chunk indices should produce different nonces
        let nonce1 = derive_aes_gcm_nonce(block_key, block_hash, 0).unwrap();
        let nonce2 = derive_aes_gcm_nonce(block_key, block_hash, 1).unwrap();
        let nonce3 = derive_aes_gcm_nonce(block_key, block_hash, 2).unwrap();
        
        assert_ne!(nonce1, nonce2);
        assert_ne!(nonce2, nonce3);
        assert_ne!(nonce1, nonce3);
    }
    
    #[test]
    fn test_xchacha_nonce_size() {
        let block_key = b"test_block_key_32_bytes_long!!!";
        let block_hash = b"test_block_hash_value";
        
        let nonce = derive_xchacha_nonce(block_key, block_hash, 0).unwrap();
        assert_eq!(nonce.len(), 24);
    }
    
    #[test]
    fn test_different_keys_different_nonces() {
        let block_key1 = b"test_block_key1_32_bytes_long!!";
        let block_key2 = b"test_block_key2_32_bytes_long!!";
        let block_hash = b"test_block_hash_value";
        
        let nonce1 = derive_aes_gcm_nonce(block_key1, block_hash, 0).unwrap();
        let nonce2 = derive_aes_gcm_nonce(block_key2, block_hash, 0).unwrap();
        
        // Different keys should produce different nonces
        assert_ne!(nonce1, nonce2);
    }
    
    #[test]
    fn test_different_hashes_different_nonces() {
        let block_key = b"test_block_key_32_bytes_long!!!";
        let block_hash1 = b"test_block_hash_value1";
        let block_hash2 = b"test_block_hash_value2";
        
        let nonce1 = derive_aes_gcm_nonce(block_key, block_hash1, 0).unwrap();
        let nonce2 = derive_aes_gcm_nonce(block_key, block_hash2, 0).unwrap();
        
        // Different salts should produce different nonces
        assert_ne!(nonce1, nonce2);
    }
}
