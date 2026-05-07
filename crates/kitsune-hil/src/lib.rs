// ARCHITECTURE: kitsune-hil is the Human-in-the-Loop gate system.
// This is a security-critical crate that ensures no consequential action
// (financial transactions, account creation, credential usage) can be
// executed without explicit human confirmation.
//
// The HIL gate produces non-cloneable, time-limited approval tokens that
// are consumed by the action executor. This makes it architecturally
// impossible to bypass the confirmation step.

pub mod approval;
pub mod error;
pub mod gate;
pub mod presentation;
pub mod trigger;

pub use approval::{ActionId, HilApproval};
pub use error::{HilError, HilResult};
pub use gate::HilGate;
pub use presentation::HilPresentation;
pub use trigger::HilTriggerClass;
