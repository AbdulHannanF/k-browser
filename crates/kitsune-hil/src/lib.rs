// ARCHITECTURE: kitsune-hil is the Human-in-the-Loop gate system.
// This is a security-critical crate that ensures no consequential action
// (financial transactions, account creation, credential usage) can be
// executed without explicit human confirmation.
//
// The HIL gate produces non-cloneable, time-limited approval tokens that
// are consumed by the action executor. This makes it architecturally
// impossible to bypass the confirmation step.

pub mod gate;
pub mod trigger;
pub mod approval;
pub mod presentation;
pub mod error;

pub use error::{HilError, HilResult};
pub use gate::HilGate;
pub use trigger::HilTriggerClass;
pub use approval::{HilApproval, ActionId};
pub use presentation::HilPresentation;
