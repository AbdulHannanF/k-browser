// ARCHITECTURE: kitsune-core is the broker process — the privileged orchestrator.
// It owns the vault, the HIL gate, and the IPC bus. It never touches the
// network directly. All other processes communicate through it.
//
// Process model:
// - Broker (kitsune-core): Privileged. Owns vault, HIL, IPC bus.
// - Renderer(s): Sandboxed. One per tab origin.
// - Network: Sandboxed. All HTTP/HTTPS.
// - Agent: Semi-privileged. Vault access through HIL.
// - JS Engine: Heavily sandboxed. No filesystem, no direct IPC to broker.

pub mod broker;
pub mod config;
pub mod engine;
pub mod navigation;
pub mod tab;

pub use broker::{BrokerEvent, ProcessManager, ProcessStatus};

pub use config::*;
pub use engine::*;
pub use tab::*;

/// The KitsuneEngine version.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The KitsuneEngine name.
pub const ENGINE_NAME: &str = "KitsuneEngine";
