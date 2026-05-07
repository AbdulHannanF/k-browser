//! SQLite persistence layer for the KitsuneEngine vault.

use crate::error::{VaultError, VaultResult};
use crate::types::{RequestContext, VaultEntry};
use rusqlite::{params, Connection, OpenFlags};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// The default vault database path.
pub fn default_vault_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("kitsune")
        .join("vault.db")
}

/// Open or create the vault database at the given path.
pub fn open_db(path: &Path) -> VaultResult<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            VaultError::StorageError(format!("Cannot create vault directory: {}", e))
        })?;
    }

    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| VaultError::StorageError(format!("Cannot open vault database: {}", e)))?;

    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| VaultError::StorageError(format!("PRAGMA failed: {}", e)))?;

    info!(path = %path.display(), "Vault database opened");
    Ok(conn)
}

/// Create the schema if tables don't exist yet.
pub fn create_schema(conn: &Connection) -> VaultResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS vault_entries (
            id TEXT PRIMARY KEY,
            category TEXT NOT NULL,
            label TEXT NOT NULL,
            origin_pseudonym TEXT NOT NULL,
            encrypted_value BLOB NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_id TEXT,
            action TEXT NOT NULL,
            context_json TEXT NOT NULL,
            timestamp INTEGER NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_entries_origin ON vault_entries(origin_pseudonym, category, label);
        "#,
    )
    .map_err(|e| VaultError::StorageError(format!("Schema creation failed: {}", e)))?;

    debug!("Vault schema verified");
    Ok(())
}

pub fn store_entry(conn: &Connection, entry: &VaultEntry) -> VaultResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO vault_entries (id, category, label, origin_pseudonym, encrypted_value, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.id.to_string(),
            entry.category.to_string(),
            entry.label,
            entry.origin_pseudonym,
            entry.encrypted_value,
            entry.created_at,
            entry.updated_at
        ],
    )
    .map_err(|e| VaultError::StorageError(format!("Store failed: {}", e)))?;
    Ok(())
}

pub fn retrieve_entry(
    conn: &Connection,
    origin_pseudonym: &str,
    category: &str,
    label: &str,
) -> VaultResult<Option<VaultEntry>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, category, label, origin_pseudonym, encrypted_value, created_at, updated_at
         FROM vault_entries WHERE origin_pseudonym = ?1 AND category = ?2 AND label = ?3",
        )
        .map_err(|e| VaultError::StorageError(format!("Prepare failed: {}", e)))?;

    let entry = stmt
        .query_row(params![origin_pseudonym, category, label], |row| {
            Ok(VaultEntry {
                id: row.get::<_, String>(0)?.parse().unwrap(),
                category: row.get::<_, String>(1)?.parse().unwrap(),
                label: row.get(2)?,
                origin_pseudonym: row.get(3)?,
                encrypted_value: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            })
        })
        .ok();

    Ok(entry)
}

pub fn log_audit(
    conn: &Connection,
    entry_id: Option<&str>,
    action: &str,
    context: &RequestContext,
) -> VaultResult<()> {
    let context_json = serde_json::to_string(context).unwrap_or_else(|_| "{}".to_string());
    let timestamp = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO audit_log (entry_id, action, context_json, timestamp)
         VALUES (?1, ?2, ?3, ?4)",
        params![entry_id, action, context_json, timestamp],
    )
    .map_err(|e| VaultError::StorageError(format!("Audit log failed: {}", e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::VaultCategory;

    fn temp_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        create_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_db_roundtrip() {
        let conn = temp_db();
        let entry = VaultEntry {
            id: uuid::Uuid::new_v4(),
            category: VaultCategory::Password,
            label: "test-label".to_string(),
            origin_pseudonym: "test-origin".to_string(),
            encrypted_value: vec![1, 2, 3],
            created_at: 0,
            updated_at: 0,
        };
        store_entry(&conn, &entry).unwrap();

        let retrieved = retrieve_entry(
            &conn,
            "test-origin",
            &VaultCategory::Password.to_string(),
            "test-label",
        )
        .unwrap()
        .unwrap();
        assert_eq!(retrieved.id, entry.id);
        assert_eq!(retrieved.encrypted_value, entry.encrypted_value);
    }
}
