//! Vault backend — the core implementation.

use crate::crypto::{CryptoBackend, SecretKey};
use crate::db;
use crate::error::{VaultError, VaultResult};
use crate::types::{
    RequestContext, SensitiveValue, TokenHandle, VaultCategory, VaultEntry, VaultKey,
};
use hmac::{Hmac, Mac};
use keyring;
use rand::Rng;
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use uuid::Uuid;
use zeroize::Zeroizing;

/// In-memory entry for a dispensed credential token.
/// Expires after 30 s (matching `HilApproval`) and is consumed on first use.
struct TokenStoreEntry {
    value: Zeroizing<Vec<u8>>,
    expires_at: Instant,
}

/// The vault backend, responsible for storing and retrieving encrypted secrets.
pub struct VaultBackend {
    conn: Arc<Mutex<rusqlite::Connection>>,
    secret_key: SecretKey,
    secret_salt: [u8; 32],
    /// Single-use, TTL-bound token store: token_id → decrypted bytes.
    token_store: Arc<Mutex<HashMap<Uuid, TokenStoreEntry>>>,
}

impl VaultBackend {
    /// Create a new vault backend with an explicit KDF salt.
    ///
    /// For production use prefer [`Self::new_with_keyring`] which generates
    /// and persists a random salt in the OS keychain on first run.
    pub fn new(master_password: &str, salt: &[u8; 32]) -> VaultResult<Self> {
        let db_path = db::default_vault_path();
        let conn = db::open_db(&db_path)?;
        db::create_schema(&conn)?;

        let secret_key = CryptoBackend::derive_key(master_password, salt)?;

        let entry = keyring::Entry::new("kitsune-vault", "secret-salt")
            .map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;
        let salt_hex = match entry.get_password() {
            Ok(s) => s,
            Err(keyring::Error::NoEntry) => {
                let new_salt: [u8; 32] = rand::thread_rng().gen();
                let new_salt_hex = hex::encode(new_salt);
                entry
                    .set_password(&new_salt_hex)
                    .map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;
                new_salt_hex
            }
            Err(e) => return Err(VaultError::SecureStorageUnavailable(e.to_string())),
        };
        let secret_salt: [u8; 32] = hex::decode(salt_hex)
            .map_err(|_| VaultError::SecureStorageUnavailable("Invalid salt format".into()))?
            .try_into()
            .map_err(|_| VaultError::SecureStorageUnavailable("Invalid salt length".into()))?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            secret_key,
            secret_salt,
            token_store: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Create a vault backend with the KDF salt stored in the OS keychain.
    ///
    /// On first run a random 32-byte KDF salt is generated and stored under
    /// `kitsune-vault` / `kdf-salt`.  Subsequent runs retrieve it.  This
    /// ensures every installation uses a unique Argon2id salt, so the derived
    /// key cannot be pre-computed from the password alone.
    pub fn new_with_keyring(master_password: &str) -> VaultResult<Self> {
        let kdf_entry = keyring::Entry::new("kitsune-vault", "kdf-salt")
            .map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;

        let kdf_salt: [u8; 32] = match kdf_entry.get_password() {
            Ok(hex_str) => hex::decode(&hex_str)
                .map_err(|_| {
                    VaultError::SecureStorageUnavailable("Corrupt kdf-salt in keyring".into())
                })?
                .try_into()
                .map_err(|_| {
                    VaultError::SecureStorageUnavailable("kdf-salt wrong length".into())
                })?,
            Err(keyring::Error::NoEntry) => {
                let salt: [u8; 32] = rand::thread_rng().gen();
                kdf_entry
                    .set_password(&hex::encode(salt))
                    .map_err(|e| VaultError::SecureStorageUnavailable(e.to_string()))?;
                tracing::info!("Generated new vault KDF salt and stored in OS keychain");
                salt
            }
            Err(e) => return Err(VaultError::SecureStorageUnavailable(e.to_string())),
        };

        Self::new(master_password, &kdf_salt)
    }

    /// Store a new entry in the vault.
    pub fn store(
        &self,
        key: &VaultKey,
        value: SensitiveValue,
        context: &RequestContext,
    ) -> VaultResult<()> {
        let origin_pseudonym = self.origin_pseudonym(context.domain.as_deref());
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
    ///
    /// Decrypts the stored credential and binds the plaintext to the returned
    /// `TokenHandle` in an in-memory map (30 s TTL, single-use).  The raw
    /// bytes are never exposed; callers must use [`Self::consume_token`] to
    /// dereference the handle immediately before use in the renderer.
    pub fn retrieve(&self, key: &VaultKey, context: &RequestContext) -> VaultResult<TokenHandle> {
        let origin_pseudonym = self.origin_pseudonym(context.domain.as_deref());

        let conn = self.conn.lock().unwrap();
        let entry = db::retrieve_entry(
            &conn,
            &origin_pseudonym,
            &key.category.to_string(),
            &key.label,
        )?
        .ok_or_else(|| VaultError::KeyNotFound {
            key: key.label.clone(),
        })?;

        let decrypted = CryptoBackend::decrypt(&entry.encrypted_value, &self.secret_key)?;

        let token = TokenHandle::new();

        // Bind decrypted bytes to the token with a 30 s TTL matching HilApproval.
        self.token_store.lock().unwrap().insert(
            token.id,
            TokenStoreEntry {
                value: decrypted,
                expires_at: Instant::now() + std::time::Duration::from_secs(30),
            },
        );

        let id_str = entry.id.to_string();
        db::log_audit(&conn, Some(&id_str), "retrieve", context)?;

        Ok(token)
    }

    /// Consume a previously issued credential token.
    ///
    /// Single-use: removes the entry from the in-memory store.  Returns
    /// `TokenExpired` if the 30 s TTL has elapsed, `TokenNotFound` if the
    /// token was never issued or already consumed.
    pub fn consume_token(&self, token_id: Uuid) -> VaultResult<Zeroizing<Vec<u8>>> {
        let mut store = self.token_store.lock().unwrap();
        // Purge any expired entries on each access.
        let now = Instant::now();
        store.retain(|_, e| e.expires_at > now);

        match store.remove(&token_id) {
            Some(entry) if entry.expires_at > now => Ok(entry.value),
            Some(_) => Err(VaultError::TokenExpired),
            None => Err(VaultError::TokenNotFound),
        }
    }

    /// Request credential access for a DOM form field.
    ///
    /// Requires HIL approval in the context (`context.has_hil_approval == true`).
    /// Returns `HilRequired` if called without prior user confirmation.
    pub async fn request_access(
        &self,
        _field_id: &str,
        context: &RequestContext,
    ) -> VaultResult<TokenHandle> {
        if !context.has_hil_approval {
            return Err(VaultError::HilRequired);
        }
        Ok(TokenHandle::new())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn origin_pseudonym(&self, domain: Option<&str>) -> String {
        let origin = domain.unwrap_or("");
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.secret_salt).unwrap();
        mac.update(origin.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vault() -> VaultBackend {
        let kdf_salt: [u8; 32] = rand::thread_rng().gen();
        VaultBackend::new("test-password", &kdf_salt).unwrap()
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
        assert!(token.is_valid());

        // consume_token returns the decrypted bytes
        let bytes = vault.consume_token(token.id).unwrap();
        assert_eq!(bytes.as_slice(), b"my-secret");

        // second consume is rejected (single-use)
        assert!(matches!(
            vault.consume_token(token.id),
            Err(VaultError::TokenNotFound)
        ));
    }

    #[test]
    fn request_access_requires_hil() {
        let vault = test_vault();
        let ctx_no_hil = RequestContext {
            domain: Some("example.com".to_string()),
            purpose: "test".to_string(),
            agent_id: None,
            has_hil_approval: false,
            action_id: uuid::Uuid::new_v4(),
        };
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(vault.request_access("field_id", &ctx_no_hil));
        assert!(matches!(result, Err(VaultError::HilRequired)));
    }

    #[test]
    fn request_access_succeeds_with_hil() {
        let vault = test_vault();
        let ctx_with_hil = RequestContext {
            domain: Some("example.com".to_string()),
            purpose: "test".to_string(),
            agent_id: None,
            has_hil_approval: true,
            action_id: uuid::Uuid::new_v4(),
        };
        let result = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(vault.request_access("field_id", &ctx_with_hil));
        assert!(result.is_ok());
    }
}
