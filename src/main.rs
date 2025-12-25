mod app;
mod file_watcher;
mod upload_queue;
mod api_client;
mod upload_manager;
mod ui_theme;

use eframe::egui;
use std::env;

fn main() -> Result<(), eframe::Error> {
    // Force OpenGL backend on macOS to avoid Metal compatibility issues
    env::set_var("wgpu_backend", "gl");

    env_logger::init(); // Initialize logger

    // Load icon
    let icon_data = include_bytes!("../assets/logo_padded.png");
    let icon_image = image::load_from_memory(icon_data)
        .expect("Failed to load icon")
        .to_rgba8();
    let (icon_width, icon_height) = icon_image.dimensions();
    let icon = egui::IconData {
        rgba: icon_image.into_raw(),
        width: icon_width,
        height: icon_height,
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([550.0, 650.0])
            .with_resizable(false)
            .with_decorations(true)   // Restore title bar and borders
            .with_transparent(true)   // Allow transparent background for seamless look
            .with_icon(icon),         // Set application icon
        ..Default::default()
    };

    eframe::run_native(
        "Live Moment Gallery",
        options,
        Box::new(|_cc| {
            // This is where you initialize your app
            Ok(Box::new(app::MacUploaderApp::new()))
        }),
    )
}
