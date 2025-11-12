//! HMAC-based integrity verification for backup files
//! 
//! Replaces plain SHA-256 hashing with HMAC-SHA256 to prevent collision attacks.
//! The HMAC key is derived from the encryption key using HKDF.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use hkdf::Hkdf;
use crate::error::{Result, SkylockError};

type HmacSha256 = Hmac<Sha256>;

/// Derive HMAC key from encryption key using HKDF
pub fn derive_hmac_key(encryption_key: &[u8]) -> Result<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(None, encryption_key);
    let mut hmac_key = [0u8; 32];
    hkdf.expand(b"skylock-hmac-v1", &mut hmac_key)
        .map_err(|e| SkylockError::Encryption(format!("HMAC key derivation failed: {}", e)))?;
    Ok(hmac_key)
}

/// Compute HMAC-SHA256 of data
pub fn compute_hmac(data: &[u8], hmac_key: &[u8]) -> Result<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(hmac_key)
        .map_err(|e| SkylockError::Encryption(format!("HMAC initialization failed: {}", e)))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// Verify HMAC-SHA256 in constant time
pub fn verify_hmac(data: &[u8], expected_hmac: &[u8], hmac_key: &[u8]) -> Result<bool> {
    let computed = compute_hmac(data, hmac_key)?;
    
    // Constant-time comparison using subtle crate
    use subtle::ConstantTimeEq;
    Ok(computed.ct_eq(expected_hmac).into())
}

/// Compute file hash for incremental backup (backward compatible)
pub fn compute_file_hash(data: &[u8], use_hmac: bool, hmac_key: Option<&[u8]>) -> Result<String> {
    if use_hmac {
        let key = hmac_key.ok_or_else(|| 
            SkylockError::Encryption("HMAC key required for HMAC mode".to_string())
        )?;
        let hmac = compute_hmac(data, key)?;
        Ok(hex::encode(hmac))
    } else {
        // Legacy SHA-256 mode
        use sha2::Digest;
        let mut hasher = Sha256::new();
        hasher.update(data);
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hmac_derivation() {
        let encryption_key = b"test_encryption_key_32_bytes!!!";
        let hmac_key = derive_hmac_key(encryption_key).unwrap();
        assert_eq!(hmac_key.len(), 32);
    }
    
    #[test]
    fn test_hmac_compute_verify() {
        let data = b"test data";
        let hmac_key = [42u8; 32];
        
        let hmac = compute_hmac(data, &hmac_key).unwrap();
        assert!(verify_hmac(data, &hmac, &hmac_key).unwrap());
        
        // Wrong data should fail
        assert!(!verify_hmac(b"wrong data", &hmac, &hmac_key).unwrap());
    }
    
    #[test]
    fn test_backward_compat() {
        let data = b"test data";
        let hmac_key = [42u8; 32];
        
        // SHA-256 mode
        let sha_hash = compute_file_hash(data, false, None).unwrap();
        assert_eq!(sha_hash.len(), 64); // SHA-256 hex is 64 chars
        
        // HMAC mode
        let hmac_hash = compute_file_hash(data, true, Some(&hmac_key)).unwrap();
        assert_eq!(hmac_hash.len(), 64); // HMAC-SHA256 hex is also 64 chars
        
        // But they should be different
        assert_ne!(sha_hash, hmac_hash);
    }
}
