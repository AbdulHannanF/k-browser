/// KitsuneEngine — Main Entry Point
///
/// Built in Rust. Grounded in trust. Never leaks.
use eframe::egui;
use kitsune_ui::app::KitsuneBrowser;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    // Initialize CEF early for child processes (must be before window creation)
    if let Err(e) = kitsune_cef::initialize() {
        eprintln!("Failed to initialize CEF: {:?}", e);
        std::process::exit(1);
    }

    // Initialize structured logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(true)
        .init();

    tracing::info!(
        version = kitsune_core::ENGINE_VERSION,
        "Starting KitsuneEngine"
    );

    // Load the embedded kitsune fox icon (256×256 RGBA PNG baked into the binary)
    let icon = load_icon();

    // Configure the native window (decorations off — we draw our own titlebar)
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("KitsuneEngine")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([800.0, 600.0])
        .with_decorations(false);

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(std::sync::Arc::new(icon_data));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    // Launch the UI
    eframe::run_native(
        "KitsuneEngine",
        options,
        Box::new(|cc| Ok(Box::new(KitsuneBrowser::new(cc)))),
    )
}

fn load_icon() -> Option<egui::IconData> {
    const ICON_BYTES: &[u8] = include_bytes!("../assets/kitsune-icon.png");
    let img = image::load_from_memory(ICON_BYTES).ok()?;
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    Some(egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    })
}
