//! Vault cryptography — key derivation and encryption.

use crate::error::{VaultError, VaultResult};
use std::io::{Read, Write};
use zeroize::Zeroize;

/// A derived encryption key — zeroized on drop.
#[derive(zeroize::Zeroize)]
#[zeroize(drop)]
pub struct SecretKey(zeroize::Zeroizing<[u8; 32]>);

// SECURITY: Never log the derived key
impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKey([REDACTED])")
    }
}

pub struct CryptoBackend;

impl CryptoBackend {
    /// Derive 32-byte key from master password + salt using Argon2id
    pub fn derive_key(master_password: &str, salt: &[u8; 32]) -> VaultResult<SecretKey> {
        use argon2::{Argon2, Params, Algorithm, Version};
        let params = Params::new(65536, 3, 4, Some(32)).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = [0u8; 32];
        argon2.hash_password_into(master_password.as_bytes(), salt, &mut key).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        Ok(SecretKey(zeroize::Zeroizing::new(key)))
    }

    /// Encrypt plaintext with age using the derived key as passphrase
    pub fn encrypt(plaintext: &[u8], key: &SecretKey) -> VaultResult<Vec<u8>> {
        use age::secrecy::SecretString;
        let passphrase = SecretString::new(hex::encode(key.0.as_ref()));
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);
        let mut ciphertext = vec![];
        let mut writer = encryptor.wrap_output(&mut ciphertext).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        writer.write_all(plaintext).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        writer.finish().map_err(|e| VaultError::CryptoError(e.to_string()))?;
        Ok(ciphertext)
    }

    /// Decrypt age ciphertext with the derived key
    pub fn decrypt(ciphertext: &[u8], key: &SecretKey) -> VaultResult<zeroize::Zeroizing<Vec<u8>>> {
        use age::secrecy::SecretString;
        let passphrase = SecretString::new(hex::encode(key.0.as_ref()));
        let decryptor = age::Decryptor::new(ciphertext).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        let mut decryptor = match decryptor {
            age::Decryptor::Passphrase(d) => d,
            _ => return Err(VaultError::DecryptionFailed("wrong encryptor type".into())),
        };
        let mut plaintext = vec![];
        let mut reader = decryptor.decrypt(&passphrase, None).map_err(|e| VaultError::DecryptionFailed(e.to_string()))?;
        reader.read_to_end(&mut plaintext).map_err(|e| VaultError::CryptoError(e.to_string()))?;
        Ok(zeroize::Zeroizing::new(plaintext))
    }
}
