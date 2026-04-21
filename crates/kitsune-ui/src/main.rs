/// KitsuneEngine — Main Entry Point
///
/// Built in Rust. Grounded in trust. Never leaks.

use kitsune_ui::app::KitsuneApp;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .init();

    tracing::info!(
        version = kitsune_core::ENGINE_VERSION,
        "Starting KitsuneEngine"
    );

    // Configure the native window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("KitsuneEngine")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    // Launch the UI
    eframe::run_native(
        "KitsuneEngine",
        options,
        Box::new(|cc| Ok(Box::new(KitsuneApp::new(cc)))),
    )
}
