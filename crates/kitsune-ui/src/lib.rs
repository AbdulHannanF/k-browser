// ARCHITECTURE: kitsune-ui is the native desktop host for KitsuneEngine.
// It boots a minimal egui frame with a split-panel layout:
//   - Top: Chrome bar (logo, tabs, navigation, URL bar, privacy pill)
//   - Left: Agent workspace panel (command input, agent cards, log, budget)
//   - Right: Session info panel (status, capabilities, vault)
//   - Center: WebView2 surface rendering real web pages
//   - Overlay: HIL confirmation dialog when an agent needs approval

pub mod animation;
pub mod app;
pub mod chrome;
pub mod dialogs;
pub mod panels;
pub mod theme;
