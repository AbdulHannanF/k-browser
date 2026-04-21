#![allow(warnings)]
// ARCHITECTURE: kitsune-agent is the AI agent runtime.
// Agents are structured, auditable configurations that execute browser
// automation tasks within strict safety constraints.
//
// Key security properties:
// 1. Agents can NEVER bypass HIL for consequential actions
// 2. Agent capabilities are declared in AgentConstraints (not soft instructions)
// 3. Cost accounting is mandatory for all external interactions
// 4. Agents receive opaque tokens from the vault, never raw secrets
// 5. Agent lineage is tracked — sub-agents inherit intersection of parent constraints

pub mod spec;
pub mod runtime;
pub mod tools;
pub mod budget;
pub mod dom_access;
pub mod error;
pub mod executor;

pub use error::{AgentError, AgentResult};
pub use spec::*;
pub use runtime::*;
pub use tools::*;
pub use budget::*;
