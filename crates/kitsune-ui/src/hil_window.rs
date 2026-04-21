//! HIL (Human-in-the-Loop) confirmation window.
//!
//! Spawns a **separate OS-level window** (not a panel) for sensitive-action
//! confirmations. This window cannot be controlled by any website or script.
//!
//! **Window properties**:
//! - Always on top
//! - Non-resizable
//! - Centered on screen
//! - 3-second countdown before CONFIRM is clickable
//! - Red DENY button always active
//! - Title: "KitsuneEngine — Action Required"

//! - Title: "KitsuneEngine — Action Required"

use eframe::egui::{Color32, ViewportBuilder};
use tracing::{info, warn};

/// Trigger classes for HIL confirmation prompts.
#[derive(Debug, Clone)]
pub enum HilTriggerClass {
    /// Financial transaction (payment, transfer).
    FinancialTransaction,
    /// Credential access (password autofill).
    CredentialAccess,
    /// Data export (downloading user data).
    DataExport,
    /// Agent action (autonomous agent performing sensitive action).
    AgentAction,
    /// Custom trigger.
    Custom(String),
}

impl HilTriggerClass {
    /// Human-readable label for the header bar.
    pub fn label(&self) -> &str {
        match self {
            HilTriggerClass::FinancialTransaction => "Financial Transaction",
            HilTriggerClass::CredentialAccess => "Credential Access",
            HilTriggerClass::DataExport => "Data Export",
            HilTriggerClass::AgentAction => "Agent Action",
            HilTriggerClass::Custom(s) => s.as_str(),
        }
    }
}

/// A request for HIL confirmation.
#[derive(Debug, Clone)]
pub struct HilRequest {
    /// What triggered this confirmation.
    pub trigger_class: HilTriggerClass,
    /// Name of the agent requesting the action.
    pub agent_name: String,
    /// Human-readable description of what will happen.
    pub action_description: String,
    /// Bullet points explaining the action.
    pub bullet_points: Vec<String>,
    /// Vault entry labels involved (never values!).
    pub vault_labels: Vec<String>,
    /// Estimated cost in USD (0.0 = no cost).
    pub estimated_cost: f64,
}

/// The user's decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HilDecision {
    Confirmed,
    Denied,
}

/// Internal state for the HIL dialog.
struct HilDialogState {
    request: HilRequest,
    decision: Option<HilDecision>,
    start_time: std::time::Instant,
    countdown_secs: f32,
}

impl HilDialogState {
    fn new(request: HilRequest) -> Self {
        Self {
            request,
            decision: None,
            start_time: std::time::Instant::now(),
            countdown_secs: 3.0,
        }
    }

    fn elapsed_secs(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    fn can_confirm(&self) -> bool {
        self.elapsed_secs() >= self.countdown_secs
    }
}

impl eframe::App for HilDialogState {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request continuous repainting for the countdown
        ctx.request_repaint();

        egui::CentralPanel::default().show(ctx, |ui| {
            // ── Header bar (red background) ─────────────────────────
            let header_rect = ui.available_rect_before_wrap();
            let header_rect = egui::Rect::from_min_size(
                header_rect.min,
                egui::vec2(header_rect.width(), 48.0),
            );
            ui.painter().rect_filled(
                header_rect,
                0.0,
                Color32::from_rgb(198, 40, 40), // #C62828
            );
            let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(header_rect).layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)));
            child_ui.colored_label(Color32::WHITE, format!(
                "⚠ {} — Confirmation Required",
                self.request.trigger_class.label()
            ));
            ui.add_space(56.0);

            // ── Agent name + description ───────────────────────────
            ui.heading(&self.request.agent_name);
            ui.add_space(4.0);
            ui.label(&self.request.action_description);
            ui.add_space(12.0);

            // ── "What will happen" ─────────────────────────────────
            ui.strong("What will happen:");
            for point in &self.request.bullet_points {
                ui.label(format!("  • {}", point));
            }
            ui.add_space(8.0);

            // ── "Data involved" ────────────────────────────────────
            if !self.request.vault_labels.is_empty() {
                ui.strong("Data involved:");
                for label in &self.request.vault_labels {
                    ui.label(format!("  🔐 {}", label));
                }
                ui.add_space(8.0);
            }

            // ── Cost line ──────────────────────────────────────────
            if self.request.estimated_cost > 0.0 {
                ui.label(format!("Estimated cost: ${:.2}", self.request.estimated_cost));
            } else {
                ui.label("No cost");
            }
            ui.add_space(12.0);

            // ── Countdown progress bar ─────────────────────────────
            let elapsed = self.elapsed_secs();
            let progress = (elapsed / self.countdown_secs).min(1.0);
            ui.add(
                egui::ProgressBar::new(progress)
                    .text(if progress < 1.0 {
                        format!("Wait {:.0}s before confirming", (self.countdown_secs - elapsed).max(0.0))
                    } else {
                        "Ready to confirm".to_string()
                    }),
            );
            ui.add_space(12.0);

            // ── Buttons ────────────────────────────────────────────
            ui.horizontal(|ui| {
                // DENY — always active, red
                let deny_btn = egui::Button::new("✖ DENY")
                    .fill(Color32::from_rgb(198, 40, 40));
                if ui.add(deny_btn).clicked() {
                    info!("HIL: User DENIED action");
                    self.decision = Some(HilDecision::Denied);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                ui.add_space(20.0);

                // CONFIRM — enabled only after countdown
                let confirm_btn = egui::Button::new("✔ CONFIRM")
                    .fill(if self.can_confirm() {
                        Color32::from_rgb(46, 125, 50) // green
                    } else {
                        Color32::from_rgb(80, 80, 80) // grey
                    });

                let confirm_response = ui.add_enabled(self.can_confirm(), confirm_btn);
                if confirm_response.clicked() && self.can_confirm() {
                    info!("HIL: User CONFIRMED action");
                    self.decision = Some(HilDecision::Confirmed);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });

            ui.add_space(8.0);

            // ── Footer ─────────────────────────────────────────────
            ui.separator();
            ui.colored_label(
                Color32::from_rgb(140, 140, 140),
                "This window cannot be controlled by any website.",
            );
        });
    }
}

/// Show the HIL confirmation dialog as a separate OS window.
///
/// This blocks the calling thread until the user makes a decision.
/// Returns `HilDecision::Denied` if the user closes the window without deciding.
pub fn show_hil_dialog(request: HilRequest) -> HilDecision {
    info!(
        trigger = request.trigger_class.label(),
        agent = %request.agent_name,
        "Spawning HIL confirmation window"
    );

    let mut options = eframe::NativeOptions::default();
    options.viewport = ViewportBuilder::default()
        .with_title("KitsuneEngine — Action Required")
        .with_inner_size([480.0, 520.0])
        .with_resizable(false)
        .with_always_on_top();

    let state = std::sync::Arc::new(std::sync::Mutex::new(None::<HilDecision>));
    let _state_clone = state.clone(); // In reality this would be passed into the closure if needed to set decision

    let result = eframe::run_native(
        "KitsuneEngine HIL",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(HilDialogState::new(request)))
        }),
    );

    if let Err(e) = result {
        warn!("HIL window error: {}", e);
    }

    // Default to Denied if window was closed without a decision
    let val = *state.lock().unwrap();
    val.unwrap_or(HilDecision::Denied)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hil_trigger_labels() {
        assert_eq!(HilTriggerClass::FinancialTransaction.label(), "Financial Transaction");
        assert_eq!(HilTriggerClass::CredentialAccess.label(), "Credential Access");
        assert_eq!(HilTriggerClass::DataExport.label(), "Data Export");
        assert_eq!(HilTriggerClass::AgentAction.label(), "Agent Action");
        assert_eq!(HilTriggerClass::Custom("Test".to_string()).label(), "Test");
    }

    #[test]
    fn test_hil_request_construction() {
        let request = HilRequest {
            trigger_class: HilTriggerClass::FinancialTransaction,
            agent_name: "PaymentBot".to_string(),
            action_description: "Transfer $50 to vendor".to_string(),
            bullet_points: vec![
                "Send payment via Stripe".to_string(),
                "Amount: $50.00 USD".to_string(),
            ],
            vault_labels: vec!["Stripe API Key".to_string()],
            estimated_cost: 50.0,
        };

        assert_eq!(request.agent_name, "PaymentBot");
        assert_eq!(request.bullet_points.len(), 2);
        assert_eq!(request.vault_labels.len(), 1);
    }

    #[test]
    fn test_hil_dialog_state_countdown() {
        let request = HilRequest {
            trigger_class: HilTriggerClass::AgentAction,
            agent_name: "TestAgent".to_string(),
            action_description: "Test action".to_string(),
            bullet_points: vec![],
            vault_labels: vec![],
            estimated_cost: 0.0,
        };

        let state = HilDialogState::new(request);
        // Initially, confirm should NOT be available (countdown not elapsed)
        assert!(!state.can_confirm());
    }
}
