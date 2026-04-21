// ARCHITECTURE: kitsune-vault is the most critical crate in KitsuneEngine.
// It is a local-only, encrypted key-value store that NEVER syncs to any
// cloud service by default. The vault enforces disclosure policies at the
// data layer — every entry carries a policy that governs when and how
// it can be disclosed.
//
// Key security properties:
// 1. The vault never returns raw secrets — it returns GrantedAccess tokens
// 2. Each origin gets a unique, stable pseudonymous identifier
// 3. Cross-site tracking via shared identifiers is architecturally impossible
// 4. All access is logged in an audit trail
// 5. At-rest encryption with argon2-derived keys
//
// INVARIANT: If secure enclave key storage fails, the vault REFUSES to
// initialize rather than falling back to unencrypted storage.

pub mod backend;
pub mod types;
pub mod policy;
pub mod access;
pub mod audit;
pub mod crypto;
pub mod site_isolation;
pub mod error;
pub mod db;

pub use error::{VaultError, VaultResult};
pub use types::*;
pub use policy::*;
pub use access::*;
pub use audit::*;
pub use backend::VaultBackend;
pub use site_isolation::SiteIsolationMap;
