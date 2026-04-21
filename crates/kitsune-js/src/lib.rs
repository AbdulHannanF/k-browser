// ARCHITECTURE: kitsune-js provides the JavaScript engine bridge.
// It wraps a JS engine (V8 via rusty_v8 or SpiderMonkey) with safe Rust abstractions.
// The JS process is heavily sandboxed — no filesystem, no direct IPC to broker.
// All DOM APIs that touch credentials are stubbed and routed through kitsune-ipc.

pub mod engine;
pub mod error;

#[cfg(target_os = "windows")]
#[link(name = "advapi32")]
extern "C" {}

pub use engine::*;
pub use error::{JsError, JsResult};
