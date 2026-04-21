// ARCHITECTURE: kitsune-ui is the native UI shell for KitsuneEngine.
// It uses egui for cross-platform rendering and provides:
// 1. Privacy Dashboard — vault status, agent activity, fingerprint scores
// 2. Agent Shelf — list of agents with status indicators
// 3. HIL Confirmation Sheet — modal confirmation dialogs
// 4. Vault Manager — view/manage vault entries
// 5. Onboarding Flow — 3-screen introduction for new users

pub mod app;
pub mod theme;
pub mod hil_window;
