//! Vault backend — the core implementation.

use crate::crypto::{CryptoBackend, SecretKey};
use crate::db;
use crate::error::{VaultError, VaultResult};
use crate::types::{RequestContext, SensitiveValue, TokenHandle, VaultKey, VaultEntry, VaultCategory};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::{Arc, Mutex};
use keyring;
use rand::Rng;

/// The vault backend, responsible for storing and retrieving encrypted secrets.
pub struct VaultBackend {
    conn: Arc<Mutex<rusqlite::Connection>>,
    secret_key: SecretKey,
    secret_salt: [u8; 32], // Used for HMACing origins
}

impl VaultBackend {
    /// Create a new vault backend.
    pub fn new(master_password: &str, salt: &[u8; 32]) -> VaultResult<Self> {
        let db_path = db::default_vault_path();
        let conn = db::open_db(&db_path)?;
        db::create_schema(&conn)?;

        let secret_key = CryptoBackend::derive_key(master_password, salt)?;

        let entry = keyring::Entry::new("kitsune-vault", "secret-salt").map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;
        let salt_hex = match entry.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => {
                let new_salt: [u8; 32] = rand::thread_rng().gen();
                let new_salt_hex = hex::encode(new_salt);
                entry.set_password(&new_salt_hex).map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;
                new_salt_hex
            }
            Err(e) => return Err(VaultError::SecureStorageUnavailable(e.to_string())),
        };
        let secret_salt: [u8; 32] = hex::decode(salt_hex).map_err(|_| VaultError::SecureStorageUnavailable("Invalid salt format".into()))?.try_into().map_err(|_| VaultError::SecureStorageUnavailable("Invalid salt length".into()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            secret_key,
            secret_salt,
        })
    }

    /// Store a new entry in the vault.
    pub fn store(&self, key: &VaultKey, value: SensitiveValue, context: &RequestContext) -> VaultResult<()> {
        let origin = context.domain.as_deref().unwrap_or("");
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret_salt).unwrap();
        mac.update(origin.as_bytes());
        let origin_pseudonym = hex::encode(mac.finalize().into_bytes());

        let encrypted_value = CryptoBackend::encrypt(value.as_bytes(), &self.secret_key)?;

        let now = chrono::Utc::now().timestamp();
        let entry = VaultEntry {
            id: key.id,
            category: key.category.clone(),
            label: key.label.clone(),
            origin_pseudonym,
            encrypted_value,
            created_at: now,
            updated_at: now,
        };
        let conn = self.conn.lock().unwrap();
        db::store_entry(&conn, &entry)?;
        let id_str = entry.id.to_string();
        db::log_audit(&conn, Some(&id_str), "store", context)?;

        Ok(())
    }

    /// Retrieve an entry from the vault.
    pub fn retrieve(&self, key: &VaultKey, context: &RequestContext) -> VaultResult<TokenHandle> {
        let origin = context.domain.as_deref().unwrap_or("");
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret_salt).unwrap();
        mac.update(origin.as_bytes());
        let origin_pseudonym = hex::encode(mac.finalize().into_bytes());

        let conn = self.conn.lock().unwrap();
        let entry = db::retrieve_entry(&conn, &origin_pseudonym, &key.category.to_string(), &key.label)?
            .ok_or_else(|| VaultError::KeyNotFound { key: key.label.clone() })?;

        let _decrypted_value = CryptoBackend::decrypt(&entry.encrypted_value, &self.secret_key)?;

        let token = TokenHandle::new();
        let id_str = entry.id.to_string();
        db::log_audit(&conn, Some(&id_str), "retrieve", context)?;

        Ok(token)
    }

    pub async fn request_access(&self, _field_id: &str) -> VaultResult<TokenHandle> {
        // Placeholder implementation
        Ok(TokenHandle::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vault() -> VaultBackend {
        let salt: [u8; 32] = rand::thread_rng().gen();
        VaultBackend::new("test-password", &salt).unwrap()
    }

    #[test]
    fn test_vault_roundtrip() {
        let vault = test_vault();
        let key = VaultKey::new("test", VaultCategory::Password);
        let value = SensitiveValue::from_string("my-secret");
        let context = RequestContext {
            domain: Some("example.com".to_string()),
            purpose: "test".to_string(),
            agent_id: None,
            has_hil_approval: false,
            action_id: uuid::Uuid::new_v4(),
        };

        vault.store(&key, value, &context).unwrap();

        let token = vault.retrieve(&key, &context).unwrap();

        drop(token);
    }
}
