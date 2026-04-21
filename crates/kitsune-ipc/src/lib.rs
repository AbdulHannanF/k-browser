// ARCHITECTURE: kitsune-ipc provides the inter-process communication layer between
// the privileged broker process and sandboxed renderer/agent/JS processes.
// All messages are serializable and type-safe. The IPC bus ensures that
// sandboxed processes can never directly access privileged resources —
// they must request them through typed message channels.

pub mod bus;
pub mod channel;
pub mod error;
pub mod message;
pub mod transport;

pub use bus::IpcBus;
pub use channel::IpcChannel;
pub use error::{IpcError, IpcResult};
pub use message::*;
pub use transport::{IpcServer, IpcChannel as TransportChannel};
