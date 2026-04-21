/// Site isolation map — each origin gets a unique, stable pseudonymous identifier.
///
/// Cross-site tracking via shared identifiers is architecturally impossible
/// because the vault never hands the same identifier to two different origins.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use uuid::Uuid;

/// A pseudonymous identifier unique to a (user, origin) pair.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct SiteIdentifier(pub String);

/// Maps website origins to pseudonymous identifiers.
///
/// Each origin gets a unique, deterministic identifier derived from
/// the user's vault key + the origin. This ensures:
/// 1. The same origin always gets the same identifier (stable)
/// 2. Different origins ALWAYS get different identifiers (isolated)
/// 3. The identifier cannot be reversed to recover the vault key
pub struct SiteIsolationMap {
    /// Cache of computed identifiers.
    cache: DashMap<String, SiteIdentifier>,
    /// The user's vault key seed (used for derivation).
    seed: [u8; 32],
}

impl SiteIsolationMap {
    /// Create a new site isolation map with the given seed.
    pub fn new(seed: [u8; 32]) -> Self {
        Self {
            cache: DashMap::new(),
            seed,
        }
    }

    /// Get or create the pseudonymous identifier for an origin.
    pub fn identifier_for_origin(&self, origin: &str) -> SiteIdentifier {
        if let Some(existing) = self.cache.get(origin) {
            return existing.clone();
        }

        let identifier = self.derive_identifier(origin);
        self.cache.insert(origin.to_string(), identifier.clone());
        identifier
    }

    /// Derive a deterministic identifier from the seed + origin.
    fn derive_identifier(&self, origin: &str) -> SiteIdentifier {
        let mut hasher = Sha256::new();
        hasher.update(&self.seed);
        hasher.update(b"kitsune-site-isolation-v1");
        hasher.update(origin.as_bytes());
        let hash = hasher.finalize();

        // Use the hash bytes to create a UUID v5-style identifier
        let hex = hex::encode(&hash[..16]);
        SiteIdentifier(format!("ksi-{}", hex))
    }

    /// Check if an identifier belongs to a specific origin.
    pub fn verify_origin(&self, origin: &str, identifier: &SiteIdentifier) -> bool {
        let expected = self.derive_identifier(origin);
        expected == *identifier
    }

    /// Get all cached origins (for diagnostics/debugging only).
    pub fn cached_origins(&self) -> Vec<String> {
        self.cache.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Clear the cache (e.g., when the user changes their passphrase).
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

impl std::fmt::Debug for SiteIsolationMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SiteIsolationMap")
            .field("cached_origins_count", &self.cache.len())
            .field("seed", &"[REDACTED]")
            .finish()
    }
}

/// Hex encoding utility.
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_seed() -> [u8; 32] {
        let mut seed = [0u8; 32];
        seed[0] = 42;
        seed[1] = 7;
        seed
    }

    #[test]
    fn test_same_origin_same_identifier() {
        let map = SiteIsolationMap::new(test_seed());
        let id1 = map.identifier_for_origin("example.com");
        let id2 = map.identifier_for_origin("example.com");
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_different_origins_different_identifiers() {
        let map = SiteIsolationMap::new(test_seed());
        let id1 = map.identifier_for_origin("example.com");
        let id2 = map.identifier_for_origin("other.com");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_different_seeds_different_identifiers() {
        let seed1 = test_seed();
        let mut seed2 = test_seed();
        seed2[0] = 99;

        let map1 = SiteIsolationMap::new(seed1);
        let map2 = SiteIsolationMap::new(seed2);

        let id1 = map1.identifier_for_origin("example.com");
        let id2 = map2.identifier_for_origin("example.com");
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_verify_origin() {
        let map = SiteIsolationMap::new(test_seed());
        let id = map.identifier_for_origin("example.com");
        assert!(map.verify_origin("example.com", &id));
        assert!(!map.verify_origin("other.com", &id));
    }

    #[test]
    fn test_debug_redacted() {
        let map = SiteIsolationMap::new(test_seed());
        let debug = format!("{:?}", map);
        assert!(debug.contains("[REDACTED]"));
    }
}
